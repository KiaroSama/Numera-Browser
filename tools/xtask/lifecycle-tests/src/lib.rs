//! Platform-independent lifecycle contract harness. The lifecycle code and tests
//! below are the production files, not copies. This does not test a Tauri window
//! or updater integration; those still require native desktop SDKs.
#![allow(dead_code)]
extern crate self as numera_desktop;
extern crate self as tauri;

pub struct AppHandle;
pub struct State<'a, T> {
    value: &'a T,
}
impl<T> std::ops::Deref for State<'_, T> {
    type Target = T;
    fn deref(&self) -> &T {
        self.value
    }
}
pub trait Manager {
    fn try_state<T>(&self) -> Option<State<'_, T>> {
        None
    }
}
impl Manager for AppHandle {}

pub mod updates {
    pub mod transaction {
        pub struct Journal {
            pub root: std::path::PathBuf,
            pub nonce: String,
            pub ticket: String,
        }
        impl Journal {
            pub fn load(_: &std::path::Path) -> std::io::Result<Self> {
                Err(std::io::Error::other(
                    "updater integration is not provided by the lifecycle harness",
                ))
            }
        }
    }
}

#[path = "../../../../desktop/src-tauri/src/lifecycle.rs"]
mod lifecycle;
