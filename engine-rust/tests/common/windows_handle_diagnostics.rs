//! Passive, own-process diagnostics for existing resource assertions.
//! No object names, handle values, paths or kernel pointers leave this module.
//! The table snapshot and later duplicate/type queries are non-atomic. A handle
//! may close or be reused between them; status0 means the bounded procedure
//! completed, and supplies no atomic object identity or lifetime authority.
use std::{
    fmt::Write as _,
    time::{Duration, Instant},
};

const TYPES: [&str; 28] = [
    "File",
    "Thread",
    "Process",
    "Event",
    "Job",
    "IoCompletion",
    "Mutant",
    "Section",
    "Semaphore",
    "Timer",
    "Key",
    "Token",
    "Directory",
    "SymbolicLink",
    "ALPC Port",
    "WaitCompletionPacket",
    "TpWorkerFactory",
    "IRTimer",
    "Desktop",
    "WindowStation",
    "EtwRegistration",
    "EtwConsumer",
    "Partition",
    "Session",
    "DebugObject",
    "Device",
    "Driver",
    "Other",
];
const OTHER: usize = TYPES.len() - 1;
pub const LEGEND: &str = "type_order=File,Thread,Process,Event,Job,IoCompletion,Mutant,Section,Semaphore,Timer,Key,Token,Directory,SymbolicLink,ALPC_Port,WaitCompletionPacket,TpWorkerFactory,IRTimer,Desktop,WindowStation,EtwRegistration,EtwConsumer,Partition,Session,DebugObject,Device,Driver,Other files=Pipe,Disk,Char,Unknown standard=stdin,stdout,stderr status=complete,deadline,unsupported,symbols,query,bounds,type_cap,unresolved_type,duplicate sampling=non_atomic complete=bounded_query_procedure_only";

// Status legend is fixed, and is repeated by the parent fixture's printer:
// 0 complete, 1 deadline, 2 unsupported, 3 unavailable symbols, 4 native query
// failed, 5 invalid/truncated snapshot, 6 type cap, 7 unresolved type, 8 duplicate.
struct Snapshot {
    status: u32,
    total: u32,
    types: [u32; TYPES.len()],
    files: [u32; 4],    // Pipe, Disk, Char, Unknown
    standard: [u32; 3], // stdin, stdout, stderr (borrowed Win32 GetStdHandle)
    duplicates_failed: u32,
    queries_failed: u32,
}
impl Snapshot {
    fn new(status: u32) -> Self {
        Self {
            status,
            total: 0,
            types: [0; TYPES.len()],
            files: [0; 4],
            standard: [0; 3],
            duplicates_failed: 0,
            queries_failed: 0,
        }
    }
    fn partial(&mut self, status: u32) {
        if status == 1 || self.status == 0 {
            self.status = status;
        }
    }
    fn write(&self, text: &mut String) {
        write!(text, "{{\"status\":{},\"total\":{},\"types\":{:?},\"files\":{:?},\"standard\":{:?},\"duplicates_failed\":{},\"queries_failed\":{}}}",
            self.status, self.total, self.types, self.files, self.standard,
            self.duplicates_failed, self.queries_failed).expect("String formatting");
    }
}
fn type_index(name: &str) -> usize {
    TYPES
        .iter()
        .position(|known| *known == name)
        .unwrap_or(OTHER)
}
fn contained_utf16(base: usize, used: usize, pointer: usize, length: u16, maximum: u16) -> bool {
    let length = usize::from(length);
    length.is_multiple_of(2)
        && length <= usize::from(maximum)
        && length <= 512
        && pointer.is_multiple_of(2)
        && pointer >= base
        && pointer
            .checked_add(length)
            .zip(base.checked_add(used))
            .is_some_and(|(end, limit)| end <= limit)
}
fn snapshot(deadline: Instant) -> Snapshot {
    let now = Instant::now();
    if now >= deadline {
        return Snapshot::new(1);
    }
    #[cfg(windows)]
    {
        native::snapshot(deadline.min(now + Duration::from_millis(100)))
    }
    #[cfg(not(windows))]
    {
        let _ = Duration::from_millis(100);
        Snapshot::new(2)
    }
}

pub struct Attribution {
    before: Snapshot,
    deadline: Instant,
}
impl Attribution {
    /// Run before the caller's unchanged descriptor/ResourceGuard baseline.
    pub fn before(deadline: Instant) -> Self {
        Self {
            before: snapshot(deadline),
            deadline,
        }
    }
    /// The caller first records its original predicate and elapsed time.
    /// Successful assertions do not query handles for a second time.
    pub fn on_failure(&self, failed: bool) -> Option<String> {
        if !failed {
            return None;
        }
        let after = snapshot(self.deadline);
        let delta: Vec<i64> = after
            .types
            .iter()
            .zip(self.before.types)
            .map(|(new, old)| i64::from(*new) - i64::from(old))
            .collect();
        let mut text = String::from("{\"schema\":1,\"before\":");
        self.before.write(&mut text);
        text.push_str(",\"after\":");
        after.write(&mut text);
        write!(text, ",\"type_delta\":{delta:?}}}").expect("String formatting");
        Some(text)
    }
}

/// Folded into existing fixture cases; no new reported test identity.
pub fn portable_controls() -> String {
    assert!(LEGEND.starts_with("type_order=File,Thread,"));
    assert_eq!(type_index("File"), 0);
    assert_eq!(type_index("private-name-must-never-be-serialized"), OTHER);
    assert!(contained_utf16(100, 32, 116, 16, 16));
    assert!(!contained_utf16(100, 32, 116, 17, 18));
    assert!(!contained_utf16(100, 32, 98, 16, 16));
    assert!(!contained_utf16(100, 32, 116, 18, 18));
    assert!(!contained_utf16(usize::MAX - 8, 32, usize::MAX - 4, 8, 8));
    let mut sample = Snapshot::new(7);
    sample.types[type_index("private-name-must-never-be-serialized")] = 1;
    sample.partial(8);
    let mut text = String::new();
    sample.write(&mut text);
    assert!(!text.contains("private-name"));
    assert!(text.starts_with("{\"status\":7,"));
    let expired = Attribution::before(Instant::now());
    let report = expired.on_failure(true).expect("failure attribution");
    assert!(report.contains("\"type_delta\":[0, 0"));
    assert!(
        Attribution::before(Instant::now())
            .on_failure(false)
            .is_none()
    );
    report
}

#[cfg(windows)]
mod native {
    use super::*;
    use std::{
        collections::BTreeMap,
        ffi::c_void,
        mem,
        os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
        ptr,
    };
    type Handle = *mut c_void;
    type Query = unsafe extern "system" fn(Handle, u32, *mut c_void, u32, *mut u32) -> i32;
    type Symbol = unsafe extern "system" fn() -> isize;
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> Handle;
        fn GetStdHandle(which: u32) -> Handle;
        fn GetModuleHandleW(name: *const u16) -> Handle;
        fn GetProcAddress(module: Handle, name: *const u8) -> Option<Symbol>;
        fn DuplicateHandle(
            source: Handle,
            handle: Handle,
            target: Handle,
            duplicate: *mut Handle,
            access: u32,
            inherit: i32,
            options: u32,
        ) -> i32;
        fn GetFileType(handle: Handle) -> u32;
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Entry {
        handle: usize,
        _handle_count: usize,
        _pointer_count: usize,
        _granted_access: u32,
        object_type: u32,
        _attributes: u32,
        _reserved: u32,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    struct Unicode {
        length: u16,
        maximum: u16,
        buffer: *const u16,
    }

    fn late(result: &mut Snapshot, deadline: Instant) -> bool {
        if Instant::now() >= deadline {
            result.partial(1);
            true
        } else {
            false
        }
    }
    fn duplicate(process: Handle, raw: usize) -> Option<OwnedHandle> {
        let mut handle = ptr::null_mut();
        // SAFETY: both process arguments are the current-process pseudo-handle;
        // source comes only from its own snapshot. SAME_ACCESS, not CLOSE_SOURCE;
        // this initialized output receives one noninheritable owned duplicate.
        if unsafe { DuplicateHandle(process, raw as Handle, process, &mut handle, 0, 0, 2) } == 0
            || handle.is_null()
            || handle as usize == usize::MAX
        {
            None
        } else {
            // SAFETY: successful DuplicateHandle transferred one CloseHandle
            // owner. Every return/unwind path drops it; never close the source.
            Some(unsafe { OwnedHandle::from_raw_handle(handle) })
        }
    }
    fn object_type(query: Query, handle: &OwnedHandle) -> Option<usize> {
        // usize storage supplies the native alignment; read only the documented
        // leading UNICODE_STRING after validating the returned buffer length.
        let mut bytes = [0usize; 512];
        let mut used = 0u32;
        // SAFETY: exact NtQueryObject ABI, owned duplicate and initialized,
        // aligned bounded output. Class2 requests TYPE, never object NAME.
        let status = unsafe {
            query(
                handle.as_raw_handle(),
                2,
                bytes.as_mut_ptr().cast(),
                mem::size_of_val(&bytes) as u32,
                &mut used,
            )
        };
        if status < 0
            || used as usize > mem::size_of_val(&bytes)
            || (used as usize) < mem::size_of::<Unicode>()
        {
            return None;
        }
        // SAFETY: the successful bounded result contains this entire header.
        let name = unsafe { ptr::read_unaligned(bytes.as_ptr().cast::<Unicode>()) };
        if !contained_utf16(
            bytes.as_ptr() as usize,
            used as usize,
            name.buffer as usize,
            name.length,
            name.maximum,
        ) {
            return None;
        }
        // SAFETY: checked alignment, even byte length and complete containment
        // in this still-live output buffer. No untrusted pointer escapes.
        let wide = unsafe { std::slice::from_raw_parts(name.buffer, usize::from(name.length) / 2) };
        String::from_utf16(wide).ok().map(|text| type_index(&text))
    }
    pub(super) fn snapshot(deadline: Instant) -> Snapshot {
        let mut result = Snapshot::new(0);
        if late(&mut result, deadline) {
            return result;
        }
        let ntdll = [110u16, 116, 100, 108, 108, 46, 100, 108, 108, 0];
        // SAFETY: fixed terminated module name; borrow already-loaded ntdll.
        let module = unsafe { GetModuleHandleW(ntdll.as_ptr()) };
        if late(&mut result, deadline) {
            return result;
        }
        if module.is_null() {
            result.partial(3);
            return result;
        }
        // SAFETY: borrowed loaded module and fixed terminated export names.
        let process_query =
            unsafe { GetProcAddress(module, c"NtQueryInformationProcess".as_ptr().cast()) };
        if late(&mut result, deadline) {
            return result;
        }
        // SAFETY: same loaded-module contract; no LoadLibrary or global dump.
        let object_query = unsafe { GetProcAddress(module, c"NtQueryObject".as_ptr().cast()) };
        if late(&mut result, deadline) {
            return result;
        }
        let (Some(process_query), Some(object_query)) = (process_query, object_query) else {
            result.partial(3);
            return result;
        };
        // SAFETY: both named exports have exactly this documented five-argument
        // NTAPI query ABI; no caller-supplied symbol participates.
        let process_query = unsafe { mem::transmute::<Symbol, Query>(process_query) };
        // SAFETY: NtQueryObject has the same scalar/output ABI as Query.
        let object_query = unsafe { mem::transmute::<Symbol, Query>(object_query) };
        // SAFETY: borrowed pseudo-handle; never close it.
        let process = unsafe { GetCurrentProcess() };
        if late(&mut result, deadline) {
            return result;
        }
        let mut standard = [0usize; 3];
        for (slot, which) in
            standard
                .iter_mut()
                .zip([(-10i32) as u32, (-11i32) as u32, (-12i32) as u32])
        {
            if late(&mut result, deadline) {
                return result;
            }
            // SAFETY: fixed Win32 standard handle selectors. This does not
            // initialize Rust's stdout/stderr or own/close the returned handle.
            *slot = unsafe { GetStdHandle(which) } as usize;
            if late(&mut result, deadline) {
                return result;
            }
        }
        let mut entries = Vec::new();
        let mut observed = false;
        let mut capacity = 16 * 1024usize;
        for _ in 0..3 {
            if late(&mut result, deadline) {
                return result;
            }
            let mut storage = vec![0usize; capacity.div_ceil(mem::size_of::<usize>())];
            let mut used = 0u32;
            // SAFETY: current process only, class51 since Windows8; aligned,
            // initialized bounded storage and a distinct initialized length.
            let status = unsafe {
                process_query(
                    process,
                    51,
                    storage.as_mut_ptr().cast(),
                    capacity as u32,
                    &mut used,
                )
            };
            if late(&mut result, deadline) {
                return result;
            }
            if status < 0 {
                if matches!(status as u32, 0xc0000004 | 0xc0000023 | 0x80000005) {
                    capacity = (used as usize).max(capacity.saturating_mul(2));
                    if capacity <= 1024 * 1024 {
                        continue;
                    }
                }
                result.partial(4);
                return result;
            }
            let header = 2 * mem::size_of::<usize>();
            if (used as usize) < header || used as usize > capacity {
                result.partial(5);
                return result;
            }
            // SAFETY: the complete two-word header was checked above.
            let count = unsafe { ptr::read_unaligned(storage.as_ptr()) };
            let required = count
                .checked_mul(mem::size_of::<Entry>())
                .and_then(|n| n.checked_add(header));
            if count > 4096 || required.is_none_or(|n| n > used as usize) {
                result.partial(5);
                return result;
            }
            for index in 0..count {
                // SAFETY: checked total count, multiplication and returned byte
                // extent above; copy records, never follow numeric handle values.
                entries.push(unsafe {
                    ptr::read_unaligned(
                        storage
                            .as_ptr()
                            .cast::<u8>()
                            .add(header + index * mem::size_of::<Entry>())
                            .cast::<Entry>(),
                    )
                });
            }
            observed = true;
            break;
        }
        if !observed {
            result.partial(5);
            return result;
        }
        result.total = entries.len() as u32;
        let mut types = BTreeMap::new();
        for entry in &entries {
            if types.contains_key(&entry.object_type) {
                continue;
            }
            if types.len() >= 64 {
                result.partial(6);
                break;
            }
            let mut resolved = None;
            for representative in entries
                .iter()
                .filter(|e| e.object_type == entry.object_type)
                .take(2)
            {
                if late(&mut result, deadline) {
                    break;
                }
                let Some(duplicate) = duplicate(process, representative.handle) else {
                    result.duplicates_failed += 1;
                    result.partial(8);
                    continue;
                };
                if late(&mut result, deadline) {
                    break;
                }
                resolved = object_type(object_query, &duplicate);
                if late(&mut result, deadline) {
                    break;
                }
                if resolved.is_some() {
                    break;
                }
                result.queries_failed += 1;
            }
            if resolved.is_none() {
                result.partial(7);
            }
            types.insert(entry.object_type, resolved.unwrap_or(OTHER));
        }
        for entry in &entries {
            let kind = types.get(&entry.object_type).copied().unwrap_or(OTHER);
            result.types[kind] += 1;
            for (index, handle) in standard.iter().enumerate() {
                if *handle != 0 && *handle != usize::MAX && *handle == entry.handle {
                    result.standard[index] += 1;
                }
            }
        }
        for entry in &entries {
            if types.get(&entry.object_type) != Some(&0) {
                continue;
            }
            if late(&mut result, deadline) {
                break;
            }
            let Some(handle) = duplicate(process, entry.handle) else {
                result.duplicates_failed += 1;
                result.partial(8);
                continue;
            };
            if late(&mut result, deadline) {
                break;
            }
            // SAFETY: own live duplicate; queries a static file type, never a
            // file name, content, pipe read or a device control operation.
            let file_type = unsafe { GetFileType(handle.as_raw_handle()) };
            if late(&mut result, deadline) {
                break;
            }
            result.files[match file_type {
                3 => 0,
                1 => 1,
                2 => 2,
                _ => 3,
            }] += 1;
        }
        result
    }
}
