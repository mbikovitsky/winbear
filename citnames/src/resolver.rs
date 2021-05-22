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
    collections::HashMap,
    ffi::{OsStr, OsString},
    fs, io,
    path::{Path, PathBuf},
};

#[cfg(test)]
use mockall::automock;

#[cfg_attr(test, automock)]
pub trait Resolver {
    fn from_current_directory(&self, file: &Path) -> io::Result<PathBuf>;

    fn from_path(
        &self,
        file: &Path,
        environment: &HashMap<OsString, OsString>,
    ) -> io::Result<PathBuf>;

    fn from_search_path(&self, file: &Path, search_path: &Path) -> io::Result<PathBuf>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DefaultResolver;

impl Resolver for DefaultResolver {
    fn from_current_directory(&self, file: &Path) -> io::Result<PathBuf> {
        let result = fs::canonicalize(file)?;
        if result.exists() {
            Ok(result)
        } else {
            Err(io::Error::from(io::ErrorKind::NotFound))
        }
    }

    fn from_path(
        &self,
        file: &Path,
        environment: &HashMap<OsString, OsString>,
    ) -> io::Result<PathBuf> {
        if file.components().count() != 0 {
            return self.from_current_directory(file);
        }

        let paths = environment.get(OsStr::new("PATH"));
        if let Some(paths) = paths {
            return self.from_search_path(file, paths.as_ref());
        }

        Err(io::Error::from(io::ErrorKind::NotFound))
    }

    fn from_search_path(&self, file: &Path, search_path: &Path) -> io::Result<PathBuf> {
        if file.components().count() != 0 {
            return self.from_current_directory(file);
        }

        for path in std::env::split_paths(search_path.as_os_str()) {
            if path.as_os_str().is_empty() {
                continue;
            }

            let candidate = path.join(file);
            if let Ok(result) = self.from_current_directory(&candidate) {
                return Ok(result);
            }
        }

        Err(io::Error::from(io::ErrorKind::NotFound))
    }
}
