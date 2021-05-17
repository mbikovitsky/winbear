#[macro_use]
extern crate static_assertions;

use std::error::Error;

use debugger::run_debug_loop;
use execution_logger::ExecutionLogger;
use process::ProcessCreator;

mod debugger;
mod execution_logger;
mod process;
mod util;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();

    let mut logger = ExecutionLogger::new()?;

    ProcessCreator::new_with_arguments(&args, false)
        .debug(true)
        .create()?;

    run_debug_loop(&mut logger, None)?;

    Ok(())
}
