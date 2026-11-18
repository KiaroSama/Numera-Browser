#[cfg(unix)]
use std::os::fd::AsRawFd;
#[cfg(windows)]
use std::os::windows::io::AsRawHandle;
use std::{
    io::{self, Read},
    process::{ChildStderr, ChildStdout},
};

pub enum Pipe {
    Out(ChildStdout),
    Err(ChildStderr),
}
impl Read for Pipe {
    fn read(&mut self, bytes: &mut [u8]) -> io::Result<usize> {
        match self {
            Self::Out(pipe) => pipe.read(bytes),
            Self::Err(pipe) => pipe.read(bytes),
        }
    }
}
#[cfg(unix)]
impl AsRawFd for Pipe {
    fn as_raw_fd(&self) -> std::os::fd::RawFd {
        match self {
            Self::Out(pipe) => pipe.as_raw_fd(),
            Self::Err(pipe) => pipe.as_raw_fd(),
        }
    }
}
impl Pipe {
    #[cfg(unix)]
    #[allow(unsafe_code)]
    pub fn prepare(&self) -> io::Result<()> {
        let fd = self.as_raw_fd();
        // SAFETY: uniquely owned open child-output descriptor, query flags then
        // add nonblocking without discarding its existing file status flags.
        let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
        if flags < 0 || unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    #[cfg(unix)]
    pub(crate) fn wait_readable(&self, timeout: std::time::Duration) -> io::Result<()> {
        let mut descriptor = libc::pollfd {
            fd: self.as_raw_fd(),
            events: libc::POLLIN,
            revents: 0,
        };
        let millis = timeout.as_nanos().div_ceil(1_000_000);
        let millis = i32::try_from(millis).unwrap_or(i32::MAX);
        // SAFETY: the pipe retains the descriptor and poll borrows one initialized entry.
        #[allow(unsafe_code)]
        let ready = unsafe { libc::poll(&mut descriptor, 1, millis) };
        if ready < 0 {
            let error = io::Error::last_os_error();
            if error.kind() != io::ErrorKind::Interrupted {
                return Err(error);
            }
        } else if descriptor.revents & libc::POLLNVAL != 0 {
            return Err(io::Error::other("Child output descriptor is invalid"));
        }
        Ok(())
    }
    #[cfg(windows)]
    pub fn prepare(&self) -> io::Result<()> {
        Ok(())
    }
    #[cfg(not(any(unix, windows)))]
    pub fn prepare(&self) -> io::Result<()> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Bounded child output is unsupported on this platform",
        ))
    }

    pub fn available(&mut self, bytes: &mut [u8]) -> io::Result<Option<usize>> {
        #[cfg(windows)]
        {
            use windows_sys::Win32::{Foundation::ERROR_BROKEN_PIPE, System::Pipes::PeekNamedPipe};
            let handle = match self {
                Self::Out(pipe) => pipe.as_raw_handle(),
                Self::Err(pipe) => pipe.as_raw_handle(),
            };
            let mut available = 0;
            // SAFETY: the single capture owner retains this pipe handle and no
            // other read/write is in progress. No data consumed by the peek.
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
                    Ok(Some(0))
                } else {
                    Err(error)
                };
            }
            if available == 0 {
                return Ok(None);
            }
            let count = bytes.len().min(available as usize);
            self.read(&mut bytes[..count]).map(Some)
        }
        #[cfg(not(windows))]
        match self.read(bytes) {
            Ok(count) => Ok(Some(count)),
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted
                ) =>
            {
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }
}
