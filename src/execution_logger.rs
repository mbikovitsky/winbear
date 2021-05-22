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

use std::{collections::HashSet, error::Error, ffi::OsString, path::PathBuf};

use bindings::Windows::Win32::System::Threading::{PROCESS_QUERY_INFORMATION, PROCESS_VM_READ};

use crate::{
    debugger::{
        run_debug_loop, DebugEvent, DebugEventHandler, DebugEventInfo, DebugEventResponse,
        ExceptionContinuation,
    },
    process::{EnvironmentBlock, Process, ProcessCreator},
};

pub struct ExecutionLogger {
    extant_processes: HashSet<u32>,
    executions: Vec<ExecutionInfo>,
}

impl ExecutionLogger {
    pub fn new() -> Self {
        Self {
            extant_processes: HashSet::new(),
            executions: Vec::new(),
        }
    }

    pub fn log(&mut self, process_creator: &ProcessCreator) -> Result<(), Box<dyn Error>> {
        process_creator.clone().debug(true).create()?;

        run_debug_loop(self, None)?;

        Ok(())
    }

    pub fn executions(&self) -> &[ExecutionInfo] {
        &self.executions
    }

    fn add_execution(&mut self, process_id: u32) -> Result<(), Box<dyn Error>> {
        let process = Process::open(process_id, PROCESS_VM_READ | PROCESS_QUERY_INFORMATION)?;

        let execution = ExecutionInfo {
            executable: process.image_name()?,
            command_line: process.command_line()?,
            working_dir: process.current_directory()?,
            environment: process.environment()?,
            pid: process_id,
            ppid: process.parent_process_id()?,
        };

        self.executions.push(execution);
        let inserted = self.extant_processes.insert(process_id);
        assert!(inserted);

        Ok(())
    }

    fn finish_execution(&mut self, process_id: u32) {
        let removed = self.extant_processes.remove(&process_id);
        assert!(removed);
    }

    fn is_done(&self) -> bool {
        self.extant_processes.is_empty()
    }
}

impl DebugEventHandler for ExecutionLogger {
    fn handle_event(&mut self, event: &DebugEvent) -> DebugEventResponse {
        match event.info() {
            DebugEventInfo::CreateProcess(_) => {
                if let Err(error) = self.add_execution(event.process_id()) {
                    // TODO: better logging
                    eprintln!("{}", error);
                }

                return DebugEventResponse::Continue(ExceptionContinuation::NotHandled);
            }
            DebugEventInfo::ExitProcess(_) => {
                self.finish_execution(event.process_id());

                if self.is_done() {
                    return DebugEventResponse::ExitDetach(ExceptionContinuation::NotHandled);
                }

                return DebugEventResponse::Continue(ExceptionContinuation::NotHandled);
            }
            _ => {
                return DebugEventResponse::Continue(ExceptionContinuation::NotHandled);
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExecutionInfo {
    pub executable: PathBuf,
    pub command_line: OsString,
    pub working_dir: PathBuf,
    pub environment: EnvironmentBlock,
    pub pid: u32,
    pub ppid: u32,
}
