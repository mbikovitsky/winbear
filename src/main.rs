#[macro_use]
extern crate static_assertions;

use std::error::Error;

use debugger::{wait_for_debug_event, DebugEventInfo};
use process::ProcessCreator;
use wmi::{Wmi, WmiConnector};

mod debugger;
mod process;
mod wmi;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();

    windows::initialize_mta()?;
    let wmi = WmiConnector::new("root\\cimv2").connect()?;

    let main_process_id = ProcessCreator::new_with_arguments(&args, false)
        .debug(true)
        .create()?
        .process_id();

    loop {
        let debug_event = wait_for_debug_event(None)?;

        match debug_event.info() {
            DebugEventInfo::ExitProcess(_) => {
                if debug_event.process_id() == main_process_id {
                    debug_event.continue_event(false)?;
                    break;
                }
            }
            DebugEventInfo::CreateProcess(_) => {
                let command_line = get_process_command_line(&wmi, debug_event.process_id())?;
                if let Some(command_line) = command_line {
                    dbg!(command_line);
                }
            }
            _ => {}
        }

        debug_event.continue_event(false)?;
    }

    Ok(())
}

fn get_process_command_line(wmi: &Wmi, process_id: u32) -> windows::Result<Option<String>> {
    const E_NOTFOUND: windows::HRESULT = windows::HRESULT(0x8000100D);

    Ok(wmi
        .exec_query(format!(
            "select CommandLine from Win32_Process where ProcessId = {}",
            process_id
        ))?
        .nth(0)
        .ok_or(windows::Error::from(E_NOTFOUND))??
        .get("CommandLine")?
        .get_string())
}
