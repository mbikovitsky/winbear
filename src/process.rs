use std::{convert::TryInto, error::Error};

use bindings::Windows::Win32::System::{
    SystemServices::HANDLE,
    Threading::{
        CreateProcessW, TerminateProcess, DEBUG_PROCESS, PROCESS_CREATION_FLAGS, STARTUPINFOW,
    },
    WindowsProgramming::CloseHandle,
};

#[derive(Debug)]
pub struct Process {
    handle: HANDLE,
    process_id: u32,
}

impl Process {
    pub fn terminate(&self, exit_code: u32) -> windows::Result<()> {
        unsafe { TerminateProcess(self.handle, exit_code).ok() }
    }

    pub fn process_id(&self) -> u32 {
        self.process_id
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.handle).ok().unwrap();
        }
    }
}

#[derive(Debug, Clone)]
pub struct ProcessCreator {
    command_line: String,
    inherit_handles: bool,
    flags: PROCESS_CREATION_FLAGS,
    current_directory: Option<String>,
}

impl ProcessCreator {
    pub fn new_with_command_line(command_line: impl AsRef<str>) -> Self {
        Self::new(command_line.as_ref().to_string())
    }

    pub fn new_with_arguments<I, A>(arguments: I, force_quote: bool) -> Self
    where
        I: IntoIterator<Item = A>,
        A: AsRef<str>,
    {
        let arguments: Vec<_> = arguments
            .into_iter()
            .map(|argument| quote_argument(argument, force_quote))
            .collect();

        let command_line = arguments.join(" ");

        Self::new(command_line)
    }

    fn new(command_line: String) -> Self {
        Self {
            command_line,
            inherit_handles: false,
            flags: Default::default(),
            current_directory: None,
        }
    }

    pub fn inherit_handles(mut self, value: bool) -> ProcessCreator {
        self.inherit_handles = value;
        self
    }

    pub fn debug(mut self, value: bool) -> ProcessCreator {
        if value {
            self.flags |= DEBUG_PROCESS;
        } else {
            self.flags = PROCESS_CREATION_FLAGS(self.flags.0 & !DEBUG_PROCESS.0);
        }
        self
    }

    pub fn current_directory(mut self, value: Option<&str>) -> ProcessCreator {
        self.current_directory = value.and_then(|value| Some(value.to_string()));
        self
    }

    pub fn create(&self) -> Result<Process, Box<dyn Error>> {
        unsafe {
            let mut startup_info = STARTUPINFOW {
                cb: std::mem::size_of::<STARTUPINFOW>().try_into().unwrap(),
                ..Default::default()
            };

            let mut process_info = Default::default();

            let success = match &self.current_directory {
                Some(current_directory) => CreateProcessW(
                    None,
                    self.command_line.to_string(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    self.inherit_handles,
                    self.flags,
                    std::ptr::null_mut(),
                    current_directory.as_str(),
                    &mut startup_info,
                    &mut process_info,
                ),
                None => CreateProcessW(
                    None,
                    self.command_line.to_string(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    self.inherit_handles,
                    self.flags,
                    std::ptr::null_mut(),
                    None,
                    &mut startup_info,
                    &mut process_info,
                ),
            };
            success.ok()?;

            CloseHandle(process_info.hThread).ok().unwrap();

            Ok(Process {
                handle: process_info.hProcess,
                process_id: process_info.dwProcessId,
            })
        }
    }
}

fn quote_argument(argument: impl AsRef<str>, force: bool) -> String {
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
