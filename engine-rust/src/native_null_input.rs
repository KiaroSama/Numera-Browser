//! Identify NUL without reading an arbitrary character device synchronously.
#![cfg(windows)]
#![allow(unsafe_code)]
use std::io;
use windows_sys::{
    Wdk::Foundation::{NtQueryObject, OBJECT_NAME_INFORMATION},
    Win32::Foundation::HANDLE,
};

pub fn is_null(handle: HANDLE) -> io::Result<bool> {
    // Aligned bounded storage for OBJECT_NAME_INFORMATION and its UTF-16 name.
    let mut storage = [0usize; 512];
    let mut used = 0;
    let capacity = std::mem::size_of_val(&storage);
    // SAFETY: live borrowed character handle; writable aligned buffer of exact
    // capacity. ObjectNameInformation (1) queries metadata, never device input.
    let status = unsafe {
        NtQueryObject(
            handle,
            1,
            storage.as_mut_ptr().cast(),
            capacity as u32,
            &mut used,
        )
    };
    if status < 0
        || (used as usize) < std::mem::size_of::<OBJECT_NAME_INFORMATION>()
        || used as usize > capacity
    {
        return Err(io::Error::other(
            "Native character input identity could not be verified",
        ));
    }
    // SAFETY: successful query initialized the header in aligned storage.
    let name = unsafe { &*(storage.as_ptr().cast::<OBJECT_NAME_INFORMATION>()) }.Name;
    let start = storage.as_ptr() as usize;
    let pointer = name.Buffer as usize;
    let length = usize::from(name.Length);
    if !length.is_multiple_of(2)
        || !pointer.is_multiple_of(std::mem::align_of::<u16>())
        || pointer < start
        || pointer
            .checked_add(length)
            .is_none_or(|end| end > start + capacity)
    {
        return Err(io::Error::other(
            "Native character input identity is malformed",
        ));
    }
    // SAFETY: UTF-16 pointer/length are checked within this initialized query buffer.
    let text = unsafe { std::slice::from_raw_parts(name.Buffer, length / 2) };
    Ok(text == "\\Device\\Null".encode_utf16().collect::<Vec<_>>())
}
