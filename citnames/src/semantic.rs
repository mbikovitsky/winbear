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

use std::{
    error::Error,
    fmt::Debug,
    path::{Path, PathBuf},
};

use log::debug;

use crate::{
    configuration::Compilation,
    semantic::tool::{
        not_recognized, recognized_ok, recognized_with_error, tool_any::ToolAny,
        tool_clang::ToolClang, tool_cuda::ToolCuda, tool_extending_wrapper::ToolExtendingWrapper,
        tool_gcc::ToolGcc, tool_wrapper::ToolWrapper, Tool,
    },
    Entry, Run,
};

mod parsers;
mod tool;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Semantic {
    QueryCompiler,
    Preprocess,
    Compile(CompileSemantic),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileSemantic {
    pub working_dir: PathBuf,
    pub compiler: PathBuf,
    pub flags: Vec<String>,
    pub sources: Vec<PathBuf>,
    pub output: Option<PathBuf>,
}

impl Semantic {
    pub fn into_entries(&self) -> Option<Vec<Entry>> {
        if let Self::Compile(compile) = self {
            let abspath = |path: &Path| {
                if path.is_absolute() {
                    path.to_path_buf()
                } else {
                    compile.working_dir.join(path)
                }
            };

            let result = compile
                .sources
                .iter()
                .map(|source| {
                    let mut entry = Entry {
                        file: abspath(source),
                        directory: compile.working_dir.clone(),
                        output: compile.output.as_ref().map(|output| abspath(output)),
                        arguments: vec![compile.compiler.to_str().unwrap().to_string()],
                    };
                    entry.arguments.append(&mut compile.flags.clone());
                    if let Some(output) = &compile.output {
                        entry.arguments.push("-o".to_string());
                        entry.arguments.push(output.to_str().unwrap().to_string());
                    }
                    entry.arguments.push(source.to_str().unwrap().to_string());
                    entry
                })
                .collect();

            Some(result)
        } else {
            None
        }
    }
}

pub struct Build {
    tools: Box<dyn Tool>,
}

impl Build {
    pub fn new(cfg: Compilation) -> Self {
        // TODO: use `cfg.flags_to_remove`

        let mut tools: Vec<Box<dyn Tool>> = vec![
            Box::new(ToolGcc::default()),
            Box::new(ToolClang::default()),
            Box::new(ToolWrapper::default()),
            Box::new(ToolCuda::default()),
        ];

        for compiler in cfg.compilers_to_recognize {
            tools.push(Box::new(ToolExtendingWrapper::new(compiler)));
        }

        let wrapper = ToolAny::new(tools, cfg.compilers_to_exclude);

        Self {
            tools: Box::new(wrapper),
        }
    }

    pub fn recognize(&self, event: &Run) -> Result<Option<Semantic>, Box<dyn Error>> {
        let execution = &event.execution;
        let pid = event.pid;

        let result = self.tools.recognize(execution);
        if recognized_ok(&result) {
            debug!("[pid: {}] recognized.", pid);
        } else if recognized_with_error(&result) {
            debug!(
                "[pid: {}] recognition failed: {}",
                pid,
                result.as_ref().unwrap_err()
            );
        } else if not_recognized(&result) {
            debug!("[pid: {}] not recognized.", pid);
        }

        result
    }
}
