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
    borrow::Cow,
    collections::{hash_map::DefaultHasher, HashSet},
    convert::TryFrom,
    error::Error,
    hash::{Hash, Hasher},
    path::Path,
};

use itertools::Itertools;
use serde::{Deserialize, Serialize};

use util::{command_line_to_argv, quote_argument};

use crate::{
    configuration::{Content, Format},
    Entry,
};

#[derive(Debug, Clone)]
pub struct CompilationDatabase {
    format: Format,
    content: Content,
}

impl CompilationDatabase {
    pub fn new(format: Format, content: Content) -> Self {
        Self { format, content }
    }

    pub fn to_json<'a>(
        &self,
        entries: impl IntoIterator<Item = &'a Entry>,
    ) -> Result<String, Box<dyn Error>> {
        let mut content_filter = ContentFilter::new(self.content.clone());
        let mut duplicate_filter = DuplicateFilter::new();

        let filtered: Vec<_> = entries
            .into_iter()
            .filter(|entry| content_filter.apply(entry))
            .filter(|entry| duplicate_filter.apply(entry))
            .map(|entry| {
                let mut serializable = SerializableEntry::from(entry);
                if self.format.drop_output_field {
                    serializable.output = None;
                }
                if self.format.command_as_array {
                    assert!(serializable.arguments.is_arguments());
                } else {
                    serializable.arguments =
                        SerializableArguments::Command(serializable.arguments.into_command_line());
                }
                serializable
            })
            .collect();

        Ok(serde_json::to_string_pretty(&filtered)?)
    }

    pub fn from_json(&self, input: impl AsRef<str>) -> Result<Vec<Entry>, Box<dyn Error>> {
        let result: Vec<SerializableEntry> = serde_json::from_str(input.as_ref())?;
        let result: Result<Vec<Entry>, _> = result.into_iter().map(Entry::try_from).collect();
        let result = result?;

        for entry in &result {
            if entry.file.as_os_str().is_empty() {
                return Err("Field 'file' is empty string.")?;
            }
            if entry.directory.as_os_str().is_empty() {
                return Err("Field 'directory' is empty string.")?;
            }
            if let Some(output) = &entry.output {
                if output.as_os_str().is_empty() {
                    return Err("Field 'output' is empty string.")?;
                }
            }
            if entry.arguments.is_empty() {
                return Err("Field 'arguments' is empty list.")?;
            }
        }

        Ok(result)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct SerializableEntry<'a> {
    #[serde(borrow)]
    pub file: Cow<'a, str>,

    #[serde(borrow)]
    pub directory: Cow<'a, str>,

    #[serde(borrow)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub output: Option<Cow<'a, str>>,

    #[serde(borrow)]
    #[serde(flatten)]
    pub arguments: SerializableArguments<'a>,
}

impl<'a> From<&'a Entry> for SerializableEntry<'a> {
    fn from(entry: &'a Entry) -> Self {
        Self {
            file: entry.file.to_string_lossy(),
            directory: entry.directory.to_string_lossy(),
            output: entry.output.as_ref().map(|output| output.to_string_lossy()),
            arguments: SerializableArguments::Arguments(
                entry
                    .arguments
                    .iter()
                    .map(|argument| Cow::Borrowed(argument.as_str()))
                    .collect(),
            ),
        }
    }
}

impl<'a> TryFrom<SerializableEntry<'a>> for Entry {
    type Error = Box<dyn Error>;

    fn try_from(entry: SerializableEntry<'a>) -> Result<Self, Self::Error> {
        Ok(Self {
            file: entry.file.into_owned().into(),
            directory: entry.directory.into_owned().into(),
            output: entry.output.map(|output| output.into_owned().into()),
            arguments: entry
                .arguments
                .into_arguments()?
                .into_iter()
                .map(|arg| arg.into_owned())
                .collect(),
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
enum SerializableArguments<'a> {
    #[serde(borrow)]
    Arguments(Vec<Cow<'a, str>>),

    #[serde(borrow)]
    Command(Cow<'a, str>),
}

impl<'a> SerializableArguments<'a> {
    pub fn is_arguments(&self) -> bool {
        match self {
            SerializableArguments::Arguments(_) => true,
            SerializableArguments::Command(_) => false,
        }
    }

    pub fn into_arguments(self) -> Result<Vec<Cow<'a, str>>, Box<dyn Error>> {
        Ok(match self {
            SerializableArguments::Arguments(arguments) => arguments,
            SerializableArguments::Command(command) => command_line_to_argv(command.as_ref())?
                .into_iter()
                .map(|arg| Cow::Owned(arg.into_string().unwrap()))
                .collect(),
        })
    }

    pub fn into_command_line(self) -> Cow<'a, str> {
        match self {
            SerializableArguments::Arguments(arguments) => arguments
                .into_iter()
                .map(|arg| quote_argument(arg, false))
                .join(" ")
                .into(),
            SerializableArguments::Command(command) => command,
        }
    }
}

trait Filter {
    fn apply(&mut self, entry: &Entry) -> bool;
}

#[derive(Debug, Clone)]
struct ContentFilter {
    config: Content,
}

impl ContentFilter {
    pub fn new(config: Content) -> Self {
        Self { config }
    }

    fn contains<I>(&self, root: I, file: impl AsRef<Path>) -> bool
    where
        I: IntoIterator,
        I::Item: AsRef<Path>,
    {
        root.into_iter().any(|directory| {
            // the file is contained in the directory if all path elements are
            // in the file paths too.
            directory
                .as_ref()
                .components()
                .zip(file.as_ref().components())
                .all(|(a, b)| a == b)
        })
    }
}

impl Filter for ContentFilter {
    fn apply(&mut self, entry: &Entry) -> bool {
        // if no check required, accept every entry.
        if !self.config.include_only_existing_source {
            return true;
        }

        let exists = entry.file.exists();

        let include = &self.config.paths_to_include;
        let to_include = include.is_empty() || self.contains(include, &entry.file);

        let exclude = &self.config.paths_to_exclude;
        let to_exclude = !exclude.is_empty() && self.contains(exclude, &entry.file);

        exists && to_include && !to_exclude
    }
}

#[derive(Debug, Clone)]
struct DuplicateFilter {
    hashes: HashSet<String>,
}

impl DuplicateFilter {
    pub fn new() -> Self {
        Default::default()
    }

    // The hash function based on all attributes.
    //
    // - It shall ignore the compiler name, but count all compiler flags in.
    // - Same compiler call semantic is detected by filter out the irrelevant flags.
    fn hash(entry: &Entry) -> String {
        let file = entry.file.to_str().unwrap(); // TODO
        let file: String = file.chars().rev().collect();

        let args_hash = {
            let mut hasher = DefaultHasher::new();
            entry
                .arguments
                .iter()
                .skip(1)
                .rev()
                .format(",")
                .to_string()
                .hash(&mut hasher);
            hasher.finish()
        };

        format!("{}:{}", args_hash, file)
    }
}

impl Default for DuplicateFilter {
    fn default() -> Self {
        Self {
            hashes: HashSet::new(),
        }
    }
}

impl Filter for DuplicateFilter {
    fn apply(&mut self, entry: &Entry) -> bool {
        let h2 = Self::hash(entry);
        self.hashes.insert(h2)
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        configuration::{Content, Format},
        output::CompilationDatabase,
        Entry,
    };

    const AS_ARGUMENTS: Format = Format {
        command_as_array: true,
        drop_output_field: false,
    };

    const AS_COMMAND: Format = Format {
        command_as_array: false,
        drop_output_field: false,
    };

    const AS_ARGUMENTS_NO_OUTPUT: Format = Format {
        command_as_array: true,
        drop_output_field: true,
    };

    const AS_COMMAND_NO_OUTPUT: Format = Format {
        command_as_array: false,
        drop_output_field: true,
    };

    const NO_FILTER: Content = Content {
        include_only_existing_source: false,
        paths_to_include: vec![],
        paths_to_exclude: vec![],
    };

    fn value_serialized_and_read_back(input: &[Entry], expected: &[Entry], format: &Format) {
        let sut = CompilationDatabase::new(*format, NO_FILTER);

        let serialized = sut.to_json(input).unwrap();

        let deserialized = sut.from_json(&serialized).unwrap();

        assert_eq!(expected, deserialized);
    }

    #[test]
    fn empty_value_serialized_and_read_back() {
        value_serialized_and_read_back(&[], &[], &AS_ARGUMENTS);
        value_serialized_and_read_back(&[], &[], &AS_COMMAND);
    }

    #[test]
    fn same_entries_read_back() {
        let expected = vec![
            Entry {
                file: "entry_one.c".into(),
                directory: "/path/to".into(),
                output: None,
                arguments: vec!["cc".into(), "-c".into(), "entry_one.c".into()],
            },
            Entry {
                file: "entry_two.c".into(),
                directory: "/path/to".into(),
                output: None,
                arguments: vec!["cc".into(), "-c".into(), "entry_two.c".into()],
            },
            Entry {
                file: "entries.c".into(),
                directory: "/path/to".into(),
                output: Some("entries.o".into()),
                arguments: vec![
                    "cc".into(),
                    "-c".into(),
                    "-o".into(),
                    "entries.o".into(),
                    "entries.c".into(),
                ],
            },
        ];

        value_serialized_and_read_back(&expected, &expected, &AS_ARGUMENTS);
        value_serialized_and_read_back(&expected, &expected, &AS_COMMAND);
    }

    #[test]
    fn entries_without_output_read_back() {
        let input = vec![
            Entry {
                file: "entry_one.c".into(),
                directory: "/path/to".into(),
                output: None,
                arguments: vec!["cc".into(), "-c".into(), "entry_one.c".into()],
            },
            Entry {
                file: "entry_two.c".into(),
                directory: "/path/to".into(),
                output: None,
                arguments: vec!["cc".into(), "-c".into(), "entry_two.c".into()],
            },
            Entry {
                file: "entries.c".into(),
                directory: "/path/to".into(),
                output: Some("entries.o".into()),
                arguments: vec![
                    "cc".into(),
                    "-c".into(),
                    "-o".into(),
                    "entries.o".into(),
                    "entries.c".into(),
                ],
            },
        ];

        let expected = vec![
            Entry {
                file: "entry_one.c".into(),
                directory: "/path/to".into(),
                output: None,
                arguments: vec!["cc".into(), "-c".into(), "entry_one.c".into()],
            },
            Entry {
                file: "entry_two.c".into(),
                directory: "/path/to".into(),
                output: None,
                arguments: vec!["cc".into(), "-c".into(), "entry_two.c".into()],
            },
            Entry {
                file: "entries.c".into(),
                directory: "/path/to".into(),
                output: None,
                arguments: vec![
                    "cc".into(),
                    "-c".into(),
                    "-o".into(),
                    "entries.o".into(),
                    "entries.c".into(),
                ],
            },
        ];

        value_serialized_and_read_back(&input, &expected, &AS_ARGUMENTS_NO_OUTPUT);
        value_serialized_and_read_back(&input, &expected, &AS_COMMAND_NO_OUTPUT);
    }

    #[test]
    fn merged_entries_read_back() {
        let input = vec![
            Entry {
                file: "entry_one.c".into(),
                directory: "/path/to".into(),
                output: None,
                arguments: vec!["cc".into(), "-c".into(), "entry_one.c".into()],
            },
            Entry {
                file: "entry_two.c".into(),
                directory: "/path/to".into(),
                output: None,
                arguments: vec!["cc".into(), "-c".into(), "entry_two.c".into()],
            },
            Entry {
                file: "entry_one.c".into(),
                directory: "/path/to".into(),
                output: None,
                arguments: vec!["cc1".into(), "-c".into(), "entry_one.c".into()],
            },
            Entry {
                file: "entry_two.c".into(),
                directory: "/path/to".into(),
                output: None,
                arguments: vec!["cc1".into(), "-c".into(), "entry_two.c".into()],
            },
        ];

        let expected = vec![
            Entry {
                file: "entry_one.c".into(),
                directory: "/path/to".into(),
                output: None,
                arguments: vec!["cc".into(), "-c".into(), "entry_one.c".into()],
            },
            Entry {
                file: "entry_two.c".into(),
                directory: "/path/to".into(),
                output: None,
                arguments: vec!["cc".into(), "-c".into(), "entry_two.c".into()],
            },
        ];

        value_serialized_and_read_back(&input, &expected, &AS_ARGUMENTS);
        value_serialized_and_read_back(&input, &expected, &AS_COMMAND);
        value_serialized_and_read_back(&input, &expected, &AS_ARGUMENTS_NO_OUTPUT);
        value_serialized_and_read_back(&input, &expected, &AS_COMMAND_NO_OUTPUT);
    }

    #[test]
    fn deserialize_fails_with_empty_stream() {
        let sut = CompilationDatabase::new(AS_COMMAND, NO_FILTER);

        assert!(sut.from_json("").is_err());
    }

    #[test]
    fn deserialize_fails_with_missing_fields() {
        let sut = CompilationDatabase::new(AS_COMMAND, NO_FILTER);

        assert!(sut.from_json("[ { } ]").is_err());
    }

    #[test]
    fn deserialize_fails_with_empty_fields() {
        let sut = CompilationDatabase::new(AS_COMMAND, NO_FILTER);

        let json = r#"[ { "file": "file.c", "directory": "", "command": "cc -c file.c" } ]"#;

        assert!(sut.from_json(json).is_err());
    }
}
