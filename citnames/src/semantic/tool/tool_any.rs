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

use std::{error::Error, path::PathBuf};

use crate::{
    semantic::{
        tool::{recognized_ok, recognized_with_error, Tool},
        Semantic,
    },
    Execution,
};

pub struct ToolAny {
    tools: Vec<Box<dyn Tool>>,
    to_exclude: Vec<PathBuf>,
}

impl ToolAny {
    pub fn new(tools: Vec<Box<dyn Tool>>, to_exclude: Vec<PathBuf>) -> Self {
        Self { tools, to_exclude }
    }
}

impl Tool for ToolAny {
    fn recognize(&self, execution: &Execution) -> Result<Option<Semantic>, Box<dyn Error>> {
        // do different things if the execution is matching one of the nominated compilers.
        if self.to_exclude.contains(&execution.executable) {
            return Err("The tool is on the exclude list from configuration.")?;
        }

        // check if any tool can recognize the execution.
        for tool in &self.tools {
            let result = tool.recognize(execution);
            // return if it recognized in any way.
            if recognized_ok(&result) || recognized_with_error(&result) {
                return result;
            }
        }

        return Err("No tools recognize this execution.")?;
    }
}
