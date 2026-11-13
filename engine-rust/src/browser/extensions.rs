use crate::storage::{data_root::expand_home, locks::resolved};
use std::{io, path::Path};

/// Resolve launch resources against the caller's paths, not an SDK process CWD.
/// Current and named-user homes use the shared platform account resolver.
pub fn paths(entries: &[String], cwd: &Path, home: &Path) -> io::Result<Vec<String>> {
    if !cwd.is_absolute() || !home.is_absolute() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Extension base paths must be absolute",
        ));
    }
    let mut paths = Vec::new();
    for entry in entries {
        let path = Path::new(entry);
        let path = expand_home(path, home)?;
        #[cfg(windows)]
        let path = {
            use std::path::{Component, Prefix};
            if !path.has_root()
                && let Some(Component::Prefix(prefix)) = path.components().next()
            {
                let drive = match prefix.kind() {
                    Prefix::Disk(drive) => drive,
                    _ => {
                        return Err(io::Error::new(
                            io::ErrorKind::Unsupported,
                            "Extension path prefix cannot be resolved",
                        ));
                    }
                };
                let current_drive = cwd
                    .components()
                    .next()
                    .and_then(|component| match component {
                        Component::Prefix(prefix) => match prefix.kind() {
                            Prefix::Disk(drive) | Prefix::VerbatimDisk(drive) => Some(drive),
                            _ => None,
                        },
                        _ => None,
                    });
                if current_drive != Some(drive) {
                    return Err(io::Error::new(
                        io::ErrorKind::Unsupported,
                        "Cross-drive relative extension paths require a working directory for that drive",
                    ));
                }
                cwd.join(
                    path.strip_prefix(prefix.as_os_str())
                        .map_err(io::Error::other)?,
                )
            } else {
                path
            }
        };
        let path = if path.is_absolute() {
            path
        } else {
            cwd.join(path)
        };
        if path.is_dir() {
            paths.push(
                resolved(&path)?
                    .into_os_string()
                    .into_string()
                    .map_err(|_| {
                        io::Error::new(io::ErrorKind::InvalidData, "Extension path is not Unicode")
                    })?,
            );
        }
    }
    Ok(paths)
}
