use bindings::Windows::Win32::System::{
    Diagnostics::Debug::FACILITY_NT_BIT, SystemServices::NTSTATUS,
};
use windows::HRESULT;

pub fn nt_success(status: NTSTATUS) -> bool {
    status.0 >= 0
}

pub fn hresult_from_nt(status: NTSTATUS) -> HRESULT {
    // https://docs.microsoft.com/en-us/windows/win32/api/winerror/nf-winerror-hresult_from_nt
    HRESULT(status.0 as u32 | FACILITY_NT_BIT.0)
}
