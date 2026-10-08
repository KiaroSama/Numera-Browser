use std::borrow::Cow;

/// JSON bytes permit BOM and zero-byte autodetection independently of the media
/// type charset parameter. Plain non-JSON request bytes do not use this path.
pub(super) fn decode(bytes: &[u8]) -> Result<Cow<'_, str>, ()> {
    if let Some(rest) = bytes.strip_prefix(&[0, 0, 0xfe, 0xff]) {
        return utf32(rest, false).map(Cow::Owned);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xff, 0xfe, 0, 0]) {
        return utf32(rest, true).map(Cow::Owned);
    }
    if let Some(rest) = bytes.strip_prefix(&[0xfe, 0xff]) {
        return String::from_utf16be(rest).map(Cow::Owned).map_err(|_| ());
    }
    if let Some(rest) = bytes.strip_prefix(&[0xff, 0xfe]) {
        return String::from_utf16le(rest).map(Cow::Owned).map_err(|_| ());
    }
    if let Some(rest) = bytes.strip_prefix(&[0xef, 0xbb, 0xbf]) {
        return std::str::from_utf8(rest).map(Cow::Borrowed).map_err(|_| ());
    }
    if bytes.len() >= 4 {
        if bytes[0] == 0 {
            return if bytes[1] == 0 {
                utf32(bytes, false).map(Cow::Owned)
            } else {
                String::from_utf16be(bytes).map(Cow::Owned).map_err(|_| ())
            };
        }
        if bytes[1] == 0 {
            return if bytes[2] == 0 && bytes[3] == 0 {
                utf32(bytes, true).map(Cow::Owned)
            } else {
                String::from_utf16le(bytes).map(Cow::Owned).map_err(|_| ())
            };
        }
    } else if bytes.len() == 2 {
        if bytes[0] == 0 {
            return String::from_utf16be(bytes).map(Cow::Owned).map_err(|_| ());
        }
        if bytes[1] == 0 {
            return String::from_utf16le(bytes).map(Cow::Owned).map_err(|_| ());
        }
    }
    std::str::from_utf8(bytes)
        .map(Cow::Borrowed)
        .map_err(|_| ())
}
fn utf32(bytes: &[u8], little: bool) -> Result<String, ()> {
    if !bytes.len().is_multiple_of(4) {
        return Err(());
    }
    let mut text = String::new();
    for chunk in bytes.as_chunks::<4>().0 {
        let scalar = if little {
            u32::from_le_bytes(*chunk)
        } else {
            u32::from_be_bytes(*chunk)
        };
        text.push(char::from_u32(scalar).ok_or(())?);
    }
    Ok(text)
}
