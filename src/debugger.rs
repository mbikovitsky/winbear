use bindings::Windows::Win32::System::{
    Diagnostics::Debug::{
        ContinueDebugEvent, WaitForDebugEvent, CREATE_PROCESS_DEBUG_EVENT,
        CREATE_PROCESS_DEBUG_INFO, CREATE_THREAD_DEBUG_EVENT, CREATE_THREAD_DEBUG_INFO,
        DEBUG_EVENT, DEBUG_EVENT_0, EXCEPTION_DEBUG_EVENT, EXCEPTION_DEBUG_INFO,
        EXIT_PROCESS_DEBUG_EVENT, EXIT_PROCESS_DEBUG_INFO, EXIT_THREAD_DEBUG_EVENT,
        EXIT_THREAD_DEBUG_INFO, LOAD_DLL_DEBUG_EVENT, LOAD_DLL_DEBUG_INFO,
        OUTPUT_DEBUG_STRING_EVENT, OUTPUT_DEBUG_STRING_INFO, RIP_EVENT, RIP_INFO,
        UNLOAD_DLL_DEBUG_EVENT, UNLOAD_DLL_DEBUG_INFO,
    },
    SystemServices::{DBG_CONTINUE, DBG_EXCEPTION_NOT_HANDLED, HANDLE},
    WindowsProgramming::{CloseHandle, INFINITE},
};

#[derive(Debug)]
pub struct DebugEvent {
    process_id: u32,
    thread_id: u32,
    info: DebugEventInfo,
}

assert_not_impl_any!(DebugEvent: Send, Sync);

impl DebugEvent {
    pub fn continue_handled(self) -> windows::Result<()> {
        unsafe { ContinueDebugEvent(self.process_id, self.thread_id, DBG_CONTINUE.0 as _).ok() }
    }

    pub fn continue_unhandled(self) -> windows::Result<()> {
        unsafe {
            ContinueDebugEvent(
                self.process_id,
                self.thread_id,
                DBG_EXCEPTION_NOT_HANDLED.0 as _,
            )
            .ok()
        }
    }

    pub fn process_id(&self) -> u32 {
        self.process_id
    }

    pub fn thread_id(&self) -> u32 {
        self.thread_id
    }

    pub fn info(&self) -> &DebugEventInfo {
        &self.info
    }
}

#[derive(Debug)]
pub enum DebugEventInfo {
    Unknown,
    Exception(EXCEPTION_DEBUG_INFO),
    CreateThread(CREATE_THREAD_DEBUG_INFO),
    CreateProcess(CREATE_PROCESS_DEBUG_INFO),
    ExitThread(EXIT_THREAD_DEBUG_INFO),
    ExitProcess(EXIT_PROCESS_DEBUG_INFO),
    LoadDLL(LOAD_DLL_DEBUG_INFO),
    UnloadDLL(UNLOAD_DLL_DEBUG_INFO),
    OutputDebugString(OUTPUT_DEBUG_STRING_INFO),
    RIP(RIP_INFO),
}

impl Drop for DebugEventInfo {
    fn drop(&mut self) {
        match self {
            Self::CreateProcess(info) => {
                if info.hFile != HANDLE(0) {
                    unsafe {
                        CloseHandle(info.hFile).ok().unwrap();
                    }
                }
            }
            Self::LoadDLL(info) => {
                if info.hFile != HANDLE(0) {
                    unsafe {
                        CloseHandle(info.hFile).ok().unwrap();
                    }
                }
            }
            _ => {}
        }
    }
}

pub fn wait_for_debug_event(timeout_ms: Option<u32>) -> windows::Result<DebugEvent> {
    unsafe {
        let mut event = DEBUG_EVENT {
            dwDebugEventCode: CREATE_PROCESS_DEBUG_EVENT,
            dwProcessId: 0,
            dwThreadId: 0,
            u: DEBUG_EVENT_0 {
                CreateProcessInfo: Default::default(),
            },
        };

        WaitForDebugEvent(&mut event, timeout_ms.unwrap_or(INFINITE)).ok()?;

        let info = match event.dwDebugEventCode {
            EXCEPTION_DEBUG_EVENT => DebugEventInfo::Exception(event.u.Exception),
            CREATE_THREAD_DEBUG_EVENT => DebugEventInfo::CreateThread(event.u.CreateThread),
            CREATE_PROCESS_DEBUG_EVENT => DebugEventInfo::CreateProcess(event.u.CreateProcessInfo),
            EXIT_THREAD_DEBUG_EVENT => DebugEventInfo::ExitThread(event.u.ExitThread),
            EXIT_PROCESS_DEBUG_EVENT => DebugEventInfo::ExitProcess(event.u.ExitProcess),
            LOAD_DLL_DEBUG_EVENT => DebugEventInfo::LoadDLL(event.u.LoadDll),
            UNLOAD_DLL_DEBUG_EVENT => DebugEventInfo::UnloadDLL(event.u.UnloadDll),
            OUTPUT_DEBUG_STRING_EVENT => DebugEventInfo::OutputDebugString(event.u.DebugString),
            RIP_EVENT => DebugEventInfo::RIP(event.u.RipInfo),
            _ => DebugEventInfo::Unknown,
        };

        Ok(DebugEvent {
            process_id: event.dwProcessId,
            thread_id: event.dwThreadId,
            info,
        })
    }
}
