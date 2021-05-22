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
