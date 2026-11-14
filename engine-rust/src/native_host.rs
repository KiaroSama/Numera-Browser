//! Durable claim host identity matches Python platform.node(), not shell variables.
use std::io;

#[cfg(windows)]
pub fn name() -> io::Result<String> {
    use windows_sys::Win32::{
        Foundation::ERROR_MORE_DATA,
        System::SystemInformation::{ComputerNamePhysicalDnsHostname, GetComputerNameExW},
    };
    let mut size = 256;
    let mut buffer = vec![0u16; size as usize];
    // SAFETY: initialized UTF-16 buffer sized exactly by the input count.
    #[allow(unsafe_code)]
    let mut ok = unsafe {
        GetComputerNameExW(
            ComputerNamePhysicalDnsHostname,
            buffer.as_mut_ptr(),
            &mut size,
        )
    };
    if ok == 0 {
        let error = io::Error::last_os_error();
        if error.raw_os_error() != Some(ERROR_MORE_DATA as i32) {
            return Err(error);
        }
        if size == 0 || size > 65536 {
            return Err(io::Error::other("OS hostname size is invalid"));
        }
        buffer.resize(size as usize, 0);
        // SAFETY: OS-required capacity is validated and retained through this call.
        #[allow(unsafe_code)]
        {
            ok = unsafe {
                GetComputerNameExW(
                    ComputerNamePhysicalDnsHostname,
                    buffer.as_mut_ptr(),
                    &mut size,
                )
            };
        }
        if ok == 0 {
            return Err(io::Error::last_os_error());
        }
    }
    let text = String::from_utf16(&buffer[..size as usize]).map_err(io::Error::other)?;
    validate(text)
}
#[cfg(unix)]
pub fn name() -> io::Result<String> {
    let mut data = std::mem::MaybeUninit::<libc::utsname>::uninit();
    // SAFETY: uname initializes the structure on success; no uninitialized read on failure.
    #[allow(unsafe_code)]
    if unsafe { libc::uname(data.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    #[allow(unsafe_code)]
    let data = unsafe { data.assume_init() };
    let bytes = data
        .nodename
        .iter()
        .take_while(|byte| **byte != 0)
        .map(|byte| *byte as u8)
        .collect::<Vec<_>>();
    validate(String::from_utf8(bytes).map_err(io::Error::other)?)
}
fn validate(text: String) -> io::Result<String> {
    if text.is_empty() {
        return Err(io::Error::other("OS hostname is empty"));
    }
    Ok(text)
}
