use std::{
    collections::{BTreeMap, HashMap},
    error::Error,
    time::SystemTime,
};

use chrono::{DateTime, Utc};

use bindings::Windows::Win32::System::Threading::{PROCESS_QUERY_INFORMATION, PROCESS_VM_READ};

use crate::{
    debugger::{run_debug_loop, DebugEvent, DebugEventHandler, DebugEventInfo, DebugEventResponse},
    process::{Process, ProcessCreator},
    util::command_line_to_argv,
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

    pub fn log(&mut self, process_creator: &ProcessCreator) -> windows::Result<()> {
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

                object! {
                    "command": {
                        "arguments": execution.command.arguments.as_slice(),
                        "environment": execution.command.environment.clone(),
                        "program": execution.command.program.as_str(),
                        "working_dir": execution.command.working_dir.as_str(),
                    },
                    "run": {
                        "events": events.as_slice(),
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
                arguments: command_line_to_argv(process.command_line()?)?,
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
    pub program: String,
    pub arguments: Vec<String>,
    pub environment: HashMap<String, String>,
    pub working_dir: String,
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
