fn main() {
    windows::build!(
        Windows::Win32::System::Threading::{
            OpenProcess,
            CreateProcessW,
            GetCurrentProcessId,
            GetCurrentProcess,
            TerminateProcess,

            DEBUG_PROCESS,
            PROCESS_VM_READ,
        },

        Windows::Win32::System::WindowsProgramming::{
            CloseHandle,

            INFINITE,
            ProcessBasicInformation,

            PROCESS_BASIC_INFORMATION,
            PROCESSINFOCLASS,
        },

        Windows::Win32::System::Diagnostics::Debug::{
            GetLastError,
            WaitForDebugEvent,
            ContinueDebugEvent,

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
            GetModuleHandleW,
            GetProcAddress,
            NtQueryInformationProcess,
            QueryFullProcessImageNameW,
            IsWow64Process,

            DBG_CONTINUE,
            DBG_EXCEPTION_NOT_HANDLED,

            NTSTATUS,
        },
    );
}
