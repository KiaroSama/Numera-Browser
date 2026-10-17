use serde::Serialize;
use serde_json::Value;
use std::io::{self, BufRead, Read, Write};

#[derive(Clone, Copy)]
pub enum Tone {
    Bold,
    Dim,
    Green,
    Red,
    Yellow,
    Cyan,
}
impl Tone {
    fn escape(self) -> &'static str {
        match self {
            Self::Bold => "\x1b[1m",
            Self::Dim => "\x1b[2m",
            Self::Green => "\x1b[32m",
            Self::Red => "\x1b[31m",
            Self::Yellow => "\x1b[33m",
            Self::Cyan => "\x1b[36m",
        }
    }
}
/// The output owner writes either human narration or one structured result.
/// Diagnostics never enter stdout; callers supply already-sanitized errors.
pub struct Output<W, E> {
    stdout: W,
    stderr: E,
    json: bool,
    tty: bool,
    emitted: bool,
}
impl<W: Write, E: Write> Output<W, E> {
    pub fn new(stdout: W, stderr: E, json: bool, tty: bool) -> Self {
        Self {
            stdout,
            stderr,
            json,
            tty,
            emitted: false,
        }
    }
    pub fn json_mode(&self) -> bool {
        self.json
    }
    pub fn style(&self, text: &str, tone: Tone) -> String {
        if self.tty {
            format!("{}{text}\x1b[0m", tone.escape())
        } else {
            text.into()
        }
    }
    pub fn say(&mut self, text: &str, tone: Option<Tone>) -> io::Result<()> {
        if self.json {
            return Ok(());
        }
        if self.tty
            && let Some(tone) = tone
        {
            writeln!(self.stdout, "{}{text}\x1b[0m", tone.escape())?;
        } else {
            writeln!(self.stdout, "{text}")?;
        }
        self.stdout.flush()
    }
    pub fn emitted(&self) -> bool {
        self.emitted
    }
    pub fn emit(&mut self, payload: &Value, code: i32) -> io::Result<i32> {
        if self.json {
            if self.emitted {
                return Err(io::Error::other(
                    "Structured command output was already emitted",
                ));
            }
            let mut document = Vec::new();
            payload
                .serialize(&mut serde_json::Serializer::with_formatter(
                    &mut document,
                    crate::identity_script::SourceJson,
                ))
                .map_err(io::Error::other)?;
            document.push(b'\n');
            // Set before I/O: a partially written document must never be retried
            // as another JSON document on the same output stream.
            self.emitted = true;
            self.stdout.write_all(&document)?;
            self.stdout.flush()?;
        }
        Ok(code)
    }
    pub fn error(&mut self, message: &str) -> io::Result<i32> {
        self.say(message, Some(Tone::Red))?;
        self.emit(&serde_json::json!({"ok":false,"error":message}), 1)
    }
    pub fn confirm(&mut self, prompt: &str, input: &mut impl BufRead) -> io::Result<bool> {
        if self.json {
            return Err(io::Error::other(
                "this needs confirmation — pass -y (a --json run never prompts)",
            ));
        }
        self.stdout.write_all(prompt.as_bytes())?;
        self.stdout.flush()?;
        let mut text = String::new();
        let bytes = input.take(65537).read_line(&mut text)?;
        if bytes == 0 {
            return Err(io::Error::other(
                "nothing to read the confirmation from — pass -y",
            ));
        }
        if bytes > 65536 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Confirmation input is too long",
            ));
        }
        Ok(text.trim().to_lowercase() == "y")
    }
    pub fn into_parts(self) -> (W, E) {
        (self.stdout, self.stderr)
    }
}
