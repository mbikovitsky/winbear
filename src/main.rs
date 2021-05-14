#[macro_use]
extern crate static_assertions;

use std::error::Error;

use debugger::wait_for_debug_event;
use debugger::DebugEventInfo;
use process::ProcessCreator;

mod debugger;
mod process;

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
