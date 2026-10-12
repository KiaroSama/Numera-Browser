use numera_core::ownership::claims;
use serde_json::Value;
use std::{
    io::{self, Write},
    path::PathBuf,
    sync::{Arc, Mutex},
};

pub struct Published {
    pub bytes: Arc<Mutex<Vec<u8>>>,
    pub root: PathBuf,
    pub release: PathBuf,
    pub ready: PathBuf,
}
impl Write for Published {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.bytes
            .lock()
            .map_err(|_| io::Error::other("Fixture output lock poisoned"))?
            .extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        let bytes = self
            .bytes
            .lock()
            .map_err(|_| io::Error::other("Fixture output lock poisoned"))?;
        if bytes.ends_with(b"\n") && serde_json::from_slice::<Value>(&bytes).is_ok() {
            assert!(self.ready.exists());
            assert_ne!(
                claims::state(&claims::read(&self.root, "fixture"), "fixture-host"),
                claims::State::Free
            );
            std::fs::write(&self.release, b"close")?;
        }
        Ok(())
    }
}
