/*
   Copyright (C) 2021 by Michael Bikovitksy

   This file is part of winbear.

   winbear is a tool to generate a compilation database for clang tooling.

   winbear is free software: you can redistribute it and/or modify
   it under the terms of the GNU General Public License as published by
   the Free Software Foundation, either version 3 of the License, or
   (at your option) any later version.

   winbear is distributed in the hope that it will be useful,
   but WITHOUT ANY WARRANTY; without even the implied warranty of
   MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
   GNU General Public License for more details.

   You should have received a copy of the GNU General Public License
   along with winbear.  If not, see <https://www.gnu.org/licenses/>.
*/

use std::collections::HashSet;

use bindings::Windows::Win32::System::{
    Diagnostics::Debug::{
        ContinueDebugEvent, DebugActiveProcessStop, WaitForDebugEvent, CREATE_PROCESS_DEBUG_EVENT,
        CREATE_PROCESS_DEBUG_INFO, CREATE_THREAD_DEBUG_EVENT, CREATE_THREAD_DEBUG_INFO,
        EXCEPTION_DEBUG_EVENT, EXCEPTION_DEBUG_INFO, EXIT_PROCESS_DEBUG_EVENT,
        EXIT_PROCESS_DEBUG_INFO, EXIT_THREAD_DEBUG_EVENT, EXIT_THREAD_DEBUG_INFO,
        LOAD_DLL_DEBUG_EVENT, LOAD_DLL_DEBUG_INFO, OUTPUT_DEBUG_STRING_EVENT,
        OUTPUT_DEBUG_STRING_INFO, RIP_EVENT, RIP_INFO, UNLOAD_DLL_DEBUG_EVENT,
        UNLOAD_DLL_DEBUG_INFO,
    },
    SystemServices::{DBG_CONTINUE, DBG_EXCEPTION_NOT_HANDLED, HANDLE},
    WindowsProgramming::{CloseHandle, INFINITE},
};

pub trait DebugEventHandler {
    fn handle_event(&mut self, event: &DebugEvent) -> DebugEventResponse;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DebugEventResponse {
    Continue(ExceptionContinuation),
    ExitDetach(ExceptionContinuation),
    Exit,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExceptionContinuation {
    Handled,
    NotHandled,
}

#[derive(Debug)]
pub struct DebugEvent {
    process_id: u32,
    thread_id: u32,
    info: DebugEventInfo,
}

assert_not_impl_any!(DebugEvent: Send, Sync);

impl DebugEvent {
    fn continue_event(self, handled: bool) -> windows::Result<()> {
        unsafe {
            ContinueDebugEvent(
                self.process_id,
                self.thread_id,
                if handled {
                    DBG_CONTINUE.0 as _
                } else {
                    DBG_EXCEPTION_NOT_HANDLED.0 as _
                },
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
                if info.hFile != HANDLE::NULL {
                    unsafe {
                        CloseHandle(info.hFile).ok().unwrap();
                    }
                }
            }
            Self::LoadDLL(info) => {
                if info.hFile != HANDLE::NULL {
                    unsafe {
                        CloseHandle(info.hFile).ok().unwrap();
                    }
                }
            }
            _ => {}
        }
    }
}

pub fn run_debug_loop(
    handler: &mut impl DebugEventHandler,
    timeout_ms: Option<u32>,
) -> windows::Result<()> {
    let mut debugged_processes = HashSet::new();

    loop {
        let debug_event = wait_for_debug_event(timeout_ms)?;

        match debug_event.info() {
            DebugEventInfo::CreateProcess(_) => {
                let inserted = debugged_processes.insert(debug_event.process_id());
                assert!(inserted);
            }
            DebugEventInfo::ExitProcess(_) => {
                let removed = debugged_processes.remove(&debug_event.process_id());
                assert!(removed);
            }
            _ => {}
        }

        match handler.handle_event(&debug_event) {
            DebugEventResponse::Continue(ExceptionContinuation::Handled) => {
                debug_event.continue_event(true)?;
            }
            DebugEventResponse::Continue(ExceptionContinuation::NotHandled) => {
                debug_event.continue_event(false)?;
            }
            DebugEventResponse::ExitDetach(continuation) => {
                match continuation {
                    ExceptionContinuation::Handled => debug_event.continue_event(true)?,
                    ExceptionContinuation::NotHandled => debug_event.continue_event(false)?,
                }
                for pid in debugged_processes {
                    let result = unsafe { DebugActiveProcessStop(pid).ok() };
                    if let Err(error) = result {
                        // TODO: better logging
                        eprintln!("{}", error);
                    }
                }
                return Ok(());
            }
            DebugEventResponse::Exit => {
                return Ok(());
            }
        }
    }
}

fn wait_for_debug_event(timeout_ms: Option<u32>) -> windows::Result<DebugEvent> {
    unsafe {
        let mut event = std::mem::zeroed();

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
