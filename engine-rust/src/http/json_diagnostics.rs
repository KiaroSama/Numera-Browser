//! Syntax-only compatibility diagnostics. Serde remains the value decoder.
//! A decoder disagreement without a proven syntax error is not a Python verdict.
pub(super) struct Diagnostic {
    pub position: usize,
    pub message: &'static str,
}
struct Scanner<'a> {
    bytes: &'a [u8],
    offset: usize,
    unsupported: bool,
}
type Parse = Result<(), Diagnostic>;

pub(super) fn diagnose(text: &str) -> Option<Diagnostic> {
    let mut scanner = Scanner {
        bytes: text.as_bytes(),
        offset: 0,
        unsupported: false,
    };
    scanner.space();
    let result = scanner.value(0).and_then(|()| {
        scanner.space();
        if scanner.offset == scanner.bytes.len() {
            Ok(())
        } else {
            scanner.fail("Extra data", scanner.offset)
        }
    });
    if scanner.unsupported {
        return None;
    }
    result.err().map(|mut error| {
        error.position = text
            .char_indices()
            .take_while(|(offset, _)| *offset < error.position)
            .count();
        error
    })
}
impl Scanner<'_> {
    fn fail(&self, message: &'static str, position: usize) -> Parse {
        Err(Diagnostic { position, message })
    }
    fn current(&self) -> Option<u8> {
        self.bytes.get(self.offset).copied()
    }
    fn space(&mut self) {
        while self
            .current()
            .is_some_and(|byte| matches!(byte, b' ' | b'\t' | b'\r' | b'\n'))
        {
            self.offset += 1;
        }
    }
    fn value(&mut self, depth: usize) -> Parse {
        // The caller handles deeper decoder disagreements explicitly, never as
        // a guessed original syntax error; bound recursion on untrusted input.
        if depth >= 128 {
            self.unsupported = true;
            return Ok(());
        }
        match self.current() {
            Some(b'"') => self.string(),
            Some(b'{') => self.object(depth + 1),
            Some(b'[') => self.array(depth + 1),
            Some(b'n') => self.literal(b"null"),
            Some(b't') => self.literal(b"true"),
            Some(b'f') => self.literal(b"false"),
            Some(b'N') => self.literal(b"NaN"),
            Some(b'I') => self.literal(b"Infinity"),
            Some(b'-') if self.bytes[self.offset..].starts_with(b"-Infinity") => {
                self.literal(b"-Infinity")
            }
            Some(b'-' | b'0'..=b'9') => self.number(),
            _ => self.fail("Expecting value", self.offset),
        }
    }
    fn literal(&mut self, literal: &[u8]) -> Parse {
        if self.bytes[self.offset..].starts_with(literal) {
            self.offset += literal.len();
            Ok(())
        } else {
            self.fail("Expecting value", self.offset)
        }
    }
    fn string(&mut self) -> Parse {
        let start = self.offset;
        self.offset += 1;
        while let Some(byte) = self.current() {
            match byte {
                b'"' => {
                    self.offset += 1;
                    return Ok(());
                }
                b'\\' => {
                    let slash = self.offset;
                    self.offset += 1;
                    match self.current() {
                        Some(b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't') => {
                            self.offset += 1
                        }
                        Some(b'u') => {
                            let u = self.offset;
                            for _ in 0..4 {
                                self.offset += 1;
                                if !self
                                    .current()
                                    .is_some_and(|digit| digit.is_ascii_hexdigit())
                                {
                                    return self.fail("Invalid \\uXXXX escape", u);
                                }
                            }
                            self.offset += 1;
                        }
                        Some(_) => return self.fail("Invalid \\escape", slash),
                        None => return self.fail("Unterminated string starting at", start),
                    }
                }
                0..=31 => return self.fail("Invalid control character at", self.offset),
                _ => self.offset += 1,
            }
        }
        self.fail("Unterminated string starting at", start)
    }
    fn number(&mut self) -> Parse {
        let start = self.offset;
        if self.current() == Some(b'-') {
            self.offset += 1;
        }
        match self.current() {
            Some(b'0') => self.offset += 1,
            Some(b'1'..=b'9') => {
                self.offset += 1;
                self.digits();
            }
            _ => return self.fail("Expecting value", start),
        }
        if self.current() == Some(b'.')
            && self
                .bytes
                .get(self.offset + 1)
                .is_some_and(u8::is_ascii_digit)
        {
            self.offset += 2;
            self.digits();
        }
        if matches!(self.current(), Some(b'e' | b'E')) {
            let exponent = self.offset;
            self.offset += 1;
            if matches!(self.current(), Some(b'+' | b'-')) {
                self.offset += 1;
            }
            if self.current().is_some_and(|byte| byte.is_ascii_digit()) {
                self.digits();
            } else {
                self.offset = exponent;
            }
        }
        Ok(())
    }
    fn digits(&mut self) {
        while self.current().is_some_and(|byte| byte.is_ascii_digit()) {
            self.offset += 1;
        }
    }
    fn object(&mut self, depth: usize) -> Parse {
        self.offset += 1;
        self.space();
        if self.current() == Some(b'}') {
            self.offset += 1;
            return Ok(());
        }
        loop {
            if self.current() != Some(b'"') {
                return self.fail(
                    "Expecting property name enclosed in double quotes",
                    self.offset,
                );
            }
            self.string()?;
            self.space();
            if self.current() != Some(b':') {
                return self.fail("Expecting ':' delimiter", self.offset);
            }
            self.offset += 1;
            self.space();
            self.value(depth)?;
            self.space();
            match self.current() {
                Some(b'}') => {
                    self.offset += 1;
                    return Ok(());
                }
                Some(b',') => {
                    let comma = self.offset;
                    self.offset += 1;
                    self.space();
                    if self.current() == Some(b'}') {
                        return self.fail("Illegal trailing comma before end of object", comma);
                    }
                }
                _ => return self.fail("Expecting ',' delimiter", self.offset),
            }
        }
    }
    fn array(&mut self, depth: usize) -> Parse {
        self.offset += 1;
        self.space();
        if self.current() == Some(b']') {
            self.offset += 1;
            return Ok(());
        }
        loop {
            self.value(depth)?;
            self.space();
            match self.current() {
                Some(b']') => {
                    self.offset += 1;
                    return Ok(());
                }
                Some(b',') => {
                    let comma = self.offset;
                    self.offset += 1;
                    self.space();
                    if self.current() == Some(b']') {
                        return self.fail("Illegal trailing comma before end of array", comma);
                    }
                }
                _ => return self.fail("Expecting ',' delimiter", self.offset),
            }
        }
    }
}
