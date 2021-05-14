fn main() {
    windows::build!(
        Windows::Win32::System::Threading::{
            CreateProcessW,
            DEBUG_PROCESS,
        },

        Windows::Win32::System::WindowsProgramming::{
            CloseHandle,
            INFINITE,
        },

        Windows::Win32::System::Diagnostics::Debug::{
            WaitForDebugEvent,
            ContinueDebugEvent,
            EXCEPTION_DEBUG_EVENT,
            CREATE_THREAD_DEBUG_EVENT,
            CREATE_PROCESS_DEBUG_EVENT,
            EXIT_THREAD_DEBUG_EVENT,
            EXIT_PROCESS_DEBUG_EVENT,
            LOAD_DLL_DEBUG_EVENT,
            UNLOAD_DLL_DEBUG_EVENT,
            OUTPUT_DEBUG_STRING_EVENT,
            RIP_EVENT,
        },

        Windows::Win32::System::SystemServices::{
            DBG_CONTINUE,
            DBG_EXCEPTION_NOT_HANDLED,
        },
    );
}
