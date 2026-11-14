//! A source text-mode writer uses the native newline spelling; caller bytes
//! and refused writes must never pass through this conversion.
pub fn written_bytes(bytes: Vec<u8>) -> Vec<u8> {
    #[cfg(windows)]
    {
        String::from_utf8(bytes)
            .expect("Source writer produced UTF-8 text")
            .replace("\r\n", "\n")
            .replace('\n', "\r\n")
            .into_bytes()
    }
    #[cfg(not(windows))]
    bytes
}
