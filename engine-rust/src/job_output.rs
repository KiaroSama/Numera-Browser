//! Stream Python-compatible decoded log lines with bounded resident output.
use std::{
    collections::VecDeque,
    fs::File,
    io::{self, Read},
    path::Path,
    time::{Duration, Instant},
};

pub fn tail(path: &Path, lines: i64) -> io::Result<Vec<String>> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let end = Instant::now() + Duration::from_secs(10);
    let mut output = VecDeque::new();
    let mut line = String::new();
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 8192];
    let mut cr = false;
    let mut seen = 0u64;
    let mut retained = 0usize;
    loop {
        if Instant::now() >= end {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Job log scan exceeded its time budget",
            ));
        }
        let count = file.read(&mut chunk)?;
        bytes.extend_from_slice(&chunk[..count]);
        let mut cursor = 0;
        while cursor < bytes.len() {
            let (text, consume, replacement) = match std::str::from_utf8(&bytes[cursor..]) {
                Ok(text) => (text, text.len(), false),
                Err(error) => {
                    let valid = error.valid_up_to();
                    if valid > 0 {
                        let text = std::str::from_utf8(&bytes[cursor..cursor + valid])
                            .map_err(io::Error::other)?;
                        (text, valid, false)
                    } else if let Some(length) = error.error_len() {
                        ("", length, true)
                    } else if count == 0 {
                        ("", bytes.len() - cursor, true)
                    } else {
                        break;
                    }
                }
            };
            for ch in text.chars().chain(replacement.then_some('\u{fffd}')) {
                if cr && ch == '\n' {
                    cr = false;
                    continue;
                }
                cr = ch == '\r';
                if matches!(
                    ch,
                    '\n' | '\r'
                        | '\u{b}'
                        | '\u{c}'
                        | '\u{1c}'
                        | '\u{1d}'
                        | '\u{1e}'
                        | '\u{85}'
                        | '\u{2028}'
                        | '\u{2029}'
                ) {
                    push(
                        &mut output,
                        std::mem::take(&mut line),
                        lines,
                        &mut seen,
                        &mut retained,
                    )?;
                } else {
                    line.push(ch);
                    if line.len() > crate::mcp::MAX_FRAME {
                        return Err(too_large());
                    }
                }
            }
            cursor += consume;
        }
        bytes.drain(..cursor);
        if count == 0 {
            break;
        }
    }
    if !line.is_empty() {
        push(&mut output, line, lines, &mut seen, &mut retained)?;
    }
    Ok(output.into_iter().collect())
}
fn push(
    output: &mut VecDeque<String>,
    line: String,
    lines: i64,
    seen: &mut u64,
    retained: &mut usize,
) -> io::Result<()> {
    *seen += 1;
    if lines < 0 && *seen <= lines.unsigned_abs() {
        return Ok(());
    }
    *retained += line.len() + 1;
    output.push_back(line);
    if lines > 0
        && output.len() as u64 > lines as u64
        && let Some(first) = output.pop_front()
    {
        *retained -= first.len() + 1;
    }
    if *retained > crate::mcp::MAX_FRAME {
        return Err(too_large());
    }
    Ok(())
}
fn too_large() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "Job output exceeds the bounded response size",
    )
}
