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

use std::{error::Error, ffi::OsStr, path::Path};

use crate::{Execution, resolver::{DefaultResolver, Resolver}, semantic::Semantic};

use super::{Tool, tool_gcc::compilation};

#[derive(Debug, Clone, Copy, Default)]
pub struct ToolWrapper;

impl Tool for ToolWrapper {
    fn recognize(&self, execution: &Execution) -> Result<Option<Semantic>, Box<dyn Error>> {
        if is_ccache_call(&execution.executable) {
            if is_ccache_query(&execution.arguments) {
                return Ok(Some(Semantic::QueryCompiler));
            } else {
                compilation(&remove_wrapper(execution))
            }
        } else if is_distcc_call(&execution.executable) {
            if is_distcc_query(&execution.arguments) {
                return Ok(Some(Semantic::QueryCompiler));
            } else {
                compilation(&remove_wrapper(execution))
            }
        } else {
            return Ok(None);
        }
    }
}

fn is_ccache_call(program: impl AsRef<Path>) -> bool {
    if let Some(string) = program.as_ref().file_stem() {
        string == "ccache"
    } else {
        false
    }
}

fn is_ccache_query(arguments: &[String]) -> bool {
    if arguments.len() <= 1 {
        return true;
    }

    let second = &arguments[1];
    if looks_like_ccache_parameter(second) {
        return true;
    }

    false
}

fn looks_like_ccache_parameter(candidate: impl AsRef<OsStr>) -> bool {
    let candidate = candidate.as_ref();

    if let Some(first) = candidate.to_string_lossy().chars().nth(0) {
        first == '-'
    } else {
        true
    }
}

fn is_distcc_call(program: impl AsRef<Path>) -> bool {
    if let Some(string) = program.as_ref().file_stem() {
        string == "distcc"
    } else {
        false
    }
}

fn is_distcc_query(arguments: &[String]) -> bool {
    if arguments.len() <= 1 {
        return true;
    }

    let second = &arguments[1];
    if looks_like_distcc_parameter(second) {
        return true;
    }

    false
}

fn looks_like_distcc_parameter(candidate: impl AsRef<OsStr>) -> bool {
    let candidate = candidate.as_ref();

    if candidate.is_empty() {
        return true;
    }

    static FLAGS: [&str; 6] = [
        "--help",
        "--version",
        "--show-hosts",
        "--scan-includes",
        "-j",
        "--show-principal",
    ];

    FLAGS
        .iter()
        .find(|flag| OsStr::new(flag) == candidate)
        .is_some()
}

fn remove_wrapper(execution: &Execution) -> Execution {
    let resolver = DefaultResolver::default();
    remove_wrapper_with_resolver(&resolver, execution)
}

fn remove_wrapper_with_resolver(resolver: &impl Resolver, execution: &Execution) -> Execution {
    let environment = &execution.environment;

    if let Some(path) = environment.get(OsStr::new("PATH")) {
        // take the second argument as a program candidate
        if let Some(program) = execution.arguments.iter().nth(1) {
            // use resolver to get the full path to the executable.
            if let Ok(candidate) = resolver.from_search_path(program.as_ref(), path.as_ref()) {
                let mut copy = execution.clone();
                copy.arguments.remove(0);
                copy.executable = candidate;
                return copy;
            }
        }
    }

    // fall back to the second argument as an executable.
    let mut copy = execution.clone();
    copy.arguments.remove(0);
    copy.executable = copy.arguments.first().unwrap().into();
    copy
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        ffi::OsStr,
        io,
        path::{Path, PathBuf},
    };

    use crate::{resolver::MockResolver, Execution};

    #[test]
    fn is_ccache_call() {
        assert!(!super::is_ccache_call("cc"));
        assert!(!super::is_ccache_call("/usr/bin/cc"));
        assert!(!super::is_ccache_call("gcc"));
        assert!(!super::is_ccache_call("/usr/bin/gcc"));
        assert!(!super::is_ccache_call("c++"));
        assert!(!super::is_ccache_call("/usr/bin/c++"));
        assert!(!super::is_ccache_call("g++"));
        assert!(!super::is_ccache_call("/usr/bin/g++"));

        assert!(super::is_ccache_call("ccache"));
    }

    #[test]
    fn is_ccache_query() {
        assert!(super::is_ccache_query(&["ccache".to_string()]));
        assert!(super::is_ccache_query(&[
            "ccache".to_string(),
            "-c".to_string()
        ]));
        assert!(super::is_ccache_query(&[
            "ccache".to_string(),
            "--cleanup".to_string()
        ]));

        assert!(!super::is_ccache_query(&[
            "ccache".to_string(),
            "cc".to_string(),
            "-c".to_string()
        ]));
    }

    #[test]
    fn is_distcc_call() {
        assert!(!super::is_distcc_call("cc"));
        assert!(!super::is_distcc_call("/usr/bin/cc"));
        assert!(!super::is_distcc_call("gcc"));
        assert!(!super::is_distcc_call("/usr/bin/gcc"));
        assert!(!super::is_distcc_call("c++"));
        assert!(!super::is_distcc_call("/usr/bin/c++"));
        assert!(!super::is_distcc_call("g++"));
        assert!(!super::is_distcc_call("/usr/bin/g++"));

        assert!(super::is_distcc_call("distcc"));
    }

    #[test]
    fn is_distcc_query() {
        assert!(super::is_distcc_query(&["distcc".to_string()]));
        assert!(super::is_distcc_query(&[
            "distcc".to_string(),
            "--help".to_string()
        ]));
        assert!(super::is_distcc_query(&[
            "distcc".to_string(),
            "--show-hosts".to_string()
        ]));
        assert!(super::is_distcc_query(&[
            "distcc".to_string(),
            "-j".to_string()
        ]));

        assert!(!super::is_distcc_query(&[
            "distcc".to_string(),
            "cc".to_string(),
            "--help".to_string()
        ]));
        assert!(!super::is_distcc_query(&[
            "distcc".to_string(),
            "cc".to_string(),
            "-c".to_string()
        ]));
    }

    #[test]
    fn remove_wrapper() {
        let mut environment = HashMap::new();
        environment.insert("PATH".into(), "/usr/bin;/usr/sbin".into());

        let input = Execution {
            executable: PathBuf::from("/usr/bin/ccache"),
            arguments: ["ccache", "cc", "-c", "-o", "source.o", "source.c"]
                .iter()
                .copied()
                .map(String::from)
                .collect(),
            working_dir: PathBuf::from("/home/user/project"),
            environment: environment.clone(),
        };

        let expected = Execution {
            executable: PathBuf::from("/usr/bin/cc"),
            arguments: ["cc", "-c", "-o", "source.o", "source.c"]
                .iter()
                .copied()
                .map(String::from)
                .collect(),
            working_dir: PathBuf::from("/home/user/project"),
            environment: environment,
        };

        let mut mock = MockResolver::new();
        mock.expect_from_search_path()
            .withf(|file: &Path, _search_path: &Path| file == OsStr::new("cc"))
            .times(1)
            .returning(|_, _| Ok("/usr/bin/cc".into()));

        let result = super::remove_wrapper_with_resolver(&mock, &input);
        assert_eq!(expected, result);
    }

    #[test]
    fn remove_wrapper_fails_to_resolve() {
        let mut environment = HashMap::new();
        environment.insert("PATH".into(), "/usr/bin;/usr/sbin".into());

        let input = Execution {
            executable: PathBuf::from("/usr/bin/ccache"),
            arguments: ["ccache", "cc", "-c", "-o", "source.o", "source.c"]
                .iter()
                .copied()
                .map(String::from)
                .collect(),
            working_dir: PathBuf::from("/home/user/project"),
            environment: environment.clone(),
        };

        let expected = Execution {
            executable: PathBuf::from("cc"),
            arguments: ["cc", "-c", "-o", "source.o", "source.c"]
                .iter()
                .copied()
                .map(String::from)
                .collect(),
            working_dir: PathBuf::from("/home/user/project"),
            environment: environment,
        };

        let mut mock = MockResolver::new();
        mock.expect_from_search_path()
            .withf(|file: &Path, _search_path: &Path| file == OsStr::new("cc"))
            .times(1)
            .returning(|_, _| Err(io::Error::from(io::ErrorKind::NotFound)));

        let result = super::remove_wrapper_with_resolver(&mock, &input);
        assert_eq!(expected, result);
    }
}
