#[macro_use]
extern crate static_assertions;

#[macro_use]
extern crate lazy_static;

use std::error::Error;

use debugger::wait_for_debug_event;
use debugger::DebugEventInfo;
use process::ProcessCreator;
use wmi::Wmi;

mod debugger;
mod process;
mod util;
mod wmi;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();

    let main_process_id = ProcessCreator::new_with_arguments(&args, false)
        .debug(true)
        .create()?
        .process_id();

    loop {
        let debug_event = wait_for_debug_event(None)?;

        if let DebugEventInfo::ExitProcess(_) = debug_event.info() {
            if debug_event.process_id() == main_process_id {
                debug_event.continue_unhandled()?;
                break;
            }
        }

        debug_event.continue_unhandled()?;
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
