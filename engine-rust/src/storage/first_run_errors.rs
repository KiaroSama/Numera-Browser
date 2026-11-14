//! Original-runtime filesystem diagnostics for the first-run importer.
use std::{fmt, io, path::Path};

fn quoted(units: &[u32]) -> String {
    let quote = if units.contains(&u32::from('\'')) && !units.contains(&u32::from('"')) {
        '"'
    } else {
        '\''
    };
    let mut result = String::from(quote);
    for &unit in units {
        match char::from_u32(unit) {
            Some(character) if character == quote => {
                result.push('\\');
                result.push(character);
            }
            Some(character @ ('\'' | '"')) => result.push(character),
            Some(character) => {
                let escaped = crate::validation::repr(&serde_json::Value::String(character.into()));
                result.push_str(&escaped[1..escaped.len() - 1]);
            }
            None => result.push_str(&format!("\\u{unit:04x}")),
        }
    }
    result.push(quote);
    result
}

fn path_units(path: &Path) -> Vec<u32> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let mut remaining = path.as_os_str().as_bytes();
        let mut units = Vec::new();
        while !remaining.is_empty() {
            match std::str::from_utf8(remaining) {
                Ok(text) => {
                    units.extend(text.chars().map(u32::from));
                    break;
                }
                Err(error) => {
                    let (valid, invalid) = remaining.split_at(error.valid_up_to());
                    units.extend(
                        std::str::from_utf8(valid)
                            .expect("Validated prefix")
                            .chars()
                            .map(u32::from),
                    );
                    let length = error.error_len().unwrap_or(invalid.len());
                    units.extend(
                        invalid[..length]
                            .iter()
                            .map(|byte| 0xdc00 + u32::from(*byte)),
                    );
                    remaining = &invalid[length..];
                }
            }
        }
        units
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        char::decode_utf16(path.as_os_str().encode_wide())
            .map(|unit| {
                unit.map(u32::from)
                    .unwrap_or_else(|error| u32::from(error.unpaired_surrogate()))
            })
            .collect()
    }
}

fn path_repr(path: &Path) -> String {
    quoted(&path_units(path))
}

#[cfg(unix)]
pub(super) fn pipe(source: &Path) -> io::Error {
    let mut units: Vec<_> = "`".chars().map(u32::from).collect();
    units.extend(path_units(source));
    units.extend("` is a named pipe".chars().map(u32::from));
    let raw = units
        .iter()
        .map(|&unit| {
            char::from_u32(unit)
                .map(|character| character.to_string())
                .unwrap_or_else(|| format!("\\u{unit:04x}"))
        })
        .collect();
    io::Error::other(QuotedMessage {
        raw,
        quoted: quoted(&units),
    })
}

#[derive(Debug)]
struct QuotedMessage {
    raw: String,
    quoted: String,
}
impl fmt::Display for QuotedMessage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.raw)
    }
}
impl std::error::Error for QuotedMessage {}

#[derive(Debug)]
struct FileError {
    code: i32,
    text: String,
    path: String,
}
impl fmt::Display for FileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let label = if cfg!(windows) { "WinError" } else { "Errno" };
        write!(
            formatter,
            "[{label} {}] {}: {}",
            self.code, self.text, self.path
        )
    }
}
impl std::error::Error for FileError {}

pub(super) fn at(error: io::Error, path: &Path) -> io::Error {
    let Some(code) = error.raw_os_error() else {
        return error;
    };
    let raw = error.to_string();
    let suffix = format!(" (os error {code})");
    let text = raw.strip_suffix(&suffix).unwrap_or(&raw).to_owned();
    io::Error::new(
        error.kind(),
        FileError {
            code,
            text,
            path: path_repr(path),
        },
    )
}

pub(super) fn native(error: &io::Error) -> bool {
    error.raw_os_error().is_some() || error.get_ref().is_some_and(|error| error.is::<FileError>())
}

fn object_repr(path: &Path) -> String {
    let class = if cfg!(windows) {
        "WindowsPath"
    } else {
        "PosixPath"
    };
    let mut units = path_units(path);
    if cfg!(windows) {
        for unit in &mut units {
            if *unit == u32::from('\\') {
                *unit = u32::from('/');
            }
        }
    }
    format!("{class}({})", quoted(&units))
}

#[derive(Debug, Default)]
pub(super) struct CopyFailures(Vec<(String, String, String)>);
#[cfg(all(feature = "test-support", target_os = "macos"))]
pub(crate) fn pipe_failure(source: &Path, destination: &Path) -> String {
    let mut failures = CopyFailures::default();
    failures.push(source, destination, pipe(source));
    failures
        .finish()
        .expect_err("one named-pipe error")
        .to_string()
}
impl CopyFailures {
    pub(super) fn push(&mut self, source: &Path, destination: &Path, error: io::Error) {
        self.collect(path_repr(source), path_repr(destination), error);
    }

    pub(super) fn metadata(
        &mut self,
        source: &Path,
        destination: &Path,
        entry: bool,
        error: io::Error,
    ) {
        let (source, destination) = if entry {
            (
                format!(
                    "<DirEntry {}>",
                    path_repr(Path::new(
                        source.file_name().expect("Nested directory has a name")
                    ))
                ),
                path_repr(destination),
            )
        } else {
            (object_repr(source), object_repr(destination))
        };
        self.collect(source, destination, error);
    }

    fn collect(&mut self, source: String, destination: String, error: io::Error) {
        if let Some(nested) = error
            .get_ref()
            .and_then(|error| error.downcast_ref::<Self>())
        {
            self.0.extend(nested.0.iter().cloned());
        } else {
            let text = if let Some(quoted) = error
                .get_ref()
                .and_then(|error| error.downcast_ref::<QuotedMessage>())
            {
                quoted.quoted.clone()
            } else {
                crate::validation::repr(&serde_json::Value::String(error.to_string()))
            };
            self.0.push((source, destination, text));
        }
    }

    pub(super) fn finish(self) -> io::Result<()> {
        if self.0.is_empty() {
            Ok(())
        } else {
            Err(io::Error::other(self))
        }
    }
}
impl fmt::Display for CopyFailures {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let rows: Vec<_> = self
            .0
            .iter()
            .map(|(source, destination, error)| format!("({source}, {destination}, {error})"))
            .collect();
        write!(formatter, "[{}]", rows.join(", "))
    }
}
impl std::error::Error for CopyFailures {}
