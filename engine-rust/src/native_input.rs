//! A single raw stdin owner: no blocking reader thread can survive cancellation.
#[cfg(windows)]
use std::time::Duration;
use std::{
    io::{self, Read},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

pub struct Input<R> {
    stream: R,
    cancelled: Arc<AtomicBool>,
    #[cfg(windows)]
    console_ready: bool,
}
impl<R> Input<R> {
    pub fn new(stream: R, cancelled: Arc<AtomicBool>) -> Self {
        Self {
            stream,
            cancelled,
            #[cfg(windows)]
            console_ready: false,
        }
    }
    pub fn into_inner(self) -> R {
        self.stream
    }
}

#[cfg(windows)]
impl<R: std::os::windows::io::AsRawHandle + Read> Read for Input<R> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        use windows_sys::Win32::{
            Foundation::ERROR_BROKEN_PIPE,
            Storage::FileSystem::{
                FILE_TYPE_CHAR, FILE_TYPE_DISK, FILE_TYPE_PIPE, GetFileType, ReadFile,
            },
            System::{
                Console::{
                    GetConsoleMode, GetNumberOfConsoleInputEvents, INPUT_RECORD, KEY_EVENT,
                    PeekConsoleInputW, ReadConsoleInputW,
                },
                Pipes::PeekNamedPipe,
            },
        };
        if bytes.is_empty() {
            return Ok(0);
        }
        let handle = self.stream.as_raw_handle();
        // SAFETY: the retained stream owns this live handle for the entire read.
        #[allow(unsafe_code)]
        let kind = unsafe { GetFileType(handle) };
        let mut mode = 0;
        #[allow(unsafe_code)]
        let console = kind == FILE_TYPE_CHAR && unsafe { GetConsoleMode(handle, &mut mode) } != 0;
        if kind == FILE_TYPE_CHAR && !console && crate::native_null_input::is_null(handle)? {
            return Ok(0);
        }
        if ![FILE_TYPE_DISK, FILE_TYPE_PIPE].contains(&kind) && !console {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Native input handle is not a file, pipe or console",
            ));
        }
        loop {
            crate::job_spawn::reap_completed()?;
            if self.cancelled.load(Ordering::Acquire) {
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "Native input cancelled",
                ));
            }
            if console {
                if !self.console_ready {
                    let mut available = 0;
                    #[allow(unsafe_code)]
                    if unsafe { GetNumberOfConsoleInputEvents(handle, &mut available) } == 0 {
                        return Err(io::Error::last_os_error());
                    }
                    if available == 0 {
                        std::thread::park_timeout(Duration::from_millis(25));
                        continue;
                    }
                    if available > 65536 {
                        return Err(io::Error::other("Console input exceeds limit"));
                    }
                    let mut events = vec![INPUT_RECORD::default(); available as usize];
                    let mut found = 0;
                    // SAFETY: sole input owner peeks initialized records without consuming a key.
                    #[allow(unsafe_code)]
                    if unsafe {
                        PeekConsoleInputW(
                            handle,
                            events.as_mut_ptr(),
                            events.len() as u32,
                            &mut found,
                        )
                    } == 0
                    {
                        return Err(io::Error::last_os_error());
                    }
                    for event in &events[..found as usize] {
                        if u32::from(event.EventType) == KEY_EVENT {
                            // SAFETY: tag was checked before accessing the matching union arm.
                            #[allow(unsafe_code)]
                            let key = unsafe { event.Event.KeyEvent };
                            #[allow(unsafe_code)]
                            let character = unsafe { key.uChar.UnicodeChar };
                            if key.bKeyDown != 0 && [13, 26].contains(&character) {
                                self.console_ready = true;
                                break;
                            }
                        }
                    }
                    if !self.console_ready {
                        // Discard only non-key console events, so they cannot fill the peek window.
                        let leading = events[..found as usize]
                            .iter()
                            .take_while(|event| u32::from(event.EventType) != KEY_EVENT)
                            .count();
                        if leading > 0 {
                            let mut consumed = 0;
                            #[allow(unsafe_code)]
                            if unsafe {
                                ReadConsoleInputW(
                                    handle,
                                    events.as_mut_ptr(),
                                    leading as u32,
                                    &mut consumed,
                                )
                            } == 0
                            {
                                return Err(io::Error::last_os_error());
                            }
                        }
                        std::thread::park_timeout(Duration::from_millis(25));
                        continue;
                    }
                }
                // The stdlib performs the console UTF-16/UTF-8 conversion and buffering.
                let count = self.stream.read(&mut bytes[..1])?;
                if count == 0 || bytes[0] == b'\n' {
                    self.console_ready = false;
                }
                return Ok(count);
            }
            let mut count = bytes.len().min(u32::MAX as usize) as u32;
            if kind == FILE_TYPE_PIPE {
                let mut available = 0;
                // SAFETY: exactly one owner reads this synchronous pipe; no overlapping I/O.
                #[allow(unsafe_code)]
                let ok = unsafe {
                    PeekNamedPipe(
                        handle,
                        std::ptr::null_mut(),
                        0,
                        std::ptr::null_mut(),
                        &mut available,
                        std::ptr::null_mut(),
                    )
                };
                if ok == 0 {
                    let error = io::Error::last_os_error();
                    return if error.raw_os_error() == Some(ERROR_BROKEN_PIPE as i32) {
                        Ok(0)
                    } else {
                        Err(error)
                    };
                }
                if available == 0 {
                    std::thread::park_timeout(Duration::from_millis(25));
                    continue;
                }
                count = count.min(available);
            }
            let mut read = 0;
            // SAFETY: buffer is writable for count bytes, stream handle is retained;
            // pipe reads never request more than this sole owner just observed.
            #[allow(unsafe_code)]
            let ok = unsafe {
                ReadFile(
                    handle,
                    bytes.as_mut_ptr(),
                    count,
                    &mut read,
                    std::ptr::null_mut(),
                )
            };
            if ok != 0 {
                return Ok(read as usize);
            }
            let error = io::Error::last_os_error();
            return if error.raw_os_error() == Some(ERROR_BROKEN_PIPE as i32) {
                Ok(0)
            } else {
                Err(error)
            };
        }
    }
}
#[cfg(unix)]
impl<R: std::os::fd::AsRawFd> Read for Input<R> {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        if bytes.is_empty() {
            return Ok(0);
        }
        let fd = self.stream.as_raw_fd();
        loop {
            crate::job_spawn::reap_completed()?;
            if self.cancelled.load(Ordering::Acquire) {
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    "Native input cancelled",
                ));
            }
            let mut descriptor = libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            };
            // SAFETY: one initialized pollfd and a finite timeout; stream retains fd.
            #[allow(unsafe_code)]
            let ready = unsafe { libc::poll(&mut descriptor, 1, 25) };
            if ready < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error);
            }
            if ready == 0 {
                continue;
            }
            if descriptor.revents & libc::POLLNVAL != 0 {
                return Err(io::Error::other("Native input descriptor is invalid"));
            }
            // SAFETY: this is the only reader of the retained descriptor, and poll
            // confirmed readiness/EOF. The provided buffer is writable for its length.
            #[allow(unsafe_code)]
            let count = unsafe { libc::read(fd, bytes.as_mut_ptr().cast(), bytes.len()) };
            if count >= 0 {
                return Ok(count as usize);
            }
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        }
    }
}
