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

use std::path::PathBuf;

#[derive(Debug, Clone, Default)]
pub struct Configuration {
    pub output: Output,
    pub compilation: Compilation,
}

#[derive(Debug, Clone, Default)]
pub struct Output {
    pub format: Format,
    pub content: Content,
}

#[derive(Debug, Clone, Copy)]
pub struct Format {
    pub command_as_array: bool,
    pub drop_output_field: bool,
}

impl Default for Format {
    fn default() -> Self {
        Self {
            command_as_array: true,
            drop_output_field: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Content {
    pub include_only_existing_source: bool,
    pub paths_to_include: Vec<PathBuf>,
    pub paths_to_exclude: Vec<PathBuf>,
}

impl Default for Content {
    fn default() -> Self {
        Self {
            include_only_existing_source: false,
            paths_to_include: vec![],
            paths_to_exclude: vec![],
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct Compilation {
    pub compilers_to_recognize: Vec<CompilerWrapper>,
    pub compilers_to_exclude: Vec<PathBuf>,
    pub flags_to_remove: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct CompilerWrapper {
    pub executable: PathBuf,
    pub additional_flags: Vec<String>,
}
