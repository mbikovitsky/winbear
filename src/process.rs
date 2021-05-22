/*
   Copyright (C) 2021 by Michael Bikovitksy

   This file is part of winbear.

   winbear is a tool to generate a compilation database for clang tooling.

   winbear is free software: you can redistribute it and/or modify
   it under the terms of the GNU General Public License as published by
   the Free Software Foundation, either version 3 of the License, or
   (at your option) any later version.

   winbear is distributed in the hope that it will be useful,
   but WITHOUT ANY WARRANTY; without even the implied warranty of
   MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
   GNU General Public License for more details.

   You should have received a copy of the GNU General Public License
   along with winbear.  If not, see <https://www.gnu.org/licenses/>.
*/

use std::{
    collections::HashMap, convert::TryInto, error::Error, ffi::OsString, fmt::Debug,
    os::windows::ffi::OsStringExt, path::PathBuf,
};

use bindings::Windows::Win32::System::{
    Diagnostics::Debug::{GetLastError, ReadProcessMemory, ERROR_INSUFFICIENT_BUFFER},
    SystemServices::{
        NtQueryInformationProcess, QueryFullProcessImageNameW, HANDLE, NTSTATUS,
        PROCESS_NAME_FORMAT, PWSTR,
    },
    Threading::{
        CreateProcessW, GetCurrentProcessId, GetExitCodeProcess, OpenProcess, TerminateProcess,
        DEBUG_PROCESS, PROCESS_ACCESS_RIGHTS, PROCESS_CREATION_FLAGS, STARTUPINFOW,
    },
    WindowsProgramming::{CloseHandle, ProcessBasicInformation},
};
use windows::HRESULT;

use util::{hresult_from_nt, is_windows_vista_or_greater, nt_success, quote_argument};

#[derive(Debug)]
pub struct Process {
    handle: HANDLE,
    process_id: u32,
}

impl Process {
    pub fn open(process_id: u32, access: PROCESS_ACCESS_RIGHTS) -> Result<Self, Box<dyn Error>> {
        unsafe {
            let handle = OpenProcess(access, false, process_id);
            if handle.is_null() {
                return Err(windows::Error::from(HRESULT::from_thread()))?;
            }

            Ok(Process { handle, process_id })
        }
    }

    pub fn open_self(access: PROCESS_ACCESS_RIGHTS) -> Result<Self, Box<dyn Error>> {
        Self::open(unsafe { GetCurrentProcessId() }, access)
    }

    pub fn terminate(&self, exit_code: u32) -> Result<(), Box<dyn Error>> {
        unsafe { TerminateProcess(self.handle, exit_code).ok()? }
        Ok(())
    }

    pub fn image_name(&self) -> Result<PathBuf, Box<dyn Error>> {
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
                    let name = OsString::from_wide(name);
                    return Ok(name.into());
                }

                result.resize(result.len() * 2, 0);
            }
        }
    }

    pub fn command_line(&self) -> Result<OsString, Box<dyn Error>> {
        let params = self.native_process_parameters()?;

        Ok(self.read_unicode_string(&params.CommandLine)?)
    }

    pub fn environment(&self) -> Result<EnvironmentBlock, Box<dyn Error>> {
        let params = self.native_process_parameters()?;

        let environment_block_size: usize = params.EnvironmentSize.try_into().unwrap();

        let mut environment_block_chars =
            vec![0u16; environment_block_size / std::mem::size_of::<u16>()];

        unsafe {
            self.read_slice(
                params.Environment.try_into().unwrap(),
                &mut environment_block_chars,
            )?;
        }

        Ok(EnvironmentBlock {
            data: environment_block_chars,
        })
    }

    pub fn current_directory(&self) -> Result<PathBuf, Box<dyn Error>> {
        let params = self.native_process_parameters()?;

        Ok(self
            .read_unicode_string(&params.CurrentDirectory.DosPath)?
            .into())
    }

    fn native_process_parameters(&self) -> Result<RTL_USER_PROCESS_PARAMETERS64, Box<dyn Error>> {
        let peb = self.native_peb()?;

        // The definition of RTL_USER_PROCESS_PARAMETERS64 is valid from Vista only
        assert!(is_windows_vista_or_greater());

        let params: RTL_USER_PROCESS_PARAMETERS64 =
            unsafe { self.read_struct(peb.ProcessParameters.try_into().unwrap())? };

        Ok(params)
    }

    fn native_peb(&self) -> Result<PEB64, Box<dyn Error>> {
        let peb_address = self.native_peb_address()?;

        unsafe { Ok(self.read_struct(peb_address)?) }
    }

    fn native_peb_address(&self) -> Result<usize, Box<dyn Error>> {
        assert_cfg!(
            target_pointer_width = "64",
            "To avoid problems with WOW64, 32-bit builds are disallowed"
        );

        Ok(self.get_process_basic_info()?.PebBaseAddress)
    }

    fn get_process_basic_info(&self) -> Result<PROCESS_BASIC_INFORMATION, Box<dyn Error>> {
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
                return Err(windows::Error::from(hresult_from_nt(status)))?;
            }

            Ok(info)
        }
    }

    pub fn read_memory(
        &self,
        base_address: usize,
        buffer: &mut [u8],
    ) -> Result<(), Box<dyn Error>> {
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

    pub unsafe fn read_struct<T: Copy>(&self, base_address: usize) -> Result<T, Box<dyn Error>> {
        let mut buffer = vec![0; std::mem::size_of::<T>()];

        self.read_memory(base_address, &mut buffer)?;

        let ptr = buffer.as_ptr() as *const T;

        Ok(ptr.read_unaligned())
    }

    unsafe fn read_slice<T: Copy>(
        &self,
        base_address: usize,
        output: &mut [T],
    ) -> Result<(), Box<dyn Error>> {
        let buffer = std::slice::from_raw_parts_mut(
            output.as_mut_ptr() as _,
            output.len() * std::mem::size_of::<T>(),
        );
        return self.read_memory(base_address, buffer);
    }

    fn read_unicode_string(&self, string: &UNICODE_STRING64) -> Result<OsString, Box<dyn Error>> {
        let string_size: usize = string.Length.into();

        let mut string_chars = vec![0u16; string_size / std::mem::size_of::<u16>()];

        unsafe {
            self.read_slice(string.Buffer.try_into().unwrap(), &mut string_chars)?;
        }

        Ok(OsString::from_wide(&string_chars))
    }

    pub fn process_id(&self) -> u32 {
        self.process_id
    }

    pub fn parent_process_id(&self) -> Result<u32, Box<dyn Error>> {
        Ok(self
            .get_process_basic_info()?
            .InheritedFromUniqueProcessId
            .try_into()
            .unwrap())
    }

    pub fn exit_code(&self) -> Result<u32, Box<dyn Error>> {
        unsafe {
            let mut exit_code = 0;
            GetExitCodeProcess(self.handle, &mut exit_code).ok()?;
            Ok(exit_code)
        }
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle).ok().unwrap();
        }
    }
}

#[derive(Clone)]
pub struct EnvironmentBlock {
    data: Vec<u16>,
}

impl EnvironmentBlock {
    pub fn iter(&self) -> impl Iterator<Item = (OsString, OsString)> + '_ {
        self.data
            .split(|c| *c == 0)
            .take_while(|variable_data| !variable_data.is_empty())
            .filter_map(|variable_data| {
                if let Some(separator_position) =
                    variable_data.iter().position(|c| *c == 61 /* = */)
                {
                    let (name, value) = variable_data.split_at(separator_position);
                    let value = &value[1..];
                    Some((OsString::from_wide(name), OsString::from_wide(value)))
                } else {
                    None
                }
            })
    }
}

impl From<EnvironmentBlock> for HashMap<OsString, OsString> {
    fn from(block: EnvironmentBlock) -> Self {
        Self::from(&block)
    }
}

impl<'a> From<&'a EnvironmentBlock> for HashMap<OsString, OsString> {
    fn from(block: &'a EnvironmentBlock) -> Self {
        block.iter().collect()
    }
}

impl Debug for EnvironmentBlock {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let map: HashMap<OsString, OsString> = self.into();
        return map.fmt(f);
    }
}

#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
#[allow(non_snake_case)]
struct PROCESS_BASIC_INFORMATION {
    pub ExitStatus: NTSTATUS,
    pub PebBaseAddress: usize,
    pub AffinityMask: usize,
    pub BasePriority: i32,
    pub UniqueProcessId: usize,
    pub InheritedFromUniqueProcessId: usize,
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
    pub Reserved2: [u64; 5],
    pub CurrentDirectory: CURDIR64,
    pub DllPath: UNICODE_STRING64,
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
struct CURDIR64 {
    pub DosPath: UNICODE_STRING64,
    pub Handle: u64,
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

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        convert::TryInto,
        env,
        ffi::{OsStr, OsString},
        fs,
    };

    use bindings::Windows::Win32::System::Threading::{PROCESS_QUERY_INFORMATION, PROCESS_VM_READ};

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
        let read_env_block: HashMap<OsString, OsString> = read_env_block.into();

        assert_eq!(value, read_env_block.get(OsStr::new(variable)).unwrap());

        process.terminate(-1i32 as _).unwrap();

        env::remove_var(variable);
    }

    #[test]
    fn self_current_directory() {
        let this_process = Process::open_self(PROCESS_VM_READ | PROCESS_QUERY_INFORMATION).unwrap();

        let read_current_directory = this_process.current_directory().unwrap();

        let actual_current_directory = env::current_dir().unwrap();

        assert_eq!(
            fs::canonicalize(actual_current_directory).unwrap(),
            fs::canonicalize(read_current_directory).unwrap()
        );
    }

    #[test]
    fn self_change_current_directory() {
        let original_dir = env::current_dir().unwrap();

        let parent_dir = original_dir.parent().unwrap();

        env::set_current_dir(parent_dir).unwrap();

        let this_process = Process::open_self(PROCESS_VM_READ | PROCESS_QUERY_INFORMATION).unwrap();

        let read_current_directory = this_process.current_directory().unwrap();

        assert_eq!(
            fs::canonicalize(parent_dir).unwrap(),
            fs::canonicalize(read_current_directory).unwrap()
        );

        env::set_current_dir(original_dir).unwrap();
    }

    #[test]
    fn current_directory_same_bitness() {
        test_remote_current_directory("C:\\Windows\\System32\\notepad.exe");
    }

    #[cfg(target_pointer_width = "64")]
    #[test]
    fn current_directory_64_32() {
        test_remote_current_directory("C:\\Windows\\SysWOW64\\notepad.exe");
    }

    fn test_remote_current_directory(command_line: &str) {
        let process = ProcessCreator::new_with_command_line(command_line)
            .create()
            .unwrap();

        let read_current_directory = process.current_directory().unwrap();

        let actual_current_directory = env::current_dir().unwrap();

        assert_eq!(
            fs::canonicalize(actual_current_directory).unwrap(),
            fs::canonicalize(read_current_directory).unwrap()
        );

        process.terminate(-1i32 as _).unwrap();
    }
}
