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

use std::error::Error;

use crate::{semantic::Semantic, Execution};

pub mod tool_any;
pub mod tool_clang;
pub mod tool_cuda;
pub mod tool_extending_wrapper;
pub mod tool_gcc;
pub mod tool_wrapper;

pub trait Tool {
    fn recognize(&self, execution: &Execution) -> Result<Option<Semantic>, Box<dyn Error>>;
}

pub fn recognized_ok(result: &Result<Option<Semantic>, Box<dyn Error>>) -> bool {
    if let Ok(semantic) = result {
        semantic.is_some()
    } else {
        false
    }
}

pub fn recognized_with_error(result: &Result<Option<Semantic>, Box<dyn Error>>) -> bool {
    result.is_err()
}

pub fn not_recognized(result: &Result<Option<Semantic>, Box<dyn Error>>) -> bool {
    if let Ok(semantic) = result {
        semantic.is_none()
    } else {
        false
    }
}
