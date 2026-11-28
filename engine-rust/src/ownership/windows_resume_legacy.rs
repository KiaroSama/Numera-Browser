// Literal previous Windows resume implementation from commit
// 0b9f60f989e3767d48f34be928b20509e9fc9032. This test-only negative control
// exercises the actual Win32 mechanism; production never invokes it.
use std::{io, mem};
use windows_sys::Win32::{
    Foundation::{CloseHandle, ERROR_NO_MORE_FILES, INVALID_HANDLE_VALUE},
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, TH32CS_SNAPTHREAD, THREADENTRY32, Thread32First, Thread32Next,
        },
        Threading::{OpenThread, ResumeThread, THREAD_SUSPEND_RESUME},
    },
};

pub fn resume(pid: u32) -> io::Result<()> {
    // SAFETY: read-only kernel thread snapshot, handle closed on all paths.
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPTHREAD, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return Err(io::Error::last_os_error());
    }
    let result = (|| {
        // SAFETY: required POD entry size set before enumeration.
        let mut entry: THREADENTRY32 = unsafe { mem::zeroed() };
        entry.dwSize = mem::size_of_val(&entry) as u32;
        let mut present = unsafe { Thread32First(snapshot, &mut entry) } != 0;
        let mut resumed = false;
        while present {
            if entry.th32OwnerProcessID == pid {
                // SAFETY: targeted thread from snapshot; no other access rights.
                let handle = unsafe { OpenThread(THREAD_SUSPEND_RESUME, 0, entry.th32ThreadID) };
                if handle.is_null() {
                    return Err(io::Error::last_os_error());
                }
                let count = unsafe { ResumeThread(handle) };
                let error = if count == u32::MAX {
                    Some(io::Error::last_os_error())
                } else {
                    None
                };
                unsafe { CloseHandle(handle) };
                if let Some(error) = error {
                    return Err(error);
                }
                resumed = true;
            }
            present = unsafe { Thread32Next(snapshot, &mut entry) } != 0;
        }
        let error = io::Error::last_os_error();
        if !resumed {
            return Err(io::Error::other("Suspended child has no resumable thread"));
        }
        if error.raw_os_error() != Some(ERROR_NO_MORE_FILES as i32) {
            return Err(error);
        }
        Ok(())
    })();
    unsafe { CloseHandle(snapshot) };
    result
}
