fn main() {
    windows::build!(
        Windows::Win32::System::Threading::{
            OpenProcess,
            CreateProcessW,
            GetCurrentProcessId,
            TerminateProcess,
            GetExitCodeProcess,

            DEBUG_PROCESS,
            PROCESS_VM_READ,
            PROCESS_QUERY_INFORMATION,
        },

        Windows::Win32::System::WindowsProgramming::{
            CloseHandle,
            VerSetConditionMask,
            VerifyVersionInfoW,

            INFINITE,
            ProcessBasicInformation,
            VER_MAJORVERSION,
            VER_MINORVERSION,
            VER_SERVICEPACKMAJOR,
        },

        Windows::Win32::System::Diagnostics::Debug::{
            GetLastError,
            WaitForDebugEvent,
            ContinueDebugEvent,
            ReadProcessMemory,

            FACILITY_NT_BIT,

            EXCEPTION_DEBUG_EVENT,
            CREATE_THREAD_DEBUG_EVENT,
            CREATE_PROCESS_DEBUG_EVENT,
            EXIT_THREAD_DEBUG_EVENT,
            EXIT_PROCESS_DEBUG_EVENT,
            LOAD_DLL_DEBUG_EVENT,
            UNLOAD_DLL_DEBUG_EVENT,
            OUTPUT_DEBUG_STRING_EVENT,
            RIP_EVENT,
            ERROR_INSUFFICIENT_BUFFER,
        },

        Windows::Win32::System::SystemServices::{
            NtQueryInformationProcess,
            QueryFullProcessImageNameW,

            DBG_CONTINUE,
            DBG_EXCEPTION_NOT_HANDLED,
            VER_GREATER_EQUAL,
        },

        Windows::Win32::UI::Shell::CommandLineToArgvW,

        Windows::Win32::System::Memory::LocalFree,
    );
}
