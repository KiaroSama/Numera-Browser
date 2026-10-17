//! Match the source text-stream newline translation only at the executable edge.
use std::io::{self, Write};

pub struct TextWriter<W>(W);
impl<W> TextWriter<W> {
    pub fn new(writer: W) -> Self {
        Self(writer)
    }
}
impl<W: Write> Write for TextWriter<W> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        #[cfg(windows)]
        {
            let mut start = 0;
            for (index, byte) in bytes.iter().enumerate() {
                if *byte == b'\n' {
                    self.0.write_all(&bytes[start..index])?;
                    self.0.write_all(b"\r\n")?;
                    start = index + 1;
                }
            }
            self.0.write_all(&bytes[start..])?;
            Ok(bytes.len())
        }
        #[cfg(not(windows))]
        self.0.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.0.flush()
    }
}
