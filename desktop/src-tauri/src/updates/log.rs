use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    path::Path,
    sync::Mutex,
    time::{SystemTime, UNIX_EPOCH},
};
pub struct Log(Mutex<Option<File>>);
fn stamp() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |value| value.as_secs());
    let days = (seconds / 86400) as i64 + 719468;
    let era = if days >= 0 { days } else { days - 146096 } / 146097;
    let doe = days - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += if month <= 2 { 1 } else { 0 };
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02} UTC",
        seconds / 3600 % 24,
        seconds / 60 % 60,
        seconds % 60
    )
}
impl Log {
    pub fn new(root: &Path, component: &str) -> Self {
        let result = (|| -> io::Result<File> {
            let directory = root.join("logs");
            std::fs::create_dir_all(&directory)?;
            let name = stamp().replace(' ', "_").replace(':', "-");
            for suffix in 0..32 {
                let path = directory.join(format!(
                    "{component}_{name}{}{}.log",
                    if suffix == 0 { "" } else { "_" },
                    if suffix == 0 {
                        String::new()
                    } else {
                        suffix.to_string()
                    }
                ));
                match OpenOptions::new().append(true).create_new(true).open(path) {
                    Ok(file) => return Ok(file),
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(error),
                }
            }
            Err(io::Error::other("No unique update log name"))
        })();
        if result.is_err() {
            eprintln!("Update diagnostic file unavailable; using safe console fallback");
        }
        Self(Mutex::new(result.ok()))
    }
    pub fn event(&self, level: &str, message: &str) {
        let line = format!("[{}] [{level}] [updates] {message}\n", stamp());
        if let Ok(mut file) = self.0.lock() {
            if let Some(file) = file.as_mut() {
                let _ = file.write_all(line.as_bytes());
                let _ = file.flush();
            } else {
                eprint!("{line}");
            }
        }
    }
}
