#[macro_use]
extern crate static_assertions;

#[macro_use]
extern crate json;

use std::error::Error;

use clap::{crate_authors, crate_name, crate_version, App, Arg};

use execution_logger::ExecutionLogger;
use process::ProcessCreator;

mod debugger;
mod execution_logger;
mod process;

fn main() -> Result<(), Box<dyn Error>> {
    let args = App::new(crate_name!())
        .version(crate_version!())
        .author(crate_authors!())
        .about("Creates compile_commands.json from uncooperating build systems")
        .setting(clap::AppSettings::TrailingVarArg)
        .arg(
            Arg::with_name("output")
                .short("o")
                .long("output")
                .value_name("FILENAME")
                .help("Compilation database path")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("COMMAND")
                .required(true)
                .multiple(true)
                .help("Command to run"),
        )
        .get_matches();

    let command: Vec<_> = args.values_of("COMMAND").unwrap().collect();

    let output = args.value_of("output").unwrap_or("compile_commands.json");

    let mut logger = ExecutionLogger::new();

    logger.log(&ProcessCreator::new_with_arguments(&command, false))?;

    Ok(())
}
