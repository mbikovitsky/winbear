fn main() {
    windows::build!(
        Windows::Win32::System::Threading::{
            CreateProcessW,
            TerminateProcess,

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

        Windows::Win32::System::OleAutomation::{
            VariantClear,

            VT_BSTR,

            BSTR,
            VARIANT,
        },

        Windows::Win32::System::Wmi::{
            IWbemLocator,
            IWbemContext,
            IWbemServices,
            IEnumWbemClassObject,
            IWbemClassObject,

            WbemLocator,

            WBEM_INFINITE,
            WBEM_FLAG_CONNECT_USE_MAX_WAIT,
            WBEM_FLAG_RETURN_IMMEDIATELY,
            WBEM_FLAG_FORWARD_ONLY,
        },

        Windows::Win32::System::Com::{
            CoSetProxyBlanket,
            RPC_C_AUTHN_LEVEL_DEFAULT,
            RPC_C_IMP_LEVEL_IMPERSONATE,
            EOAC_DEFAULT,
        },

        Windows::Win32::System::Rpc::{
            RPC_C_AUTHN_DEFAULT,
            RPC_C_AUTHZ_DEFAULT,
        },
    );
}
