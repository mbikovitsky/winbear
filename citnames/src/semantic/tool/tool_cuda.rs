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

use std::{ffi::OsStr, path::Path};

use lazy_static::lazy_static;
use regex::Regex;

use super::{tool_gcc::GccLike, Tool};

#[derive(Debug, Clone, Copy, Default)]
pub struct ToolCuda;

impl GccLike for ToolCuda {
    fn recognize_program(&self, program: impl AsRef<Path>) -> bool {
        lazy_static! {
            static ref RE: Regex = Regex::new(r#"^(nvcc)$"#).unwrap();
        }

        RE.is_match(
            &program
                .as_ref()
                .file_stem()
                .unwrap_or(OsStr::new(""))
                .to_string_lossy(),
        )
    }
}

impl Tool for ToolCuda {
    fn recognize(
        &self,
        execution: &crate::Execution,
    ) -> Result<Option<super::Semantic>, Box<dyn std::error::Error>> {
        self.recognize_execution(execution)
    }
}
