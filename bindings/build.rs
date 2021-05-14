fn main() {
    windows::build!(
        Windows::Win32::System::Threading::{
            CreateProcessW,
            DEBUG_PROCESS,
        },

        Windows::Win32::System::WindowsProgramming::CloseHandle,
    );
}
