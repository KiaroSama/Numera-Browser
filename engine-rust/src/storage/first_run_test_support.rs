//! Scoped replay of a directory order observed by a historical filesystem.
//! Entries still come from the real directory and all copy effects stay live.
use std::{cell::Cell, fs::DirEntry, io, path::Path};

type Order = fn(&Path, &mut [DirEntry]);
thread_local! {
    static ORDER: Cell<Option<Order>> = const { Cell::new(None) };
}
pub fn with_directory_order<R>(order: Order, operation: impl FnOnce() -> R) -> R {
    struct Restore(Option<Order>);
    impl Drop for Restore {
        fn drop(&mut self) {
            ORDER.with(|slot| slot.set(self.0));
        }
    }
    let _restore = Restore(ORDER.with(|slot| slot.replace(Some(order))));
    operation()
}
pub(super) fn ordered(path: &Path, mut entries: Vec<DirEntry>) -> io::Result<Vec<DirEntry>> {
    if let Some(order) = ORDER.with(Cell::get) {
        let mut before = entries.iter().map(DirEntry::path).collect::<Vec<_>>();
        order(path, &mut entries);
        let mut after = entries.iter().map(DirEntry::path).collect::<Vec<_>>();
        before.sort();
        after.sort();
        if before != after {
            return Err(io::Error::other(
                "Directory-order replay changed real entries",
            ));
        }
    }
    Ok(entries)
}

#[cfg(target_os = "macos")]
pub fn pipe_failure(source: &Path, destination: &Path) -> String {
    super::first_run::errors::pipe_failure(source, destination)
}
