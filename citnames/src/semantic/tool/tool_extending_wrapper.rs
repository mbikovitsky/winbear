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

use std::path::Path;

use crate::{configuration::CompilerWrapper, semantic::Semantic, Execution};

use super::{tool_gcc::GccLike, Tool};

#[derive(Debug, Clone)]
pub struct ToolExtendingWrapper {
    compiler_to_recognize: CompilerWrapper,
}

impl ToolExtendingWrapper {
    pub fn new(compiler_to_recognize: CompilerWrapper) -> Self {
        Self {
            compiler_to_recognize
        }
    }
}

impl GccLike for ToolExtendingWrapper {
    fn recognize_program(&self, program: impl AsRef<Path>) -> bool {
        self.compiler_to_recognize.executable == program.as_ref()
    }
}

impl Tool for ToolExtendingWrapper {
    fn recognize(
        &self,
        execution: &Execution,
    ) -> Result<Option<Semantic>, Box<dyn std::error::Error>> {
        self.recognize_execution(execution).map(|semantic| {
            if let Some(semantic) = semantic {
                match semantic {
                    Semantic::Compile(mut compile) => {
                        compile
                            .flags
                            .append(&mut self.compiler_to_recognize.additional_flags.clone());
                        Some(Semantic::Compile(compile))
                    }
                    _ => Some(semantic),
                }
            } else {
                None
            }
        })
    }
}
