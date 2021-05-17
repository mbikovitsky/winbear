#[macro_use]
extern crate static_assertions;

#[macro_use]
extern crate json;

use std::error::Error;

use execution_logger::ExecutionLogger;
use process::ProcessCreator;

mod debugger;
mod execution_logger;
mod process;
mod util;

fn main() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();

    let mut logger = ExecutionLogger::new();

    logger.log(&ProcessCreator::new_with_arguments(&args, false))?;

    Ok(())
}
