use crate::storage::{data_root::expand_home, locks::resolved};
use std::{io, path::Path};

/// Resolve launch resources against the caller's paths, not an SDK process CWD.
/// Named-user homes require a platform account resolver before this boundary.
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
        if !path.is_absolute()
            && path.components().next().is_some_and(|component| {
                component
                    .as_os_str()
                    .to_str()
                    .is_some_and(|value| value.starts_with('~') && value != "~")
            })
        {
            return Err(io::Error::new(
                io::ErrorKind::Unsupported,
                "Named-user extension paths require account resolution",
            ));
        }
        let path = expand_home(path, home);
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
