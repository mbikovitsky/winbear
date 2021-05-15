use std::{convert::TryInto, error::Error, ffi::c_void};

use bindings::Windows::Win32::System::{
    SystemServices::{NtQueryInformationProcess, HANDLE, NTSTATUS},
    Threading::{
        CreateProcessW, GetCurrentProcessId, OpenProcess, TerminateProcess, DEBUG_PROCESS,
        PROCESS_ACCESS_RIGHTS, PROCESS_CREATION_FLAGS, STARTUPINFOW,
    },
    WindowsProgramming::{CloseHandle, ProcessBasicInformation, PROCESS_BASIC_INFORMATION},
};
use windows::HRESULT;

use crate::util::{hresult_from_nt, nt_success};

#[cfg(target_pointer_width = "32")]
use crate::util::get_ntdll_export;

#[cfg(target_pointer_width = "32")]
use bindings::Windows::Win32::System::SystemServices::{IsWow64Process, FARPROC};

#[cfg(target_pointer_width = "32")]
use bindings::Windows::Win32::System::WindowsProgramming::PROCESSINFOCLASS;

#[cfg(target_pointer_width = "32")]
use bindings::Windows::Win32::System::Threading::GetCurrentProcess;

#[derive(Debug)]
pub struct Process {
    handle: HANDLE,
    process_id: u32,
}

impl Process {
    pub fn is_system_32_bit() -> windows::Result<bool> {
        #[cfg(target_pointer_width = "64")]
        {
            return Ok(false);
        }

        #[cfg(target_pointer_width = "32")]
        unsafe {
            let mut result = false.into();
            IsWow64Process(GetCurrentProcess(), &mut result).ok()?;
            return Ok(!result.as_bool());
        }
    }

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

    pub fn command_line(&self) -> Result<String, Box<dyn Error>> {
        let peb = self.native_peb()?;

        let (command_line_addr, command_line_size) = match peb {
            NativePeb::Peb32(peb) => {
                let params: RTL_USER_PROCESS_PARAMETERS32 =
                    unsafe { self.read_struct(peb.ProcessParameters.into())? };

                (params.CommandLine.Buffer.into(), params.CommandLine.Length)
            }
            NativePeb::Peb64(peb) => {
                let params: RTL_USER_PROCESS_PARAMETERS64 =
                    unsafe { self.read_struct(peb.ProcessParameters.into())? };

                (params.CommandLine.Buffer, params.CommandLine.Length)
            }
        };

        let mut command_line_bytes = vec![0; command_line_size.into()];
        self.read_memory(command_line_addr.into(), &mut command_line_bytes)?;

        let command_line_chars = unsafe {
            std::slice::from_raw_parts::<u16>(
                command_line_bytes.as_ptr() as _,
                command_line_bytes.len() / std::mem::size_of::<u16>(),
            )
        };

        Ok(String::from_utf16(command_line_chars)?)
    }

    fn native_peb(&self) -> windows::Result<NativePeb> {
        let peb_address = self.native_peb_address()?;

        if Self::is_system_32_bit()? {
            unsafe {
                return Ok(NativePeb::Peb32(self.read_struct(peb_address)?));
            }
        } else {
            unsafe {
                return Ok(NativePeb::Peb64(self.read_struct(peb_address)?));
            }
        }
    }

    fn native_peb_address(&self) -> windows::Result<u64> {
        #[cfg(target_pointer_width = "32")]
        {
            lazy_static! {
                static ref WOW64_QUERY_INFO: Option<FN_NtQueryInformationProcess> =
                    get_ntdll_export("NtWow64QueryInformationProcess64").map_or(
                        None,
                        |function| unsafe {
                            Some(
                                std::mem::transmute::<FARPROC, FN_NtQueryInformationProcess>(
                                    function,
                                ),
                            )
                        }
                    );
            }

            if let Some(function) = *WOW64_QUERY_INFO {
                unsafe {
                    let mut info = PROCESS_BASIC_INFORMATION64::default();
                    let mut return_length = 0;
                    let status = function(
                        self.handle,
                        ProcessBasicInformation,
                        &mut info as *mut _ as _,
                        std::mem::size_of_val(&info).try_into().unwrap(),
                        &mut return_length,
                    );
                    if !nt_success(status) {
                        return Err(windows::Error::from(hresult_from_nt(status)));
                    }

                    return Ok(info.PebBaseAddress);
                }
            }
        }

        unsafe {
            let mut info = PROCESS_BASIC_INFORMATION::default();
            let mut return_length = 0;
            let status = NtQueryInformationProcess(
                self.handle,
                ProcessBasicInformation,
                &mut info as *mut _ as _,
                std::mem::size_of_val(&info).try_into().unwrap(),
                &mut return_length,
            );
            if !nt_success(status) {
                return Err(windows::Error::from(hresult_from_nt(status)));
            }

            Ok((info.PebBaseAddress as usize).try_into().unwrap())
        }
    }

    pub fn read_memory(&self, base_address: u64, buffer: &mut [u8]) -> windows::Result<()> {
        lazy_static! {
            static ref READ_FN: ReadVirtualMemory = ReadVirtualMemory::get();
        }

        let status = READ_FN.invoke(self.handle, base_address, buffer);
        if !nt_success(status) {
            return Err(windows::Error::from(hresult_from_nt(status)));
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
    Local(FN_NtReadVirtualMemory),

    #[cfg(target_pointer_width = "32")]
    Native(FN_NtWow64ReadVirtualMemory64),
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
                std::mem::transmute::<FARPROC, FN_NtWow64ReadVirtualMemory64>(read_fn.unwrap());

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
                    if nt_success(status) {
                        assert_eq!(returned, buffer.len());
                    }
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
                    if nt_success(status) {
                        assert_eq!(returned, buffer.len().try_into().unwrap());
                    }
                    return status;
                }
            };
        }
    }
}

enum NativePeb {
    Peb32(PEB32),
    Peb64(PEB64),
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
#[allow(non_snake_case)]
struct PEB32 {
    pub Reserved1: [u8; 2],
    pub BeingDebugged: u8,
    pub Reserved2: [u8; 1],
    pub Reserved3: [u32; 2],
    pub Ldr: u32,
    pub ProcessParameters: u32,
    pub Reserved4: [u32; 3],
    pub AtlThunkSListPtr: u32,
    pub Reserved5: u32,
    pub Reserved6: u32,
    pub Reserved7: u32,
    pub Reserved8: u32,
    pub AtlThunkSListPtr32: u32,
    pub Reserved9: [u32; 45],
    pub Reserved10: [u8; 96],
    pub PostProcessInitRoutine: u32,
    pub Reserved11: [u8; 128],
    pub Reserved12: [u32; 1],
    pub SessionId: u32,
}

impl Default for PEB32 {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

#[derive(Debug, Clone, Copy)]
#[repr(C)]
#[allow(non_snake_case)]
struct PEB64 {
    pub Reserved1: [u8; 2],
    pub BeingDebugged: u8,
    pub Reserved2: [u8; 21],
    pub LoaderData: u64,
    pub ProcessParameters: u64,
    pub Reserved3: [u8; 520],
    pub PostProcessInitRoutine: u64,
    pub Reserved4: [u8; 136],
    pub SessionId: u32,
}

impl Default for PEB64 {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
#[allow(non_snake_case)]
struct RTL_USER_PROCESS_PARAMETERS32 {
    pub Reserved1: [u8; 16],
    pub Reserved2: [u32; 10],
    pub ImagePathName: UNICODE_STRING32,
    pub CommandLine: UNICODE_STRING32,
}

#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
#[allow(non_snake_case)]
struct RTL_USER_PROCESS_PARAMETERS64 {
    pub Reserved1: [u8; 16],
    pub Reserved2: [u64; 10],
    pub ImagePathName: UNICODE_STRING64,
    pub CommandLine: UNICODE_STRING64,
}

#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
#[allow(non_snake_case)]
struct UNICODE_STRING32 {
    pub Length: u16,
    pub MaximumLength: u16,
    pub Buffer: u32,
}

#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
#[allow(non_snake_case)]
struct UNICODE_STRING64 {
    pub Length: u16,
    pub MaximumLength: u16,
    pub Buffer: u64,
}

#[allow(non_camel_case_types)]
type FN_NtReadVirtualMemory = unsafe extern "system" fn(
    ProcessHandle: HANDLE,
    BaseAddress: usize,
    Buffer: *mut c_void,
    BufferLength: usize,
    ReturnLength: *mut usize,
) -> NTSTATUS;

#[cfg(target_pointer_width = "32")]
#[allow(non_camel_case_types)]
type FN_NtWow64ReadVirtualMemory64 = unsafe extern "system" fn(
    ProcessHandle: HANDLE,
    BaseAddress: u64,
    Buffer: *mut c_void,
    BufferLength: u64,
    ReturnLength: *mut u64,
) -> NTSTATUS;

#[cfg(target_pointer_width = "32")]
#[allow(non_camel_case_types)]
type FN_NtQueryInformationProcess = unsafe extern "system" fn(
    ProcessHandle: HANDLE,
    ProcessInformationClass: PROCESSINFOCLASS,
    ProcessInformation: *mut c_void,
    ProcessInformationLength: u32,
    ReturnLength: *mut u32,
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

#[cfg(target_pointer_width = "32")]
#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
#[allow(non_snake_case)]
struct PROCESS_BASIC_INFORMATION64 {
    pub ExitStatus: NTSTATUS,
    pub PebBaseAddress: u64,
    pub AffinityMask: u64,
    pub BasePriority: u32,
    pub UniqueProcessId: u64,
    pub InheritedFromUniqueProcessId: u64,
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

    #[test]
    fn command_line_same_bitness() {
        test_command_line("C:\\Windows\\System32\\notepad.exe")
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn command_line_64_32() {
        test_command_line("C:\\Windows\\SysWOW64\\notepad.exe")
    }

    #[cfg(target_pointer_width = "32")]
    #[test]
    fn command_line_32_64() {
        test_command_line("C:\\Windows\\SysNative\\notepad.exe")
    }

    fn test_command_line(command_line: &str) {
        let process = ProcessCreator::new_with_command_line(command_line)
            .create()
            .unwrap();

        let read_command_line = process.command_line().unwrap();

        assert_eq!(read_command_line, command_line);

        process.terminate(-1 as _).unwrap();
    }
}
