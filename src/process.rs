use std::{collections::HashMap, convert::TryInto, error::Error};

use bindings::Windows::Win32::System::{
    Diagnostics::Debug::{GetLastError, ReadProcessMemory, ERROR_INSUFFICIENT_BUFFER},
    SystemServices::{
        NtQueryInformationProcess, QueryFullProcessImageNameW, HANDLE, PROCESS_NAME_FORMAT, PWSTR,
    },
    Threading::{
        CreateProcessW, GetCurrentProcessId, OpenProcess, TerminateProcess, DEBUG_PROCESS,
        PROCESS_ACCESS_RIGHTS, PROCESS_CREATION_FLAGS, STARTUPINFOW,
    },
    WindowsProgramming::{CloseHandle, ProcessBasicInformation, PROCESS_BASIC_INFORMATION},
};
use windows::HRESULT;

use crate::util::{hresult_from_nt, is_windows_vista_or_greater, nt_success};

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

    pub fn image_name(&self) -> Result<String, Box<dyn Error>> {
        let mut result = vec![0];
        loop {
            unsafe {
                let mut size: u32 = result.len().try_into().unwrap();
                let success = QueryFullProcessImageNameW(
                    self.handle,
                    PROCESS_NAME_FORMAT(0),
                    PWSTR(result.as_mut_ptr()),
                    &mut size,
                );
                let error = GetLastError();
                if !success.as_bool() && error != ERROR_INSUFFICIENT_BUFFER {
                    return Err(windows::Error::from(HRESULT::from_win32(error.0)))?;
                }

                if success.as_bool() {
                    let name = result.as_slice();
                    let name = &name[..size as usize];
                    let name = String::from_utf16(name)?;
                    return Ok(name);
                }

                result.resize(result.len() * 2, 0);
            }
        }
    }

    pub fn command_line(&self) -> Result<String, Box<dyn Error>> {
        let params = self.native_process_parameters()?;

        let mut command_line_bytes = vec![0; params.CommandLine.Length.into()];
        self.read_memory(
            params.CommandLine.Buffer.try_into().unwrap(),
            &mut command_line_bytes,
        )?;

        let command_line_chars = unsafe {
            std::slice::from_raw_parts::<u16>(
                command_line_bytes.as_ptr() as _,
                command_line_bytes.len() / std::mem::size_of::<u16>(),
            )
        };

        Ok(String::from_utf16(command_line_chars)?)
    }

    pub fn environment(&self) -> Result<HashMap<String, String>, Box<dyn Error>> {
        let params = self.native_process_parameters()?;

        let environment_block = unsafe {
            let mut environment_block_bytes = vec![0; params.EnvironmentSize.try_into().unwrap()];
            self.read_memory(
                params.Environment.try_into().unwrap(),
                &mut environment_block_bytes,
            )?;

            let environment_block_slice: &[u16] = std::slice::from_raw_parts(
                environment_block_bytes.as_ptr() as _,
                environment_block_bytes.len() / std::mem::size_of::<u16>(),
            );

            String::from_utf16(environment_block_slice)?
        };

        let environment = environment_block
            .split('\0')
            .take_while(|pair_string| !pair_string.is_empty())
            .filter_map(|pair_string| pair_string.split_once('='))
            .map(|(name, value)| (name.to_string(), value.to_string()))
            .collect();

        Ok(environment)
    }

    fn native_process_parameters(&self) -> Result<RTL_USER_PROCESS_PARAMETERS64, Box<dyn Error>> {
        let peb = self.native_peb()?;

        // The definition of RTL_USER_PROCESS_PARAMETERS64 is valid from Vista only
        assert!(is_windows_vista_or_greater());

        let params: RTL_USER_PROCESS_PARAMETERS64 =
            unsafe { self.read_struct(peb.ProcessParameters.try_into().unwrap())? };

        Ok(params)
    }

    fn native_peb(&self) -> windows::Result<PEB64> {
        let peb_address = self.native_peb_address()?;

        unsafe { Ok(self.read_struct(peb_address)?) }
    }

    fn native_peb_address(&self) -> windows::Result<usize> {
        assert_cfg!(
            target_pointer_width = "64",
            "To avoid problems with WOW64, 32-bit builds are disallowed"
        );

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

            Ok(info.PebBaseAddress as usize)
        }
    }

    pub fn read_memory(&self, base_address: usize, buffer: &mut [u8]) -> windows::Result<()> {
        unsafe {
            let mut bytes_read = 0;
            ReadProcessMemory(
                self.handle,
                base_address as _,
                buffer.as_mut_ptr() as _,
                buffer.len(),
                &mut bytes_read,
            )
            .ok()?;
            assert_eq!(bytes_read, buffer.len());
        }

        Ok(())
    }

    pub unsafe fn read_struct<T: Copy>(&self, base_address: usize) -> windows::Result<T> {
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

#[derive(Debug, Clone, Copy)]
#[repr(C)]
#[allow(non_snake_case)]
struct RTL_USER_PROCESS_PARAMETERS64 {
    pub Reserved1: [u8; 16],
    pub Reserved2: [u64; 10],
    pub ImagePathName: UNICODE_STRING64,
    pub CommandLine: UNICODE_STRING64,
    pub Environment: u64,
    pub Reserved3: [u8; 0x368],
    pub EnvironmentSize: u64,
}

impl Default for RTL_USER_PROCESS_PARAMETERS64 {
    fn default() -> Self {
        unsafe { std::mem::zeroed() }
    }
}

#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
#[allow(non_snake_case)]
struct UNICODE_STRING64 {
    pub Length: u16,
    pub MaximumLength: u16,
    pub Buffer: u64,
}

#[derive(Debug, Clone)]
pub struct ProcessCreator {
    command_line: String,
    flags: PROCESS_CREATION_FLAGS,
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
            flags: Default::default(),
        }
    }

    pub fn debug(mut self, value: bool) -> ProcessCreator {
        if value {
            self.flags |= DEBUG_PROCESS;
        } else {
            self.flags = PROCESS_CREATION_FLAGS(self.flags.0 & !DEBUG_PROCESS.0);
        }
        self
    }

    pub fn create(&self) -> Result<Process, Box<dyn Error>> {
        unsafe {
            let mut startup_info = STARTUPINFOW {
                cb: std::mem::size_of::<STARTUPINFOW>().try_into().unwrap(),
                ..Default::default()
            };

            let mut process_info = Default::default();

            CreateProcessW(
                None,
                self.command_line.to_string(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                false,
                self.flags,
                std::ptr::null_mut(),
                None,
                &mut startup_info,
                &mut process_info,
            )
            .ok()?;

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
    use std::{convert::TryInto, env};

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

    fn read_nt_system_root(command_line: &str) {
        let process = ProcessCreator::new_with_command_line(command_line)
            .create()
            .unwrap();

        let nt_system_root: [u16; 0x104] = unsafe {
            process
                .read_struct(
                    (MM_SHARED_USER_DATA_VA + NT_SYSTEM_ROOT_OFFSET)
                        .try_into()
                        .unwrap(),
                )
                .unwrap()
        };
        let nt_system_root = String::from_utf16(&nt_system_root).unwrap();
        let nt_system_root = nt_system_root.trim_end_matches('\0');
        assert_eq!("C:\\Windows".to_lowercase(), nt_system_root.to_lowercase());

        process.terminate(-1i32 as _).unwrap();
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

    fn test_command_line(command_line: &str) {
        let process = ProcessCreator::new_with_command_line(command_line)
            .create()
            .unwrap();

        let read_command_line = process.command_line().unwrap();

        assert_eq!(read_command_line, command_line);

        process.terminate(-1i32 as _).unwrap();
    }

    #[test]
    fn environment_same_bitness() {
        test_environment_block("C:\\Windows\\System32\\notepad.exe", "PASTEN", "PASTEN");
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn environment_64_32() {
        test_environment_block("C:\\Windows\\SysWOW64\\notepad.exe", "PASTEN", "PASTEN");
    }

    fn test_environment_block(command_line: &str, variable: &str, value: &str) {
        env::set_var(variable, value);

        let process = ProcessCreator::new_with_command_line(command_line)
            .create()
            .unwrap();

        let read_env_block = process.environment().unwrap();

        assert_eq!(value, read_env_block.get(variable).unwrap());

        process.terminate(-1i32 as _).unwrap();

        env::remove_var(variable);
    }
}
