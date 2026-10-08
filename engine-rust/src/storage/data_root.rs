use std::{
    fs::{self, OpenOptions},
    io,
    path::{Path, PathBuf},
};

fn expand_home(path: &Path, home: &Path) -> PathBuf {
    if path == Path::new("~") {
        return home.to_owned();
    }
    if let Ok(tail) = path.strip_prefix("~") {
        return home.join(tail);
    }
    path.to_owned()
}

fn writable(directory: &Path) -> io::Result<bool> {
    if fs::create_dir_all(directory).is_err() {
        return Ok(false);
    }
    let mut nonce = [0u8; 16];
    getrandom::fill(&mut nonce).map_err(|error| {
        io::Error::other(format!("Data-root probe randomness unavailable: {error}"))
    })?;
    let name = nonce
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    let path = directory.join(format!(".wtest-{name}"));
    let file = match OpenOptions::new().write(true).create_new(true).open(&path) {
        Ok(file) => file,
        Err(_) => return Ok(false),
    };
    drop(file);
    // A failed probe cleanup is not an ordinary read-only fallback: name the
    // failure instead of abandoning a file while reporting success.
    fs::remove_file(path)?;
    Ok(true)
}

/// Environment/application/home discovery belongs to the executable adapter.
/// This boundary accepts explicit paths so isolated tests never probe user data.
pub fn resolve(
    explicit: Option<&Path>,
    environment: Option<&Path>,
    app: &Path,
    home: &Path,
) -> io::Result<PathBuf> {
    for path in [explicit, environment].into_iter().flatten() {
        if !path.as_os_str().is_empty() {
            return Ok(expand_home(path, home));
        }
    }
    let portable = app.join("data");
    if writable(&portable)? {
        return Ok(portable);
    }
    let current = home.join(".numera");
    let legacy = home.join(".persona");
    Ok(if !current.exists() && legacy.exists() {
        legacy
    } else {
        current
    })
}
