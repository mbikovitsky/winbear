use std::convert::TryInto;

use bindings::Windows::Win32::{
    System::{
        Diagnostics::Debug::FACILITY_NT_BIT,
        Memory::LocalFree,
        SystemServices::{NTSTATUS, VER_GREATER_EQUAL},
        WindowsProgramming::{
            VerSetConditionMask, VerifyVersionInfoW, OSVERSIONINFOEXW, VER_MAJORVERSION,
            VER_MINORVERSION, VER_SERVICEPACKMAJOR,
        },
    },
    UI::Shell::CommandLineToArgvW,
};
use widestring::U16CStr;
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

pub fn command_line_to_argv(command_line: impl AsRef<str>) -> windows::Result<Vec<String>> {
    unsafe {
        let mut argc = 0;
        let argv = CommandLineToArgvW(command_line.as_ref(), &mut argc);
        if argv.is_null() {
            return Err(windows::Error::from(HRESULT::from_thread()));
        }

        let argv_slice = std::slice::from_raw_parts(argv, argc.try_into().unwrap());

        let result = argv_slice
            .iter()
            .map(|argument_ptr| U16CStr::from_ptr_str(argument_ptr.0).to_string().unwrap())
            .collect();

        LocalFree(argv as _);

        Ok(result)
    }
}
