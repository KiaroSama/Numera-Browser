use crate::validation::repr;
use serde_json::Value;
use std::{
    io,
    path::{Path, PathBuf},
};

/// Refuse lexical and physical escapes before any extraction write.
/// Resolve the original component order so a link before `..` cannot escape.
pub fn member_path(base: &Path, member: &str) -> io::Result<PathBuf> {
    let normalized = member.replace('\\', "/");
    let relative = normalized.trim_matches('/');
    let shown = repr(&Value::String(member.into()));
    if relative.is_empty() || matches!(relative, "." | "..") {
        return Err(io::Error::other(format!(
            "refusing empty or dot archive member: {shown}"
        )));
    }
    let bytes = relative.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        return Err(io::Error::other(format!(
            "{shown} names a Windows drive and cannot be part of an archive"
        )));
    }
    let mut parts = Vec::new();
    for part in relative.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.pop().is_none() {
                    return Err(io::Error::other(format!(
                        "refusing archive member that escapes its folder: {shown}"
                    )));
                }
            }
            _ => {
                if [
                    ".ignoreme",
                    "secrets.md",
                    "explain-AI.md",
                    ".ai",
                    ".specify",
                    "specs",
                    ".claude",
                    ".codex",
                    ".kiro",
                    ".cursor",
                    ".cline",
                    ".agents",
                    "graphify-out",
                    ".codebase-memory",
                    "plans",
                ]
                .contains(&part)
                {
                    return Err(io::Error::other(
                        "Protected local-only content cannot be extracted from an archive",
                    ));
                }
                if part.contains(':') || part.contains('\0') {
                    return Err(io::Error::other(
                        "Archive member contains an unsafe path component",
                    ));
                }
                parts.push(part);
            }
        }
    }
    let base = super::locks::resolved(base)?;
    let target = base.join(relative);
    let refusal = || {
        io::Error::other(format!(
            "refusing archive member that escapes its folder: {shown}"
        ))
    };
    let absolute = super::paths::lexical(&target);
    if !absolute.starts_with(&base) {
        return Err(refusal());
    }
    // Keep the original link/../ ordering for the physical check.
    let physical = super::paths::physical(&target)?;
    if !physical.starts_with(&base) {
        return Err(refusal());
    }
    #[cfg(feature = "test-support")]
    crate::vault::testing::event("before-path-final-probe", &physical)?;
    let resolved = super::locks::resolved(&physical)?;
    if !resolved.starts_with(&base) {
        return Err(refusal());
    }
    Ok(resolved)
}

/// An exclusively created directory retained until publication or cleanup.
pub struct Staging {
    path: Option<PathBuf>,
}
impl Staging {
    pub fn new(root: &Path, prefix: &str) -> io::Result<Self> {
        let path = root.join(format!("{prefix}{}", random_hex(16)?));
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            std::fs::DirBuilder::new().mode(0o700).create(&path)?;
        }
        #[cfg(not(unix))]
        std::fs::create_dir(&path)?;
        Ok(Self { path: Some(path) })
    }
    pub fn path(&self) -> io::Result<&Path> {
        self.path
            .as_deref()
            .ok_or_else(|| io::Error::other("Restore staging was already transferred"))
    }
    pub fn transfer(&mut self, destination: &Path) -> io::Result<()> {
        std::fs::rename(self.path()?, destination)?;
        self.path = None;
        Ok(())
    }
    pub fn cleanup(&mut self) -> io::Result<()> {
        if let Some(path) = &self.path {
            std::fs::remove_dir_all(path)?;
            self.path = None;
        }
        Ok(())
    }
}
impl Drop for Staging {
    fn drop(&mut self) {
        if let Err(error) = self.cleanup() {
            eprintln!("WARNING: Restore staging cleanup failed: {error}");
        }
    }
}

pub(crate) fn random_hex(count: usize) -> io::Result<String> {
    let mut bytes = vec![0u8; count];
    getrandom::fill(&mut bytes).map_err(|error| {
        io::Error::other(format!("Archive identity randomness unavailable: {error}"))
    })?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}
