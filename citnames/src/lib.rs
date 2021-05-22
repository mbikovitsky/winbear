/*
   Copyright (C) 2012-2021 by László Nagy
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

use std::{collections::HashMap, ffi::OsString, path::PathBuf};

pub mod configuration;
pub mod output;
mod resolver;
pub mod semantic;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Run {
    pub execution: Execution,
    pub pid: u32,
    pub ppid: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Execution {
    pub executable: PathBuf,
    pub arguments: Vec<String>,
    pub working_dir: PathBuf,
    pub environment: HashMap<OsString, OsString>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub file: PathBuf,
    pub directory: PathBuf,
    pub output: Option<PathBuf>,
    pub arguments: Vec<String>,
}
