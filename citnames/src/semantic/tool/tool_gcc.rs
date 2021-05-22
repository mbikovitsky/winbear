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
    collections::{BTreeMap, HashMap},
    error::Error,
    ffi::{OsStr, OsString},
    path::{Path, PathBuf},
};

use lazy_static::lazy_static;
use regex::Regex;

use crate::{
    semantic::{
        parsers::{
            parse, CompilerFlag, CompilerFlagType, EverythingElseFlagMatcher, FlagDefinition,
            FlagParser, Instruction, Match, OneOf, Repeat, SourceMatcher,
        },
        tool::Tool,
        CompileSemantic, Semantic,
    },
    Execution,
};

pub trait GccLike: Tool {
    fn recognize_program(&self, program: impl AsRef<Path>) -> bool;

    fn recognize_execution(
        &self,
        execution: &Execution,
    ) -> Result<Option<Semantic>, Box<dyn Error>> {
        if self.recognize_program(&execution.executable) {
            compilation(execution)
        } else {
            Ok(None)
        }
    }
}

pub fn compilation(execution: &Execution) -> Result<Option<Semantic>, Box<dyn Error>> {
    match parse_flags(execution) {
        Ok(flags) => {
            if is_compiler_query(&flags) {
                return Ok(Some(Semantic::QueryCompiler));
            }

            if is_preprocessor(&flags) {
                return Ok(Some(Semantic::Preprocess));
            }

            let (mut arguments, sources, output) = split(&flags);

            // Validate: must have source files.
            if sources.is_empty() {
                return Err("Source files not found for compilation.")?;
            }

            // TODO: introduce semantic type for linking
            if linking(&flags) {
                arguments.insert(0, "-c".to_string());
            }

            // Create compiler flags from environment variables if present.
            let mut extra = flags_from_environment(&execution.environment);
            arguments.append(&mut extra);

            Ok(Some(Semantic::Compile(CompileSemantic {
                working_dir: execution.working_dir.clone(),
                compiler: execution.executable.clone(),
                flags: arguments,
                sources,
                output,
            })))
        }
        Err(error) => Err(error),
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ToolGcc;

impl GccLike for ToolGcc {
    fn recognize_program(&self, program: impl AsRef<Path>) -> bool {
        lazy_static! {
            static ref RE: Regex = Regex::new(
                r#"^(cc|c\+\+|cxx|CC|(([^-]*-)*([mg](cc|\+\+)|[g]?fortran)(-?\d+(\.\d+){0,2})?))$"#
            )
            .unwrap();
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

impl Tool for ToolGcc {
    fn recognize(&self, execution: &Execution) -> Result<Option<Semantic>, Box<dyn Error>> {
        self.recognize_execution(execution)
    }
}

fn parse_flags(execution: &Execution) -> Result<Vec<CompilerFlag>, Box<dyn Error>> {
    lazy_static! {
        // TODO: Can we avoid the map? Seems like a waste of memory.
        static ref FLAG_DEFINITION_MAP: BTreeMap<&'static str, FlagDefinition> =
            FLAG_DEFINITION.iter().copied().collect();
        static ref PARSER: Repeat<OneOf<3>> = Repeat::new(OneOf::new([
            Box::new(FlagParser::new(&FLAG_DEFINITION_MAP)),
            Box::new(SourceMatcher::default()),
            Box::new(EverythingElseFlagMatcher::default()),
        ]));
    }

    parse(&*PARSER, execution)
}

fn flags_from_environment(environment: &HashMap<OsString, OsString>) -> Vec<String> {
    let mut flags = vec![];

    let mut inserter = |value: &OsStr, flag: &str| {
        for path in std::env::split_paths(value) {
            let path = path.to_string_lossy();
            let directory = if path.is_empty() { "." } else { &path };
            flags.push(flag.to_string());
            flags.push(directory.to_string());
        }
    };

    for &env in &["CPATH", "C_INCLUDE_PATH", "CPLUS_INCLUDE_PATH"] {
        if let Some(value) = environment.get(OsStr::new(env)) {
            inserter(value, "-I");
        }
    }

    if let Some(value) = environment.get(OsStr::new("OBJC_INCLUDE_PATH")) {
        inserter(value, "-isystem");
    }

    flags
}

fn is_compiler_query(flags: &[CompilerFlag]) -> bool {
    // no flag is a no compilation
    if flags.is_empty() {
        return true;
    }

    // otherwise check if this was a version query of a help
    flags
        .iter()
        .any(|flag| flag.type_ == CompilerFlagType::KindOfOutputInfo)
}

fn is_preprocessor(flags: &[CompilerFlag]) -> bool {
    // one of those make dependency generation also not count as compilation.
    // (will cause duplicate element, which is hard to detect.)
    static NO_COMPILATION_FLAG: [&'static str; 2] = ["-M", "-MM"];

    flags.iter().any(|flag| {
        let candidate = &flag.arguments[0];
        (flag.type_ == CompilerFlagType::KindOfOutputNoLinking && candidate == "-E")
            || (flag.type_ == CompilerFlagType::PreprocessorMake
                && NO_COMPILATION_FLAG.contains(&candidate.as_str()))
    })
}

fn linking(flags: &[CompilerFlag]) -> bool {
    !flags
        .iter()
        .any(|flag| flag.type_ == CompilerFlagType::KindOfOutputNoLinking)
}

fn split(flags: &[CompilerFlag]) -> (Vec<String>, Vec<PathBuf>, Option<PathBuf>) {
    let mut arguments = vec![];
    let mut sources = vec![];
    let mut output = None;

    for flag in flags {
        match flag.type_ {
            CompilerFlagType::Source => {
                let candidate = flag.arguments.first().unwrap().into();
                sources.push(candidate);
            }

            CompilerFlagType::KindOfOutputOutput => {
                let candidate = flag.arguments.last().unwrap().into();
                output = Some(candidate);
            }

            CompilerFlagType::Linker
            | CompilerFlagType::PreprocessorMake
            | CompilerFlagType::DirectorySearchLinker => {}

            _ => {
                arguments.append(&mut flag.arguments.clone());
            }
        }
    }

    (arguments, sources, output)
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, path::PathBuf};

    use crate::{
        semantic::{
            tool::{
                not_recognized, recognized_ok,
                tool_gcc::{GccLike, ToolGcc},
                Tool,
            },
            CompileSemantic, Semantic,
        },
        Execution,
    };

    #[test]
    fn recognize() {
        let sut = ToolGcc::default();

        assert!(sut.recognize_program("cc"));
        assert!(sut.recognize_program("/usr/bin/cc"));
        assert!(sut.recognize_program("gcc"));
        assert!(sut.recognize_program("/usr/bin/gcc"));
        assert!(sut.recognize_program("c++"));
        assert!(sut.recognize_program("/usr/bin/c++"));
        assert!(sut.recognize_program("g++"));
        assert!(sut.recognize_program("/usr/bin/g++"));
        assert!(sut.recognize_program("arm-none-eabi-g++"));
        assert!(sut.recognize_program("/usr/bin/arm-none-eabi-g++"));
        assert!(sut.recognize_program("gcc-6"));
        assert!(sut.recognize_program("/usr/bin/gcc-6"));
        assert!(sut.recognize_program("gfortran"));
        assert!(sut.recognize_program("fortran"));
    }

    #[test]
    fn fails_on_empty() {
        let input = Default::default();

        let sut = ToolGcc::default();

        assert!(not_recognized(&sut.recognize(&input)));
    }

    #[test]
    fn simple() {
        let input = Execution {
            executable: PathBuf::from("/usr/bin/cc"),
            arguments: ["cc", "-c", "-o", "source.o", "source.c"]
                .iter()
                .copied()
                .map(String::from)
                .collect(),
            working_dir: PathBuf::from("/home/user/project"),
            environment: HashMap::new(),
        };

        let expected = Semantic::Compile(CompileSemantic {
            working_dir: input.working_dir.clone(),
            compiler: input.executable.clone(),
            flags: vec!["-c".to_string()],
            sources: vec![PathBuf::from("source.c")],
            output: Some(PathBuf::from("source.o")),
        });

        let sut = ToolGcc::default();

        let result = sut.recognize(&input);

        assert!(recognized_ok(&result));

        assert_eq!(expected, result.unwrap().unwrap());
    }

    #[test]
    fn linker_flag_filtered() {
        let input = Execution {
            executable: PathBuf::from("/usr/bin/cc"),
            arguments: ["cc", "-L.", "-lthing", "-o", "exe", "source.c"]
                .iter()
                .copied()
                .map(String::from)
                .collect(),
            working_dir: PathBuf::from("/home/user/project"),
            environment: HashMap::new(),
        };

        let expected = Semantic::Compile(CompileSemantic {
            working_dir: input.working_dir.clone(),
            compiler: input.executable.clone(),
            flags: vec!["-c".to_string()],
            sources: vec![PathBuf::from("source.c")],
            output: Some(PathBuf::from("exe")),
        });

        let sut = ToolGcc::default();

        let result = sut.recognize(&input);

        assert!(recognized_ok(&result));

        assert_eq!(expected, result.unwrap().unwrap());
    }

    #[test]
    fn pass_on_help() {
        let input = Execution {
            executable: PathBuf::from("/usr/bin/gcc"),
            arguments: ["gcc", "--version"]
                .iter()
                .copied()
                .map(String::from)
                .collect(),
            working_dir: PathBuf::from("/home/user/project"),
            environment: HashMap::new(),
        };

        let expected = Semantic::QueryCompiler;

        let sut = ToolGcc::default();

        let result = sut.recognize(&input);

        assert!(recognized_ok(&result));

        assert_eq!(expected, result.unwrap().unwrap());
    }

    #[test]
    fn simple_with_c_path() {
        let mut environment = HashMap::new();
        environment.insert(
            "CPATH".into(),
            "/usr/include/path1;/usr/include/path2".into(),
        );
        environment.insert("C_INCLUDE_PATH".into(), ";/usr/include/path3".into());

        let input = Execution {
            executable: PathBuf::from("/usr/bin/cc"),
            arguments: ["cc", "-c", "source.c"]
                .iter()
                .copied()
                .map(String::from)
                .collect(),
            working_dir: PathBuf::from("/home/user/project"),
            environment: environment,
        };

        let expected = Semantic::Compile(CompileSemantic {
            working_dir: input.working_dir.clone(),
            compiler: input.executable.clone(),
            flags: [
                "-c",
                "-I",
                "/usr/include/path1",
                "-I",
                "/usr/include/path2",
                "-I",
                ".",
                "-I",
                "/usr/include/path3",
            ]
            .iter()
            .copied()
            .map(String::from)
            .collect(),
            sources: vec![PathBuf::from("source.c")],
            output: None,
        });

        let sut = ToolGcc::default();

        let result = sut.recognize(&input);

        assert!(recognized_ok(&result));

        assert_eq!(expected, result.unwrap().unwrap());
    }
}

const FLAG_DEFINITION: &[(&'static str, FlagDefinition)] = &[
    (
        "-x",
        FlagDefinition {
            consumption: Instruction::new(1, Match::Exact, false),
            type_: CompilerFlagType::KindOfOutput,
        },
    ),
    (
        "-c",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Exact, false),
            type_: CompilerFlagType::KindOfOutputNoLinking,
        },
    ),
    (
        "-S",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Exact, false),
            type_: CompilerFlagType::KindOfOutputNoLinking,
        },
    ),
    (
        "-E",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Exact, false),
            type_: CompilerFlagType::KindOfOutputNoLinking,
        },
    ),
    (
        "-o",
        FlagDefinition {
            consumption: Instruction::new(1, Match::Exact, false),
            type_: CompilerFlagType::KindOfOutputOutput,
        },
    ),
    (
        "-dumpbase",
        FlagDefinition {
            consumption: Instruction::new(1, Match::Exact, false),
            type_: CompilerFlagType::KindOfOutput,
        },
    ),
    (
        "-dumpbase-ext",
        FlagDefinition {
            consumption: Instruction::new(1, Match::Exact, false),
            type_: CompilerFlagType::KindOfOutput,
        },
    ),
    (
        "-dumpdir",
        FlagDefinition {
            consumption: Instruction::new(1, Match::Exact, false),
            type_: CompilerFlagType::KindOfOutput,
        },
    ),
    (
        "-v",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Exact, false),
            type_: CompilerFlagType::KindOfOutput,
        },
    ),
    (
        "-###",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Exact, false),
            type_: CompilerFlagType::KindOfOutput,
        },
    ),
    (
        "--help",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Both, true),
            type_: CompilerFlagType::KindOfOutputInfo,
        },
    ),
    (
        "--target-help",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Exact, false),
            type_: CompilerFlagType::KindOfOutputInfo,
        },
    ),
    (
        "--version",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Exact, false),
            type_: CompilerFlagType::KindOfOutputInfo,
        },
    ),
    (
        "-pass-exit-codes",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Exact, false),
            type_: CompilerFlagType::KindOfOutput,
        },
    ),
    (
        "-pipe",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Exact, false),
            type_: CompilerFlagType::KindOfOutput,
        },
    ),
    (
        "-specs",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Partial, true),
            type_: CompilerFlagType::KindOfOutput,
        },
    ),
    (
        "-wrapper",
        FlagDefinition {
            consumption: Instruction::new(1, Match::Exact, false),
            type_: CompilerFlagType::KindOfOutput,
        },
    ),
    (
        "-ffile-prefix-map",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Partial, true),
            type_: CompilerFlagType::KindOfOutput,
        },
    ),
    (
        "-fplugin",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Partial, true),
            type_: CompilerFlagType::KindOfOutput,
        },
    ),
    (
        "@",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Partial, false),
            type_: CompilerFlagType::KindOfOutput,
        },
    ),
    (
        "-A",
        FlagDefinition {
            consumption: Instruction::new(1, Match::Both, false),
            type_: CompilerFlagType::Preprocessor,
        },
    ),
    (
        "-D",
        FlagDefinition {
            consumption: Instruction::new(1, Match::Both, false),
            type_: CompilerFlagType::Preprocessor,
        },
    ),
    (
        "-U",
        FlagDefinition {
            consumption: Instruction::new(1, Match::Both, false),
            type_: CompilerFlagType::Preprocessor,
        },
    ),
    (
        "-include",
        FlagDefinition {
            consumption: Instruction::new(1, Match::Exact, false),
            type_: CompilerFlagType::Preprocessor,
        },
    ),
    (
        "-imacros",
        FlagDefinition {
            consumption: Instruction::new(1, Match::Exact, false),
            type_: CompilerFlagType::Preprocessor,
        },
    ),
    (
        "-undef",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Exact, false),
            type_: CompilerFlagType::Preprocessor,
        },
    ),
    (
        "-pthread",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Exact, false),
            type_: CompilerFlagType::Preprocessor,
        },
    ),
    (
        "-M",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Exact, false),
            type_: CompilerFlagType::PreprocessorMake,
        },
    ),
    (
        "-MM",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Exact, false),
            type_: CompilerFlagType::PreprocessorMake,
        },
    ),
    (
        "-MG",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Exact, false),
            type_: CompilerFlagType::PreprocessorMake,
        },
    ),
    (
        "-MP",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Exact, false),
            type_: CompilerFlagType::PreprocessorMake,
        },
    ),
    (
        "-MD",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Exact, false),
            type_: CompilerFlagType::PreprocessorMake,
        },
    ),
    (
        "-MMD",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Exact, false),
            type_: CompilerFlagType::PreprocessorMake,
        },
    ),
    (
        "-MF",
        FlagDefinition {
            consumption: Instruction::new(1, Match::Exact, false),
            type_: CompilerFlagType::PreprocessorMake,
        },
    ),
    (
        "-MT",
        FlagDefinition {
            consumption: Instruction::new(1, Match::Exact, false),
            type_: CompilerFlagType::PreprocessorMake,
        },
    ),
    (
        "-MQ",
        FlagDefinition {
            consumption: Instruction::new(1, Match::Exact, false),
            type_: CompilerFlagType::PreprocessorMake,
        },
    ),
    (
        "-C",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Exact, false),
            type_: CompilerFlagType::Preprocessor,
        },
    ),
    (
        "-CC",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Exact, false),
            type_: CompilerFlagType::Preprocessor,
        },
    ),
    (
        "-P",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Exact, false),
            type_: CompilerFlagType::Preprocessor,
        },
    ),
    (
        "-traditional",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Both, false),
            type_: CompilerFlagType::Preprocessor,
        },
    ),
    (
        "-trigraphs",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Exact, false),
            type_: CompilerFlagType::Preprocessor,
        },
    ),
    (
        "-remap",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Exact, false),
            type_: CompilerFlagType::Preprocessor,
        },
    ),
    (
        "-H",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Exact, false),
            type_: CompilerFlagType::Preprocessor,
        },
    ),
    (
        "-Xpreprocessor",
        FlagDefinition {
            consumption: Instruction::new(1, Match::Exact, false),
            type_: CompilerFlagType::Preprocessor,
        },
    ),
    (
        "-Wp,",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Partial, false),
            type_: CompilerFlagType::Preprocessor,
        },
    ),
    (
        "-I",
        FlagDefinition {
            consumption: Instruction::new(1, Match::Both, false),
            type_: CompilerFlagType::DirectorySearch,
        },
    ),
    (
        "-iplugindir",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Partial, true),
            type_: CompilerFlagType::DirectorySearch,
        },
    ),
    (
        "-iquote",
        FlagDefinition {
            consumption: Instruction::new(1, Match::Exact, false),
            type_: CompilerFlagType::DirectorySearch,
        },
    ),
    (
        "-isystem",
        FlagDefinition {
            consumption: Instruction::new(1, Match::Exact, false),
            type_: CompilerFlagType::DirectorySearch,
        },
    ),
    (
        "-idirafter",
        FlagDefinition {
            consumption: Instruction::new(1, Match::Exact, false),
            type_: CompilerFlagType::DirectorySearch,
        },
    ),
    (
        "-iprefix",
        FlagDefinition {
            consumption: Instruction::new(1, Match::Exact, false),
            type_: CompilerFlagType::DirectorySearch,
        },
    ),
    (
        "-iwithprefix",
        FlagDefinition {
            consumption: Instruction::new(1, Match::Exact, false),
            type_: CompilerFlagType::DirectorySearch,
        },
    ),
    (
        "-iwithprefixbefore",
        FlagDefinition {
            consumption: Instruction::new(1, Match::Exact, false),
            type_: CompilerFlagType::DirectorySearch,
        },
    ),
    (
        "-isysroot",
        FlagDefinition {
            consumption: Instruction::new(1, Match::Exact, false),
            type_: CompilerFlagType::DirectorySearch,
        },
    ),
    (
        "-imultilib",
        FlagDefinition {
            consumption: Instruction::new(1, Match::Exact, false),
            type_: CompilerFlagType::DirectorySearch,
        },
    ),
    (
        "-L",
        FlagDefinition {
            consumption: Instruction::new(1, Match::Both, false),
            type_: CompilerFlagType::DirectorySearchLinker,
        },
    ),
    (
        "-B",
        FlagDefinition {
            consumption: Instruction::new(1, Match::Both, false),
            type_: CompilerFlagType::DirectorySearch,
        },
    ),
    (
        "--sysroot",
        FlagDefinition {
            consumption: Instruction::new(1, Match::Both, true),
            type_: CompilerFlagType::DirectorySearch,
        },
    ),
    (
        "-flinker-output",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Partial, true),
            type_: CompilerFlagType::Linker,
        },
    ),
    (
        "-fuse-ld",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Partial, true),
            type_: CompilerFlagType::Linker,
        },
    ),
    (
        "-l",
        FlagDefinition {
            consumption: Instruction::new(1, Match::Both, false),
            type_: CompilerFlagType::Linker,
        },
    ),
    (
        "-nostartfiles",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Exact, false),
            type_: CompilerFlagType::Linker,
        },
    ),
    (
        "-nodefaultlibs",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Exact, false),
            type_: CompilerFlagType::Linker,
        },
    ),
    (
        "-nolibc",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Exact, false),
            type_: CompilerFlagType::Linker,
        },
    ),
    (
        "-nostdlib",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Exact, false),
            type_: CompilerFlagType::Linker,
        },
    ),
    (
        "-e",
        FlagDefinition {
            consumption: Instruction::new(1, Match::Exact, false),
            type_: CompilerFlagType::Linker,
        },
    ),
    (
        "-entry",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Partial, true),
            type_: CompilerFlagType::Linker,
        },
    ),
    (
        "-pie",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Exact, false),
            type_: CompilerFlagType::Linker,
        },
    ),
    (
        "-no-pie",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Exact, false),
            type_: CompilerFlagType::Linker,
        },
    ),
    (
        "-static-pie",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Exact, false),
            type_: CompilerFlagType::Linker,
        },
    ),
    (
        "-r",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Exact, false),
            type_: CompilerFlagType::Linker,
        },
    ),
    (
        "-rdynamic",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Exact, false),
            type_: CompilerFlagType::Linker,
        },
    ),
    (
        "-s",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Exact, false),
            type_: CompilerFlagType::Linker,
        },
    ),
    (
        "-symbolic",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Exact, false),
            type_: CompilerFlagType::Linker,
        },
    ),
    (
        "-static",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Both, false),
            type_: CompilerFlagType::Linker,
        },
    ),
    (
        "-shared",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Both, false),
            type_: CompilerFlagType::Linker,
        },
    ),
    (
        "-T",
        FlagDefinition {
            consumption: Instruction::new(1, Match::Exact, false),
            type_: CompilerFlagType::Linker,
        },
    ),
    (
        "-Xlinker",
        FlagDefinition {
            consumption: Instruction::new(1, Match::Exact, false),
            type_: CompilerFlagType::Linker,
        },
    ),
    (
        "-Wl,",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Partial, false),
            type_: CompilerFlagType::Linker,
        },
    ),
    (
        "-u",
        FlagDefinition {
            consumption: Instruction::new(1, Match::Exact, false),
            type_: CompilerFlagType::Linker,
        },
    ),
    (
        "-z",
        FlagDefinition {
            consumption: Instruction::new(1, Match::Exact, false),
            type_: CompilerFlagType::Linker,
        },
    ),
    (
        "-Xassembler",
        FlagDefinition {
            consumption: Instruction::new(1, Match::Exact, false),
            type_: CompilerFlagType::Other,
        },
    ),
    (
        "-Wa,",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Partial, false),
            type_: CompilerFlagType::Other,
        },
    ),
    (
        "-ansi",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Exact, false),
            type_: CompilerFlagType::Other,
        },
    ),
    (
        "-aux-info",
        FlagDefinition {
            consumption: Instruction::new(1, Match::Exact, false),
            type_: CompilerFlagType::Other,
        },
    ),
    (
        "-std",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Partial, true),
            type_: CompilerFlagType::Other,
        },
    ),
    (
        "-O",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Both, false),
            type_: CompilerFlagType::Other,
        },
    ),
    (
        "-g",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Both, false),
            type_: CompilerFlagType::Other,
        },
    ),
    (
        "-f",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Partial, false),
            type_: CompilerFlagType::Other,
        },
    ),
    (
        "-m",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Partial, false),
            type_: CompilerFlagType::Other,
        },
    ),
    (
        "-p",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Partial, false),
            type_: CompilerFlagType::Other,
        },
    ),
    (
        "-W",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Partial, false),
            type_: CompilerFlagType::Other,
        },
    ),
    (
        "-no",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Partial, false),
            type_: CompilerFlagType::Other,
        },
    ),
    (
        "-tno",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Partial, false),
            type_: CompilerFlagType::Other,
        },
    ),
    (
        "-save",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Partial, false),
            type_: CompilerFlagType::Other,
        },
    ),
    (
        "-d",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Partial, false),
            type_: CompilerFlagType::Other,
        },
    ),
    (
        "-E",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Partial, false),
            type_: CompilerFlagType::Other,
        },
    ),
    (
        "-Q",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Partial, false),
            type_: CompilerFlagType::Other,
        },
    ),
    (
        "-X",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Partial, false),
            type_: CompilerFlagType::Other,
        },
    ),
    (
        "-Y",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Partial, false),
            type_: CompilerFlagType::Other,
        },
    ),
    (
        "--",
        FlagDefinition {
            consumption: Instruction::new(0, Match::Partial, false),
            type_: CompilerFlagType::Other,
        },
    ),
];
