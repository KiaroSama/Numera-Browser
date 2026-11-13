//! Explicit fault observers for native integration tests. This module is absent
//! unless the non-product `test-support` Cargo feature is enabled.
use std::{
    cell::RefCell,
    io,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};
#[derive(Clone, Debug)]
pub struct Event {
    pub operation: &'static str,
    pub path: PathBuf,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Continue,
    Skip,
}
type PathResolver = Arc<Mutex<Box<dyn FnMut(&Path) -> io::Result<Option<PathBuf>> + Send>>>;
type Observer = Arc<Mutex<Box<dyn FnMut(&Event) -> io::Result<Action> + Send>>>;
thread_local! {
 static OBSERVER: RefCell<Option<Observer>> = const { RefCell::new(None) };
 static PATH_RESOLVER: RefCell<Option<PathResolver>> = const { RefCell::new(None) };
}
/// An explicitly captured test observer; safe to move across a real spawn
/// boundary. Production builds have no observer state or capture calls.
#[derive(Clone)]
pub struct Context(Option<Observer>, Option<PathResolver>);
struct Restore(Option<Observer>, Option<PathResolver>);
impl Drop for Restore {
    fn drop(&mut self) {
        OBSERVER.with(|slot| {
            *slot.borrow_mut() = self.0.take();
        });
        PATH_RESOLVER.with(|slot| {
            *slot.borrow_mut() = self.1.take();
        });
    }
}
pub fn capture() -> Context {
    Context(
        OBSERVER.with(|slot| slot.borrow().clone()),
        PATH_RESOLVER.with(|slot| slot.borrow().clone()),
    )
}
pub fn with_context<R>(context: Context, operation: impl FnOnce() -> R) -> R {
    let previous = OBSERVER.with(|slot| slot.replace(context.0));
    let resolver = PATH_RESOLVER.with(|slot| slot.replace(context.1));
    let _restore = Restore(previous, resolver);
    operation()
}
/// Install an observer for one operation; unwinding restores the prior scope.
/// Callback state must be Send so real worker-thread boundaries can explicitly
/// carry this context. Independent test scopes retain independent observers.
pub fn with_observer<R>(
    observer: impl FnMut(&Event) -> io::Result<Action> + Send + 'static,
    operation: impl FnOnce() -> R,
) -> R {
    with_context(
        Context(
            Some(Arc::new(Mutex::new(Box::new(observer)))),
            PATH_RESOLVER.with(|slot| slot.borrow().clone()),
        ),
        operation,
    )
}
pub(crate) fn event(operation: &'static str, path: &Path) -> io::Result<Action> {
    let observer = OBSERVER.with(|slot| slot.borrow().clone());
    // A callback may perform another real operation (for example, the edit
    // that races a pending delete). Suppress only this observer while it runs
    // so nested I/O cannot re-lock the callback's mutex or fault recursively.
    let resolver = PATH_RESOLVER.with(|slot| slot.borrow().clone());
    with_context(Context(None, resolver), || match observer {
        Some(observer) => {
            observer
                .lock()
                .map_err(|_| io::Error::other("Test observer lock poisoned"))?(&Event {
                operation,
                path: path.to_owned(),
            })
        }
        None => Ok(Action::Continue),
    })
}

/// Override physical-path results at the actual containment boundary. Returning
/// None uses the real filesystem. This is scoped and absent from product builds.
pub fn with_path_resolver<R>(
    resolver: impl FnMut(&Path) -> io::Result<Option<PathBuf>> + Send + 'static,
    operation: impl FnOnce() -> R,
) -> R {
    with_context(
        Context(
            OBSERVER.with(|slot| slot.borrow().clone()),
            Some(Arc::new(Mutex::new(Box::new(resolver)))),
        ),
        operation,
    )
}
pub(crate) fn resolve_path(path: &Path) -> io::Result<Option<PathBuf>> {
    let resolver = PATH_RESOLVER.with(|slot| slot.borrow().clone());
    match resolver {
        Some(resolver) => {
            resolver
                .lock()
                .map_err(|_| io::Error::other("Test path resolver lock poisoned"))?(path)
        }
        None => Ok(None),
    }
}
