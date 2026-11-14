//! Bounded terminal-only password input; echo is restored on every return path.
#[cfg(unix)]
use crate::native_input::Input;
#[cfg(unix)]
use std::io::{BufRead, Read};
use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    sync::{Arc, atomic::AtomicBool},
};

pub fn read(prompt: &str, cancelled: Arc<AtomicBool>) -> io::Result<String> {
    let terminal = OpenOptions::new()
        .read(true)
        .write(true)
        .open(if cfg!(windows) { "CONIN$" } else { "/dev/tty" })?;
    let mut output = if cfg!(windows) {
        OpenOptions::new().write(true).open("CONOUT$")?
    } else {
        terminal.try_clone()?
    };
    let echo = Echo::disable(&terminal)?;
    output.write_all(prompt.as_bytes())?;
    output.flush()?;
    #[cfg(windows)]
    let result = read_console(&terminal, cancelled);
    #[cfg(unix)]
    let result = (|| {
        let mut input = io::BufReader::with_capacity(1, Input::new(terminal, cancelled));
        let mut value = String::new();
        let count = input.by_ref().take(65537).read_line(&mut value)?;
        if count == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "Master password input ended",
            ));
        }
        if count > 65536 {
            return Err(io::Error::other("Master password input exceeds limit"));
        }
        while value.ends_with(['\r', '\n']) {
            value.pop();
        }
        Ok(value)
    })();
    // Restore while the input owner still retains the terminal handle.
    echo.restore()?;
    output.write_all(if cfg!(windows) { b"\r\n" } else { b"\n" })?;
    output.flush()?;
    result
}

pub fn prepare(
    args: &serde_json::Map<String, serde_json::Value>,
    cancelled: Arc<AtomicBool>,
) -> io::Result<serde_json::Map<String, serde_json::Value>> {
    let mut args = args.clone();
    if args.get("command").and_then(serde_json::Value::as_str) != Some("vault")
        || args.get("json") == Some(&serde_json::Value::Bool(true))
    {
        return Ok(args);
    }
    let verb = args
        .get("vault_command")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("")
        .to_owned();
    let present = |args: &serde_json::Map<String, serde_json::Value>, key: &str| {
        args.get(key)
            .and_then(serde_json::Value::as_str)
            .is_some_and(|value| !value.is_empty())
    };
    if args.get("detach") == Some(&serde_json::Value::Bool(true)) && verb != "resume" {
        return Ok(args);
    }
    if matches!(verb.as_str(), "enable" | "change" | "resume")
        && !present(&args, "password")
        && (verb == "resume"
            || !std::env::var("NUMERA_PASSWORD").is_ok_and(|value| !value.is_empty()))
    {
        let password = read("Master password: ", cancelled.clone())?;
        if verb == "enable" && read("Confirm master password: ", cancelled.clone())? != password {
            return Err(io::Error::other("Passwords didn't match."));
        }
        args.insert("password".into(), password.into());
    }
    if matches!(verb.as_str(), "change" | "resume") && !present(&args, "new_password") {
        let password = read(
            if verb == "change" {
                "New master password: "
            } else {
                "The password the rotation was moving TO: "
            },
            cancelled.clone(),
        )?;
        if verb == "change" && read("Confirm new master password: ", cancelled)? != password {
            return Err(io::Error::other("Passwords didn't match."));
        }
        args.insert("new_password".into(), password.into());
    }
    Ok(args)
}

#[cfg(windows)]
struct Echo {
    terminal: File,
    mode: u32,
    restored: std::cell::Cell<bool>,
}
#[cfg(windows)]
impl Echo {
    #[allow(unsafe_code)]
    fn disable(file: &File) -> io::Result<Self> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::Console::{
            ENABLE_ECHO_INPUT, ENABLE_LINE_INPUT, ENABLE_PROCESSED_INPUT, GetConsoleMode,
            SetConsoleMode,
        };
        let terminal = file.try_clone()?;
        let handle = terminal.as_raw_handle();
        let mut mode = 0;
        // SAFETY: borrowed live terminal handle, initialized output and scalar mode.
        if unsafe { GetConsoleMode(handle, &mut mode) } == 0
            || unsafe {
                SetConsoleMode(
                    handle,
                    mode & !(ENABLE_ECHO_INPUT | ENABLE_LINE_INPUT | ENABLE_PROCESSED_INPUT),
                )
            } == 0
        {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            terminal,
            mode,
            restored: std::cell::Cell::new(false),
        })
    }
    #[allow(unsafe_code)]
    fn restore(&self) -> io::Result<()> {
        use std::os::windows::io::AsRawHandle;
        // SAFETY: guard retains its own terminal handle through restoration.
        if !self.restored.get()
            && unsafe {
                windows_sys::Win32::System::Console::SetConsoleMode(
                    self.terminal.as_raw_handle(),
                    self.mode,
                )
            } == 0
        {
            return Err(io::Error::last_os_error());
        }
        self.restored.set(true);
        Ok(())
    }
}
#[cfg(unix)]
struct Echo {
    terminal: File,
    mode: libc::termios,
    restored: std::cell::Cell<bool>,
}
#[cfg(unix)]
impl Echo {
    #[allow(unsafe_code)]
    fn disable(file: &File) -> io::Result<Self> {
        use std::os::fd::AsRawFd;
        let terminal = file.try_clone()?;
        let fd = terminal.as_raw_fd();
        // SAFETY: termios is a POD output initialized by tcgetattr on a live tty.
        let mut mode: libc::termios = unsafe { std::mem::zeroed() };
        if unsafe { libc::tcgetattr(fd, &mut mode) } != 0 {
            return Err(io::Error::last_os_error());
        }
        let mut hidden = mode;
        hidden.c_lflag &= !libc::ECHO;
        if unsafe { libc::tcsetattr(fd, libc::TCSANOW, &hidden) } != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(Self {
            terminal,
            mode,
            restored: std::cell::Cell::new(false),
        })
    }
    #[allow(unsafe_code)]
    fn restore(&self) -> io::Result<()> {
        use std::os::fd::AsRawFd;
        // SAFETY: guard retains its own tty descriptor and original termios.
        if !self.restored.get()
            && unsafe { libc::tcsetattr(self.terminal.as_raw_fd(), libc::TCSANOW, &self.mode) } != 0
        {
            return Err(io::Error::last_os_error());
        }
        self.restored.set(true);
        Ok(())
    }
}
impl Drop for Echo {
    fn drop(&mut self) {
        if self.restore().is_err() {
            eprintln!("Terminal echo could not be restored");
        }
    }
}
#[cfg(windows)]
#[allow(unsafe_code)]
fn read_console(terminal: &File, cancelled: Arc<AtomicBool>) -> io::Result<String> {
    use std::{os::windows::io::AsRawHandle, sync::atomic::Ordering, time::Duration};
    use windows_sys::Win32::System::Console::{
        GetNumberOfConsoleInputEvents, INPUT_RECORD, KEY_EVENT, ReadConsoleInputW,
    };
    let handle = terminal.as_raw_handle();
    let mut units = Vec::new();
    loop {
        if cancelled.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "Native input cancelled",
            ));
        }
        let mut available = 0;
        // SAFETY: the sole input owner retains the console handle; poll is nonblocking.
        if unsafe { GetNumberOfConsoleInputEvents(handle, &mut available) } == 0 {
            return Err(io::Error::last_os_error());
        }
        if available == 0 {
            std::thread::park_timeout(Duration::from_millis(25));
            continue;
        }
        let mut event = INPUT_RECORD::default();
        let mut count = 0;
        // SAFETY: one initialized writable record; this owner consumes only observed events.
        if unsafe { ReadConsoleInputW(handle, &mut event, 1, &mut count) } == 0 {
            return Err(io::Error::last_os_error());
        }
        if count == 0 || u32::from(event.EventType) != KEY_EVENT {
            continue;
        }
        // SAFETY: the event tag selects this union arm and its Unicode member.
        let key = unsafe { event.Event.KeyEvent };
        let character = unsafe { key.uChar.UnicodeChar };
        if key.bKeyDown == 0 || character == 0 {
            continue;
        }
        match character {
            3 => {
                cancelled.store(true, Ordering::Release);
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "Native input cancelled",
                ));
            }
            13 | 10 => {
                let value = String::from_utf16(&units)
                    .map_err(|_| io::Error::other("Master password input is not valid Unicode"))?;
                if value.len() > 65536 {
                    return Err(io::Error::other("Master password input exceeds limit"));
                }
                return Ok(value);
            }
            8 => {
                for _ in 0..key.wRepeatCount.max(1) {
                    if units
                        .pop()
                        .is_some_and(|unit| (0xdc00..=0xdfff).contains(&unit))
                        && units
                            .last()
                            .is_some_and(|unit| (0xd800..=0xdbff).contains(unit))
                    {
                        units.pop();
                    }
                }
            }
            _ => {
                let repeats = usize::from(key.wRepeatCount.max(1));
                if units.len() + repeats > 65536 {
                    return Err(io::Error::other("Master password input exceeds limit"));
                }
                units.extend(std::iter::repeat_n(character, repeats));
            }
        }
    }
}
