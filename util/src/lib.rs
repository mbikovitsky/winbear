/*
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
    convert::TryInto,
    error::Error,
    ffi::{OsStr, OsString},
};

use bindings::Windows::Win32::{
    System::{
        Diagnostics::Debug::FACILITY_NT_BIT,
        Memory::LocalFree,
        SystemServices::{NTSTATUS, PWSTR, VER_GREATER_EQUAL},
        WindowsProgramming::{
            VerSetConditionMask, VerifyVersionInfoW, OSVERSIONINFOEXW, VER_MAJORVERSION,
            VER_MINORVERSION, VER_SERVICEPACKMAJOR,
        },
    },
    UI::Shell::CommandLineToArgvW,
};
use widestring::{U16CStr, U16CString};
use windows::HRESULT;

pub fn nt_success(status: NTSTATUS) -> bool {
    status.0 >= 0
}

pub fn hresult_from_nt(status: NTSTATUS) -> HRESULT {
    // https://docs.microsoft.com/en-us/windows/win32/api/winerror/nf-winerror-hresult_from_nt
    HRESULT(status.0 as u32 | FACILITY_NT_BIT.0)
}

pub fn is_windows_vista_or_greater() -> bool {
    is_windows_version_or_greater(6, 0, 0)
}

fn is_windows_version_or_greater(
    major_version: u16,
    minor_version: u16,
    service_pack_major: u16,
) -> bool {
    unsafe {
        let mut version_info = OSVERSIONINFOEXW {
            dwOSVersionInfoSize: std::mem::size_of::<OSVERSIONINFOEXW>().try_into().unwrap(),
            dwMajorVersion: major_version.into(),
            dwMinorVersion: minor_version.into(),
            wServicePackMajor: service_pack_major,
            ..Default::default()
        };

        let ver_greater_equal = VER_GREATER_EQUAL.try_into().unwrap();

        let mask = VerSetConditionMask(0, VER_MAJORVERSION, ver_greater_equal);
        let mask = VerSetConditionMask(mask, VER_MINORVERSION, ver_greater_equal);
        let mask = VerSetConditionMask(mask, VER_SERVICEPACKMAJOR, ver_greater_equal);

        VerifyVersionInfoW(
            &mut version_info,
            VER_MAJORVERSION | VER_MINORVERSION | VER_SERVICEPACKMAJOR,
            mask,
        )
        .as_bool()
    }
}

pub fn command_line_to_argv(
    command_line: impl AsRef<OsStr>,
) -> Result<Vec<OsString>, Box<dyn Error>> {
    unsafe {
        let command_line = U16CString::from_os_str(command_line)?;
        let mut command_line = command_line.into_vec_with_nul();

        let mut argc = 0;
        let argv = CommandLineToArgvW(PWSTR(command_line.as_mut_ptr()), &mut argc);
        if argv.is_null() {
            return Err(windows::Error::from(HRESULT::from_thread()))?;
        }

        let result = {
            let argv_slice = std::slice::from_raw_parts(argv, argc.try_into().unwrap());
            argv_slice
                .iter()
                .map(|argument_ptr| U16CStr::from_ptr_str(argument_ptr.0).to_os_string())
                .collect()
        };

        LocalFree(argv as _);

        Ok(result)
    }
}

pub fn quote_argument(argument: impl AsRef<str>, force: bool) -> String {
    let argument = argument.as_ref();

    // Adapted from:
    // https://docs.microsoft.com/en-us/archive/blogs/twistylittlepassagesallalike/everyone-quotes-command-line-arguments-the-wrong-way

    //
    // Unless we're told otherwise, don't quote unless we actually
    // need to do so --- hopefully avoid problems if programs won't
    // parse quotes properly
    //
    if !force && !argument.is_empty() && !argument.contains(|c: char| c.is_whitespace() || c == '"')
    {
        return argument.to_string();
    }

    let mut result = vec!['"'];

    let mut iterator = argument.chars().peekable();
    loop {
        let mut number_backslashes = 0usize;
        while let Some('\\') = iterator.peek() {
            iterator.next();
            number_backslashes += 1;
        }

        match iterator.next() {
            Some(char) => {
                if char == '"' {
                    //
                    // Escape all backslashes and the following
                    // double quotation mark.
                    //
                    append_copies(&mut result, '\\', number_backslashes * 2 + 1);
                    result.push(char);
                } else {
                    //
                    // Backslashes aren't special here.
                    //
                    append_copies(&mut result, '\\', number_backslashes);
                    result.push(char);
                }
            }
            None => {
                //
                // Escape all backslashes, but let the terminating
                // double quotation mark we add below be interpreted
                // as a metacharacter.
                //
                append_copies(&mut result, '\\', number_backslashes * 2);
                break;
            }
        }
    }

    result.push('"');

    result.into_iter().collect()
}

fn append_copies<T: Copy>(vector: &mut Vec<T>, value: T, count: usize) {
    vector.reserve(count);
    for _ in 0..count {
        vector.push(value);
    }
}
