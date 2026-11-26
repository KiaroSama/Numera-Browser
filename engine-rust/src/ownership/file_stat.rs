//! Kernel file identity for bounded compiler-cache admission. Descriptors and
//! path observations share the same representation; no file content is read.
use std::{
    fs::{self, File},
    io,
    path::Path,
};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Stat {
    pub device: u64,
    pub inode: u64,
    pub size: u64,
    pub modified_ns: i128,
    pub changed_ns: i128,
    pub birth_ns: Option<i128>,
    pub links: u64,
    pub regular: bool,
    pub directory: bool,
    pub reparse: bool,
}
#[cfg(unix)]
fn metadata(value: &fs::Metadata) -> Stat {
    use std::os::unix::fs::MetadataExt;
    Stat {
        device: value.dev(),
        inode: value.ino(),
        size: value.size(),
        modified_ns: value.mtime() as i128 * 1_000_000_000 + value.mtime_nsec() as i128,
        changed_ns: value.ctime() as i128 * 1_000_000_000 + value.ctime_nsec() as i128,
        birth_ns: None,
        links: value.nlink(),
        regular: value.is_file(),
        directory: value.is_dir(),
        reparse: value.file_type().is_symlink(),
    }
}
#[cfg(unix)]
pub fn path(path: &Path) -> io::Result<Stat> {
    Ok(metadata(&fs::symlink_metadata(path)?))
}
#[cfg(unix)]
pub fn file(file: &File) -> io::Result<Stat> {
    Ok(metadata(&file.metadata()?))
}
#[cfg(windows)]
pub fn path(path: &Path) -> io::Result<Stat> {
    use std::os::windows::fs::OpenOptionsExt;
    use windows_sys::Win32::Storage::FileSystem::{
        FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_READ_ATTRIBUTES,
        FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    };
    let file = fs::OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT | FILE_FLAG_BACKUP_SEMANTICS)
        .open(path)?;
    self::file(&file)
}
#[cfg(windows)]
#[allow(unsafe_code)]
pub fn file(file: &File) -> io::Result<Stat> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_BASIC_INFO, FileBasicInfo, GetFileInformationByHandle, GetFileInformationByHandleEx,
    };
    let handle = file.as_raw_handle();
    let mut value = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: the File retains its live handle for both bounded metadata queries,
    // and each output buffer has the exact documented Win32 layout and length.
    if unsafe { GetFileInformationByHandle(handle, &mut value) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let mut basic = FILE_BASIC_INFO::default();
    // SAFETY: same live handle; FileBasicInfo requires precisely FILE_BASIC_INFO.
    if unsafe {
        GetFileInformationByHandleEx(
            handle,
            FileBasicInfo,
            (&mut basic as *mut FILE_BASIC_INFO).cast(),
            std::mem::size_of::<FILE_BASIC_INFO>() as u32,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let stamp = |time: windows_sys::Win32::Foundation::FILETIME| -> i128 {
        ((u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)) as i128 * 100
    };
    let directory = value.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY != 0;
    let reparse = value.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0;
    Ok(Stat {
        device: value.dwVolumeSerialNumber.into(),
        inode: (u64::from(value.nFileIndexHigh) << 32) | u64::from(value.nFileIndexLow),
        size: (u64::from(value.nFileSizeHigh) << 32) | u64::from(value.nFileSizeLow),
        modified_ns: stamp(value.ftLastWriteTime),
        changed_ns: basic.ChangeTime as i128 * 100,
        birth_ns: Some(stamp(value.ftCreationTime)),
        links: value.nNumberOfLinks.into(),
        regular: !directory && !reparse,
        directory,
        reparse,
    })
}
#[cfg(not(any(unix, windows)))]
pub fn path(_: &Path) -> io::Result<Stat> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "File identity is unavailable",
    ))
}
#[cfg(not(any(unix, windows)))]
pub fn file(_: &File) -> io::Result<Stat> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "File identity is unavailable",
    ))
}
