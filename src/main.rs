#[macro_use]
extern crate static_assertions;

use std::{collections::HashSet, error::Error};

use debugger::{run_debug_loop, DebugEvent, DebugEventHandler, DebugEventInfo, DebugEventResponse};
use process::ProcessCreator;
use wmi::{Wmi, WmiConnector};

mod debugger;
mod process;
mod util;
mod wmi;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();

    windows::initialize_mta()?;

    let mut logger = CommandLineLogger::new()?;

    ProcessCreator::new_with_arguments(&args, false)
        .debug(true)
        .create()?;

    run_debug_loop(&mut logger, None)?;

    dbg!(logger.command_lines());

    Ok(())
}

struct CommandLineLogger {
    wmi: Wmi,
    extant_processes: HashSet<u32>,
    command_lines: Vec<String>,
}

impl CommandLineLogger {
    pub fn new() -> windows::Result<Self> {
        Ok(Self {
            wmi: WmiConnector::new("root\\cimv2")
                .use_max_wait(true)
                .connect()?,
            extant_processes: HashSet::new(),
            command_lines: Vec::new(),
        })
    }

    pub fn command_lines(&self) -> &Vec<String> {
        &self.command_lines
    }

    fn get_process_command_line(&self, process_id: u32) -> windows::Result<Option<String>> {
        const E_NOTFOUND: windows::HRESULT = windows::HRESULT(0x8000100D);

        Ok(self
            .wmi
            .exec_query(format!(
                "select CommandLine from Win32_Process where ProcessId = {}",
                process_id
            ))?
            .nth(0)
            .ok_or(windows::Error::from(E_NOTFOUND))??
            .get("CommandLine")?
            .get_string())
    }
}

impl DebugEventHandler for CommandLineLogger {
    fn handle_event(&mut self, event: &DebugEvent) -> DebugEventResponse {
        match event.info() {
            DebugEventInfo::CreateProcess(_) => {
                self.extant_processes.insert(event.process_id());

                if let Ok(command_line) = self.get_process_command_line(event.process_id()) {
                    if let Some(command_line) = command_line {
                        self.command_lines.push(command_line);
                    }
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
