use std::{
    convert::TryInto,
    error::Error,
    ffi::{OsStr, OsString},
};

use bindings::Windows::Win32::{
    System::{
        Diagnostics::Debug::FACILITY_NT_BIT,
        Memory::LocalFree,
        SystemServices::{NTSTATUS, PWSTR, VER_GREATER_EQUAL},
        WindowsProgramming::{
            VerSetConditionMask, VerifyVersionInfoW, OSVERSIONINFOEXW, VER_MAJORVERSION,
            VER_MINORVERSION, VER_SERVICEPACKMAJOR,
        },
    },
    UI::Shell::CommandLineToArgvW,
};
use widestring::{U16CStr, U16CString};
use windows::HRESULT;

pub fn nt_success(status: NTSTATUS) -> bool {
    status.0 >= 0
}

pub fn hresult_from_nt(status: NTSTATUS) -> HRESULT {
    // https://docs.microsoft.com/en-us/windows/win32/api/winerror/nf-winerror-hresult_from_nt
    HRESULT(status.0 as u32 | FACILITY_NT_BIT.0)
}

pub fn is_windows_vista_or_greater() -> bool {
    is_windows_version_or_greater(6, 0, 0)
}

fn is_windows_version_or_greater(
    major_version: u16,
    minor_version: u16,
    service_pack_major: u16,
) -> bool {
    unsafe {
        let mut version_info = OSVERSIONINFOEXW {
            dwOSVersionInfoSize: std::mem::size_of::<OSVERSIONINFOEXW>().try_into().unwrap(),
            dwMajorVersion: major_version.into(),
            dwMinorVersion: minor_version.into(),
            wServicePackMajor: service_pack_major,
            ..Default::default()
        };

        let ver_greater_equal = VER_GREATER_EQUAL.try_into().unwrap();

        let mask = VerSetConditionMask(0, VER_MAJORVERSION, ver_greater_equal);
        let mask = VerSetConditionMask(mask, VER_MINORVERSION, ver_greater_equal);
        let mask = VerSetConditionMask(mask, VER_SERVICEPACKMAJOR, ver_greater_equal);

        VerifyVersionInfoW(
            &mut version_info,
            VER_MAJORVERSION | VER_MINORVERSION | VER_SERVICEPACKMAJOR,
            mask,
        )
        .as_bool()
    }
}

pub fn command_line_to_argv(
    command_line: impl AsRef<OsStr>,
) -> Result<Vec<OsString>, Box<dyn Error>> {
    unsafe {
        let command_line = U16CString::from_os_str(command_line)?;
        let mut command_line = command_line.into_vec_with_nul();

        let mut argc = 0;
        let argv = CommandLineToArgvW(PWSTR(command_line.as_mut_ptr()), &mut argc);
        if argv.is_null() {
            return Err(windows::Error::from(HRESULT::from_thread()))?;
        }

        let result = {
            let argv_slice = std::slice::from_raw_parts(argv, argc.try_into().unwrap());
            argv_slice
                .iter()
                .map(|argument_ptr| U16CStr::from_ptr_str(argument_ptr.0).to_os_string())
                .collect()
        };

        LocalFree(argv as _);

        Ok(result)
    }
}
