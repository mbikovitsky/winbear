#[macro_use]
extern crate static_assertions;

use std::{collections::HashSet, error::Error};

use bindings::Windows::Win32::System::Threading::{PROCESS_QUERY_INFORMATION, PROCESS_VM_READ};

use debugger::{run_debug_loop, DebugEvent, DebugEventHandler, DebugEventInfo, DebugEventResponse};
use process::{Process, ProcessCreator};

mod debugger;
mod process;
mod util;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();

    let mut logger = CommandLineLogger::new()?;

    ProcessCreator::new_with_arguments(&args, false)
        .debug(true)
        .create()?;

    run_debug_loop(&mut logger, None)?;

    dbg!(logger.command_lines());

    Ok(())
}

struct CommandLineLogger {
    extant_processes: HashSet<u32>,
    command_lines: Vec<String>,
}

impl CommandLineLogger {
    pub fn new() -> windows::Result<Self> {
        Ok(Self {
            extant_processes: HashSet::new(),
            command_lines: Vec::new(),
        })
    }

    pub fn command_lines(&self) -> &Vec<String> {
        &self.command_lines
    }

    fn get_process_command_line(&self, process_id: u32) -> Result<String, Box<dyn Error>> {
        let process = Process::open(process_id, PROCESS_VM_READ | PROCESS_QUERY_INFORMATION)?;

        Ok(process.command_line()?)
    }
}

impl DebugEventHandler for CommandLineLogger {
    fn handle_event(&mut self, event: &DebugEvent) -> DebugEventResponse {
        match event.info() {
            DebugEventInfo::CreateProcess(_) => {
                self.extant_processes.insert(event.process_id());

                if let Ok(command_line) = self.get_process_command_line(event.process_id()) {
                    self.command_lines.push(command_line)
                }

                // TODO: log command-line acquisition failure

                return DebugEventResponse::ExceptionNotHandled;
            }
            DebugEventInfo::ExitProcess(_) => {
                self.extant_processes.remove(&event.process_id());

                if self.extant_processes.is_empty() {
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
