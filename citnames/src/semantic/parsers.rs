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

use std::{collections::BTreeMap, error::Error};

use itertools::Itertools;

use crate::Execution;

#[derive(Debug, Clone, Copy)]
pub struct FlagDefinition {
    pub consumption: Instruction,
    pub type_: CompilerFlagType,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompilerFlagType {
    KindOfOutput,
    KindOfOutputNoLinking,
    KindOfOutputInfo,
    KindOfOutputOutput,
    Preprocessor,
    PreprocessorMake,
    Linker,
    LinkerObjectFile,
    DirectorySearch,
    DirectorySearchLinker,
    Source,
    Other,
}

#[derive(Debug, Clone)]
pub struct CompilerFlag {
    pub arguments: Vec<String>,
    pub type_: CompilerFlagType,
}

#[derive(Debug, Clone, Copy)]
pub struct Instruction {
    count: u8,
    match_: Match,
    equal: bool,
}

impl Instruction {
    pub const fn new(count: u8, match_: Match, equal: bool) -> Self {
        Self {
            count,
            match_,
            equal,
        }
    }

    pub fn count(&self, exact_match: bool) -> usize {
        if self.count > 0 {
            if exact_match {
                self.count
            } else {
                self.count - 1
            }
        } else {
            self.count
        }
        .into()
    }

    pub const fn exact_match_allowed(&self) -> bool {
        match self.match_ {
            Match::Exact | Match::Both => true,
            _ => false,
        }
    }

    pub const fn partial_match_allowed(&self) -> bool {
        match self.match_ {
            Match::Partial | Match::Both => true,
            _ => false,
        }
    }

    pub const fn equal(&self) -> bool {
        self.equal
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Match {
    Exact,
    Partial,
    Both,
}

pub trait Parser: Sync {
    fn parse<'a, 'b: 'a>(
        &self,
        input: &'a [&'b str],
    ) -> Result<(CompilerFlag, &'a [&'b str]), &'a [&'b str]>;
}

pub trait FullParser: Sync {
    fn parse<'a, 'b: 'a>(&self, input: &'a [&'b str]) -> Result<Vec<CompilerFlag>, &'a [&'b str]>;
}

#[derive(Debug, Clone, Copy)]
pub struct FlagParser<'a> {
    flags: &'a BTreeMap<&'a str, FlagDefinition>,
}

impl<'a> FlagParser<'a> {
    pub fn new(flags: &'a BTreeMap<&'a str, FlagDefinition>) -> Self {
        Self { flags }
    }

    fn lookup(&self, key: &str) -> Option<(usize, CompilerFlagType)> {
        // try to find if the key has an associated instruction
        if let Some(candidate) = self.flags.iter().skip_while(|(&k, _)| k < key).nth(0) {
            let candidate = (*candidate.0, candidate.1);

            // exact matches are preferred in all cases.
            if let Some(result) = Self::check_equal(key, &candidate) {
                return Some(result);
            }

            // check if the argument is allowed to stick to the flag
            if let Some(result) = Self::check_partial(key, &candidate) {
                return Some(result);
            }

            // check if this is the first element or not.
            if !self
                .flags
                .keys()
                .nth(0)
                .map(|&first| first == candidate.0)
                .unwrap_or(false)
            {
                let previous = self
                    .flags
                    .iter()
                    .take_while(|(&k, _)| k < key)
                    .last()
                    .unwrap();
                let previous = (*previous.0, previous.1);
                if let Some(result) = Self::check_partial(key, &previous) {
                    return Some(result);
                }
            }
        }

        // check if the last element is not the one we are looking for.
        // (this is a limitation of `lower_bound` method.)
        if let Some(candidate) = self.flags.iter().last() {
            let candidate = (*candidate.0, candidate.1);
            if let Some(result) = Self::check_partial(key, &candidate) {
                return Some(result);
            }
        }

        None
    }

    fn check_equal(
        key: &str,
        candidate: &(&'a str, &'a FlagDefinition),
    ) -> Option<(usize, CompilerFlagType)> {
        if key.is_empty() {
            return None;
        }

        if candidate.0 != key {
            return None;
        }

        if !candidate.1.consumption.exact_match_allowed() {
            return None;
        }

        let instruction = candidate.1;
        Some((instruction.consumption.count(true), instruction.type_))
    }

    fn check_partial(
        key: &str,
        candidate: &(&'a str, &'a FlagDefinition),
    ) -> Option<(usize, CompilerFlagType)> {
        if key.is_empty() {
            return None;
        }

        if !candidate.1.consumption.partial_match_allowed() {
            return None;
        }

        let length = key.len().min(candidate.0.len());

        // TODO: make extra check on equal sign
        // TODO: make extra check on mandatory following characters

        if &key.as_bytes()[..length] != &candidate.0.as_bytes()[..length] {
            return None;
        }

        let instruction = candidate.1;
        Some((instruction.consumption.count(false), instruction.type_))
    }
}

impl<'a> Parser for FlagParser<'a> {
    fn parse<'b, 'c: 'b>(
        &self,
        input: &'b [&'c str],
    ) -> Result<(CompilerFlag, &'b [&'c str]), &'b [&'c str]> {
        if input.is_empty() {
            return Err(input);
        }

        let key = input[0];

        if let Some((count, type_)) = self.lookup(key) {
            return Ok(parse_result(input, count + 1, type_));
        }

        Err(input)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SourceMatcher;

impl SourceMatcher {
    const EXTENSIONS: [&'static str; 53] = [
        // header files
        ".h", ".hh", ".H", ".hp", ".hxx", ".hpp", ".HPP", ".h++", ".tcc", // C
        ".c", ".C", // C++
        ".cc", ".CC", ".c++", ".C++", ".cxx", ".cpp", ".cp", // CUDA
        ".cu", // ObjectiveC
        ".m", ".mi", ".mm", ".M", ".mii", // Preprocessed
        ".i", ".ii", // Assembly
        ".s", ".S", ".sx", ".asm", // Fortran
        ".f", ".for", ".ftn", ".F", ".FOR", ".fpp", ".FPP", ".FTN", ".f90", ".f95", ".f03", ".f08",
        ".F90", ".F95", ".F03", ".F08",  // go
        ".go",   // brig
        ".brig", // D
        ".d", ".di", ".dd", // Ada
        ".ads", ".abd",
    ];

    fn take_extension(file: &str) -> &str {
        if let Some(index) = file.rfind('.') {
            &file[index..]
        } else {
            file
        }
    }
}

impl Parser for SourceMatcher {
    fn parse<'a, 'b: 'a>(
        &self,
        input: &'a [&'b str],
    ) -> Result<(CompilerFlag, &'a [&'b str]), &'a [&'b str]> {
        let candidate = Self::take_extension(input[0]);

        for &extension in &Self::EXTENSIONS {
            if candidate == extension {
                return Ok(parse_result(input, 1, CompilerFlagType::Source));
            }
        }

        Err(input)
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct EverythingElseFlagMatcher;

impl Parser for EverythingElseFlagMatcher {
    fn parse<'a, 'b: 'a>(
        &self,
        input: &'a [&'b str],
    ) -> Result<(CompilerFlag, &'a [&'b str]), &'a [&'b str]> {
        let front = input[0];

        if !front.is_empty() {
            return Ok(parse_result(input, 1, CompilerFlagType::LinkerObjectFile));
        }

        Err(input)
    }
}

fn parse_result<'a, 'b: 'a>(
    input: &'a [&'b str],
    count: usize,
    type_: CompilerFlagType,
) -> (CompilerFlag, &'a [&'b str]) {
    let (arguments, remainder) = input.split_at(count);

    let compiler_flag = CompilerFlag {
        arguments: arguments.iter().copied().map(String::from).collect(),
        type_,
    };

    (compiler_flag, remainder)
}

pub struct OneOf<const COUNT: usize> {
    parsers: [Box<dyn Parser>; COUNT],
}

impl<const COUNT: usize> OneOf<COUNT> {
    pub fn new(parsers: [Box<dyn Parser>; COUNT]) -> Self {
        Self { parsers }
    }
}

impl<const COUNT: usize> Parser for OneOf<COUNT> {
    fn parse<'a, 'b: 'a>(
        &self,
        input: &'a [&'b str],
    ) -> Result<(CompilerFlag, &'a [&'b str]), &'a [&'b str]> {
        if let Some(result) = self
            .parsers
            .iter()
            .filter_map(|parser| parser.parse(input).ok())
            .nth(0)
        {
            Ok(result)
        } else {
            Err(input)
        }
    }
}

pub struct Repeat<P: Parser> {
    parser: P,
}

impl<P: Parser> Repeat<P> {
    pub fn new(parser: P) -> Self {
        Self { parser }
    }
}

impl<P: Parser> FullParser for Repeat<P> {
    fn parse<'a, 'b: 'a>(&self, input: &'a [&'b str]) -> Result<Vec<CompilerFlag>, &'a [&'b str]> {
        let mut flags = vec![];
        let mut it = input;
        while !it.is_empty() {
            if let Ok((flag, remainder)) = self.parser.parse(it) {
                flags.push(flag);
                it = remainder;
            } else {
                break;
            }
        }

        if it.is_empty() {
            Ok(flags)
        } else {
            Err(it)
        }
    }
}

pub fn parse(
    parser: &impl FullParser,
    execution: &Execution,
) -> Result<Vec<CompilerFlag>, Box<dyn Error>> {
    if execution.arguments.is_empty() {
        return Err("Failed to recognize: no arguments found.")?;
    }

    let input: Vec<_> = execution.arguments[1..]
        .iter()
        .map(String::as_str)
        .collect();

    parser
        .parse(&input)
        .map_err(|remainder| format!("Failed to recognize: {}", remainder.iter().join(", ")).into())
}
