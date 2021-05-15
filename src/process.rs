use std::{convert::TryInto, error::Error, ffi::c_void};

use bindings::Windows::Win32::System::{
    SystemServices::{HANDLE, NTSTATUS},
    Threading::{
        CreateProcessW, GetCurrentProcessId, OpenProcess, TerminateProcess, DEBUG_PROCESS,
        PROCESS_ACCESS_RIGHTS, PROCESS_CREATION_FLAGS, STARTUPINFOW,
    },
    WindowsProgramming::CloseHandle,
};
use windows::HRESULT;

use crate::util::{HRESULT_FROM_NT, NT_SUCCESS};

#[cfg(target_pointer_width = "32")]
use crate::util::get_ntdll_export;

#[cfg(target_pointer_width = "32")]
use bindings::Windows::Win32::System::SystemServices::FARPROC;

#[derive(Debug)]
pub struct Process {
    handle: HANDLE,
    process_id: u32,
}

impl Process {
    pub fn open(process_id: u32, access: PROCESS_ACCESS_RIGHTS) -> windows::Result<Self> {
        unsafe {
            let handle = OpenProcess(access, false, process_id);
            if handle.is_null() {
                return Err(windows::Error::from(HRESULT::from_thread()));
            }

            Ok(Process { handle, process_id })
        }
    }

    pub fn open_self(access: PROCESS_ACCESS_RIGHTS) -> windows::Result<Self> {
        Self::open(unsafe { GetCurrentProcessId() }, access)
    }

    pub fn terminate(&self, exit_code: u32) -> windows::Result<()> {
        unsafe { TerminateProcess(self.handle, exit_code).ok() }
    }

    pub fn read_memory(&self, base_address: u64, buffer: &mut [u8]) -> windows::Result<()> {
        lazy_static! {
            static ref READ_FN: ReadVirtualMemory = ReadVirtualMemory::get();
        }

        let status = READ_FN.invoke(self.handle, base_address, buffer);
        if !NT_SUCCESS(status) {
            return Err(windows::Error::from(HRESULT_FROM_NT(status)));
        }

        Ok(())
    }

    pub unsafe fn read_struct<T: Copy>(&self, base_address: u64) -> windows::Result<T> {
        let mut buffer = vec![0; std::mem::size_of::<T>()];

        self.read_memory(base_address, &mut buffer)?;

        let ptr = buffer.as_ptr() as *const T;

        Ok(ptr.read_unaligned())
    }

    pub fn process_id(&self) -> u32 {
        self.process_id
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle).ok().unwrap();
        }
    }
}

#[derive(Clone, Copy)]
enum ReadVirtualMemory {
    Local(PFN_NtReadVirtualMemory),

    #[cfg(target_pointer_width = "32")]
    Native(PFN_NtWow64ReadVirtualMemory64),
}

impl ReadVirtualMemory {
    #[cfg(target_pointer_width = "32")]
    fn get() -> Self {
        unsafe {
            let read_fn = get_ntdll_export("NtWow64ReadVirtualMemory64");
            if read_fn.is_err() {
                return ReadVirtualMemory::Local(NtReadVirtualMemory);
            }

            let read_fn =
                std::mem::transmute::<FARPROC, PFN_NtWow64ReadVirtualMemory64>(read_fn.unwrap());

            Self::Native(read_fn)
        }
    }

    #[cfg(target_pointer_width = "64")]
    fn get() -> Self {
        ReadVirtualMemory::Local(NtReadVirtualMemory)
    }

    fn invoke(&self, process_handle: HANDLE, base_address: u64, buffer: &mut [u8]) -> NTSTATUS {
        unsafe {
            match self {
                ReadVirtualMemory::Local(local) => {
                    let mut returned = 0;
                    let status = local(
                        process_handle,
                        base_address.try_into().unwrap(),
                        buffer.as_mut_ptr() as _,
                        buffer.len(),
                        &mut returned,
                    );
                    assert_eq!(returned, buffer.len());
                    return status;
                }
                #[cfg(target_pointer_width = "32")]
                ReadVirtualMemory::Native(native) => {
                    let mut returned = 0;
                    let status = native(
                        process_handle,
                        base_address,
                        buffer.as_mut_ptr() as _,
                        buffer.len().try_into().unwrap(),
                        &mut returned,
                    );
                    assert_eq!(returned, buffer.len().try_into().unwrap());
                    return status;
                }
            };
        }
    }
}

type PFN_NtReadVirtualMemory = unsafe extern "system" fn(
    ProcessHandle: HANDLE,
    BaseAddress: usize,
    Buffer: *mut c_void,
    BufferLength: usize,
    ReturnLength: *mut usize,
) -> NTSTATUS;

#[cfg(target_pointer_width = "32")]
type PFN_NtWow64ReadVirtualMemory64 = unsafe extern "system" fn(
    ProcessHandle: HANDLE,
    BaseAddress: u64,
    Buffer: *mut c_void,
    BufferLength: u64,
    ReturnLength: *mut u64,
) -> NTSTATUS;

extern "system" {
    #[link(name = "ntdll")]
    fn NtReadVirtualMemory(
        ProcessHandle: HANDLE,
        BaseAddress: usize,
        Buffer: *mut c_void,
        BufferLength: usize,
        ReturnLength: *mut usize,
    ) -> NTSTATUS;
}

#[derive(Debug, Clone)]
pub struct ProcessCreator {
    command_line: String,
    inherit_handles: bool,
    flags: PROCESS_CREATION_FLAGS,
    current_directory: Option<String>,
}

impl ProcessCreator {
    pub fn new_with_command_line(command_line: impl AsRef<str>) -> Self {
        Self::new(command_line.as_ref().to_string())
    }

    pub fn new_with_arguments<I, A>(arguments: I, force_quote: bool) -> Self
    where
        I: IntoIterator<Item = A>,
        A: AsRef<str>,
    {
        let arguments: Vec<_> = arguments
            .into_iter()
            .map(|argument| quote_argument(argument, force_quote))
            .collect();

        let command_line = arguments.join(" ");

        Self::new(command_line)
    }

    fn new(command_line: String) -> Self {
        Self {
            command_line,
            inherit_handles: false,
            flags: Default::default(),
            current_directory: None,
        }
    }

    pub fn inherit_handles(mut self, value: bool) -> ProcessCreator {
        self.inherit_handles = value;
        self
    }

    pub fn debug(mut self, value: bool) -> ProcessCreator {
        if value {
            self.flags |= DEBUG_PROCESS;
        } else {
            self.flags = PROCESS_CREATION_FLAGS(self.flags.0 & !DEBUG_PROCESS.0);
        }
        self
    }

    pub fn current_directory(mut self, value: Option<&str>) -> ProcessCreator {
        self.current_directory = value.and_then(|value| Some(value.to_string()));
        self
    }

    pub fn create(&self) -> Result<Process, Box<dyn Error>> {
        unsafe {
            let mut startup_info = STARTUPINFOW {
                cb: std::mem::size_of::<STARTUPINFOW>().try_into().unwrap(),
                ..Default::default()
            };

            let mut process_info = Default::default();

            let success = match &self.current_directory {
                Some(current_directory) => CreateProcessW(
                    None,
                    self.command_line.to_string(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    self.inherit_handles,
                    self.flags,
                    std::ptr::null_mut(),
                    current_directory.as_str(),
                    &mut startup_info,
                    &mut process_info,
                ),
                None => CreateProcessW(
                    None,
                    self.command_line.to_string(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    self.inherit_handles,
                    self.flags,
                    std::ptr::null_mut(),
                    None,
                    &mut startup_info,
                    &mut process_info,
                ),
            };
            success.ok()?;

            CloseHandle(process_info.hThread).ok().unwrap();

            Ok(Process {
                handle: process_info.hProcess,
                process_id: process_info.dwProcessId,
            })
        }
    }
}

fn quote_argument(argument: impl AsRef<str>, force: bool) -> String {
    let argument = argument.as_ref();

    // Adapted from:
    // https://docs.microsoft.com/en-us/archive/blogs/twistylittlepassagesallalike/everyone-quotes-command-line-arguments-the-wrong-way

    //
    // Unless we're told otherwise, don't quote unless we actually
    // need to do so --- hopefully avoid problems if programs won't
    // parse quotes properly
    //
    if !force && !argument.is_empty() && !argument.contains(|c: char| c.is_whitespace() || c == '"')
    {
        return argument.to_string();
    }

    let mut result = vec!['"'];

    let mut iterator = argument.chars().peekable();
    loop {
        let mut number_backslashes = 0usize;
        while let Some('\\') = iterator.peek() {
            iterator.next();
            number_backslashes += 1;
        }

        match iterator.next() {
            Some(char) => {
                if char == '"' {
                    //
                    // Escape all backslashes and the following
                    // double quotation mark.
                    //
                    append_copies(&mut result, '\\', number_backslashes * 2 + 1);
                    result.push(char);
                } else {
                    //
                    // Backslashes aren't special here.
                    //
                    append_copies(&mut result, '\\', number_backslashes);
                    result.push(char);
                }
            }
            None => {
                //
                // Escape all backslashes, but let the terminating
                // double quotation mark we add below be interpreted
                // as a metacharacter.
                //
                append_copies(&mut result, '\\', number_backslashes * 2);
                break;
            }
        }
    }

    result.push('"');

    result.into_iter().collect()
}

fn append_copies<T: Copy>(vector: &mut Vec<T>, value: T, count: usize) {
    vector.reserve(count);
    for _ in 0..count {
        vector.push(value);
    }
}

#[cfg(test)]
mod tests {
    use bindings::Windows::Win32::System::Threading::PROCESS_VM_READ;

    use super::Process;
    use super::ProcessCreator;

    // https://www.geoffchappell.com/studies/windows/km/ntoskrnl/inc/api/ntexapi_x/kuser_shared_data/index.htm
    const MM_SHARED_USER_DATA_VA: u32 = 0x7FFE0000;
    const NT_SYSTEM_ROOT_OFFSET: u32 = 0x30;

    #[test]
    fn read_memory_same_bitness() {
        let this_process = Process::open_self(PROCESS_VM_READ).unwrap();

        let number = 42u8;

        let number_ptr: *const u8 = &number;
        let mut bytes = [0; std::mem::size_of::<u8>()];
        this_process
            .read_memory(number_ptr as _, &mut bytes)
            .unwrap();

        assert_eq!(1, bytes.len());
        assert_eq!(42, bytes[0]);
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn read_memory_64_32() {
        read_nt_system_root("C:\\Windows\\SysWOW64\\notepad.exe");
    }

    #[cfg(target_pointer_width = "32")]
    #[test]
    fn read_memory_32_64() {
        read_nt_system_root("C:\\Windows\\SysNative\\notepad.exe");
    }

    fn read_nt_system_root(command_line: &str) {
        let process = ProcessCreator::new_with_command_line(command_line)
            .create()
            .unwrap();

        let nt_system_root: [u16; 0x104] = unsafe {
            process
                .read_struct((MM_SHARED_USER_DATA_VA + NT_SYSTEM_ROOT_OFFSET).into())
                .unwrap()
        };
        let nt_system_root = String::from_utf16(&nt_system_root).unwrap();
        let nt_system_root = nt_system_root.trim_end_matches('\0');
        assert_eq!("C:\\Windows".to_lowercase(), nt_system_root.to_lowercase());

        process.terminate(-1 as _).unwrap();
    }
}
