use std::convert::TryInto;

use bindings::Windows::Win32::System::{
    Diagnostics::Debug::FACILITY_NT_BIT,
    SystemServices::{NTSTATUS, VER_GREATER_EQUAL},
    WindowsProgramming::{
        VerSetConditionMask, VerifyVersionInfoW, OSVERSIONINFOEXW, VER_MAJORVERSION,
        VER_MINORVERSION, VER_SERVICEPACKMAJOR,
    },
};
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
