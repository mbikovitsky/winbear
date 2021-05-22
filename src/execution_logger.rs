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

use std::{
    collections::{BTreeMap, HashMap},
    error::Error,
    ffi::OsString,
    path::PathBuf,
    time::SystemTime,
};

use bindings::Windows::Win32::System::Threading::{PROCESS_QUERY_INFORMATION, PROCESS_VM_READ};
use chrono::{DateTime, Utc};

use util::command_line_to_argv;

use crate::{
    debugger::{run_debug_loop, DebugEvent, DebugEventHandler, DebugEventInfo, DebugEventResponse},
    process::{EnvironmentBlock, Process, ProcessCreator},
};

pub struct ExecutionLogger {
    extant_processes: HashMap<u32, u64>,
    executions: BTreeMap<u64, Execution>,
    next_id: u64,
}

impl ExecutionLogger {
    pub fn new() -> Self {
        Self {
            extant_processes: HashMap::new(),
            executions: BTreeMap::new(),
            next_id: 0,
        }
    }

    pub fn log(&mut self, process_creator: &ProcessCreator) -> Result<(), Box<dyn Error>> {
        process_creator.clone().debug(true).create()?;

        run_debug_loop(self, None)?;

        Ok(())
    }

    pub fn executions(&self) -> Vec<&Execution> {
        self.executions.values().collect()
    }

    pub fn to_json(&self, spaces: Option<u16>) -> String {
        let executions: Vec<_> = self
            .executions
            .values()
            .map(|execution| {
                let events: Vec<_> = execution
                    .run
                    .events
                    .iter()
                    .map(|event| {
                        let at: DateTime<Utc> = event.at.into();
                        let at = at.format("%Y-%m-%dT%H:%M:%S%.3fZ");
                        let at = at.to_string();
                        match event.kind {
                            EventKind::Start => {
                                object! {
                                    "at": at,
                                    "type": "start",
                                }
                            }
                            EventKind::Stop { status } => {
                                object! {
                                    "at": at,
                                    "type": "stop",
                                    "status": status
                                }
                            }
                        }
                    })
                    .collect();

                let arguments: Vec<String> = execution
                    .command
                    .arguments
                    .iter()
                    .map(|arg| arg.to_string_lossy().to_string())
                    .collect();

                let environment: HashMap<String, String> = execution
                    .command
                    .environment
                    .iter()
                    .map(|(key, value)| {
                        (
                            key.to_string_lossy().to_string(),
                            value.to_string_lossy().to_string(),
                        )
                    })
                    .collect();

                object! {
                    "command": {
                        "arguments": arguments,
                        "environment": environment,
                        "program": execution.command.program.to_string_lossy().as_ref(),
                        "working_dir": execution.command.working_dir.to_string_lossy().as_ref(),
                    },
                    "run": {
                        "events": events,
                        "pid": execution.run.pid,
                        "ppid": execution.run.ppid,
                    }
                }
            })
            .collect();

        let result_object = object! {
            "executions": executions.as_slice()
        };

        if let Some(spaces) = spaces {
            result_object.pretty(spaces)
        } else {
            result_object.dump()
        }
    }

    fn add_execution(&mut self, process_id: u32) -> Result<(), Box<dyn Error>> {
        let process = Process::open(process_id, PROCESS_VM_READ | PROCESS_QUERY_INFORMATION)?;

        let execution = Execution {
            command: Command {
                program: process.image_name()?,
                arguments: command_line_to_argv(&process.command_line()?)?,
                environment: process.environment()?,
                working_dir: process.current_directory()?,
            },
            run: Run {
                events: vec![Event {
                    at: SystemTime::now(),
                    kind: EventKind::Start,
                }],
                pid: process_id,
                ppid: process.parent_process_id()?,
            },
        };

        let inserted_execution = self.executions.insert(self.next_id, execution).is_none();
        assert!(inserted_execution);

        let inserted_pid_map = self
            .extant_processes
            .insert(process_id, self.next_id)
            .is_none();
        assert!(inserted_pid_map);

        self.next_id += 1;

        Ok(())
    }

    fn finish_execution(&mut self, process_id: u32) {
        let process = Process::open(process_id, PROCESS_QUERY_INFORMATION).unwrap();

        let execution_id = *self.extant_processes.get(&process_id).unwrap();

        let execution = self.executions.get_mut(&execution_id).unwrap();

        execution.run.events.push(Event {
            at: SystemTime::now(),
            kind: EventKind::Stop {
                status: process.exit_code().unwrap(),
            },
        });

        self.extant_processes.remove(&process_id).unwrap();
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

                return DebugEventResponse::ExceptionNotHandled;
            }
            DebugEventInfo::ExitProcess(_) => {
                self.finish_execution(event.process_id());

                if self.is_done() {
                    return DebugEventResponse::ExitExceptionNotHandled;
                }

                return DebugEventResponse::ExceptionNotHandled;
            }
            _ => {
                return DebugEventResponse::ExceptionNotHandled;
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct Execution {
    pub command: Command,
    pub run: Run,
}

#[derive(Debug, Clone)]
pub struct Command {
    pub program: PathBuf,
    pub arguments: Vec<OsString>,
    pub environment: EnvironmentBlock,
    pub working_dir: PathBuf,
}

#[derive(Debug, Clone)]
pub struct Run {
    pub events: Vec<Event>,
    pub pid: u32,
    pub ppid: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct Event {
    pub at: SystemTime,
    pub kind: EventKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    Start,
    Stop { status: u32 },
}
