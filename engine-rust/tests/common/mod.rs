use std::{
    fs, io,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static NONCE: AtomicU64 = AtomicU64::new(0);

/// Own only a newly created test root. Unwinding must not leave failed cases
/// behind, and a recycled process id must not inherit a prior run's state.
pub struct TempDirectory {
    path: PathBuf,
    removed: bool,
}
impl TempDirectory {
    pub fn new(prefix: &str) -> io::Result<Self> {
        let parent = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/test-temp");
        fs::create_dir_all(&parent)?;
        for _ in 0..32 {
            let path = parent.join(format!(
                "{prefix}-{}-{}",
                std::process::id(),
                NONCE.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&path) {
                Ok(()) => {
                    return Ok(Self {
                        path,
                        removed: false,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::other(
            "Cannot create an exclusive test directory",
        ))
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn cleanup(mut self) -> io::Result<()> {
        fs::remove_dir_all(&self.path)?;
        self.removed = true;
        Ok(())
    }
}
impl Drop for TempDirectory {
    fn drop(&mut self) {
        if !self.removed
            && let Err(error) = fs::remove_dir_all(&self.path)
            && error.kind() != io::ErrorKind::NotFound
        {
            eprintln!(
                "Test directory cleanup failed at {}: {error}",
                self.path.display()
            );
        }
    }
}
