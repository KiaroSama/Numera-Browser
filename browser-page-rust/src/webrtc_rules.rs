//! Pure candidate policy used by the actual browser event and SDP wrappers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Altered,
    Disabled,
    Real,
}
pub fn mode(value: &str) -> Mode {
    match value {
        "disabled" => Mode::Disabled,
        "real" => Mode::Real,
        _ => Mode::Altered,
    }
}
pub(crate) fn ecma_whitespace(ch: char) -> bool {
    matches!(ch, '\u{0009}'..='\u{000d}' | '\u{0020}' | '\u{00a0}' | '\u{1680}'
        | '\u{2000}'..='\u{200a}' | '\u{2028}' | '\u{2029}' | '\u{202f}'
        | '\u{205f}' | '\u{3000}' | '\u{feff}')
}
pub fn candidate_address(line: &str) -> Option<&str> {
    let candidate = line
        .strip_prefix("a=")
        .unwrap_or(line)
        .strip_prefix("candidate:")?;
    // ASCII separators and ECMAScript \S/\d preserve the original predicate.
    let mut fields = candidate.split(' ');
    let foundation = fields.next()?;
    let component = fields.next()?;
    let transport = fields.next()?;
    let priority = fields.next()?;
    let address = fields.next()?;
    if foundation.is_empty()
        || foundation.chars().any(ecma_whitespace)
        || component.is_empty()
        || !component.bytes().all(|b| b.is_ascii_digit())
        || transport.is_empty()
        || transport.chars().any(ecma_whitespace)
        || priority.is_empty()
        || !priority.bytes().all(|b| b.is_ascii_digit())
        || address.is_empty()
        || address.chars().any(ecma_whitespace)
        || fields.next().is_none()
    {
        None
    } else {
        Some(address)
    }
}
pub fn safe_candidate(line: &str) -> bool {
    if line.match_indices(".local").any(|(index, _)| {
        line[index + 6..]
            .chars()
            .next()
            .is_none_or(|ch| !(ch.is_ascii_alphanumeric() || ch == '_'))
    }) {
        return true;
    }
    let Some(ip) = candidate_address(line) else {
        return true;
    };
    if ip.to_ascii_lowercase().ends_with(".local") {
        return true;
    }
    if ip.contains(':') {
        let address = ip.split('%').next().unwrap_or(ip).to_ascii_lowercase();
        address == "::1"
            || address == "::"
            || address.starts_with("fe8")
            || address.starts_with("fe9")
            || address.starts_with("fea")
            || address.starts_with("feb")
            || address.starts_with("fc")
            || address.starts_with("fd")
    } else {
        ip.starts_with("10.")
            || ip.starts_with("127.")
            || ip.starts_with("0.")
            || ip.starts_with("169.254.")
            || ip.starts_with("192.168.")
            || (16..=31).any(|n| ip.starts_with(&format!("172.{n}.")))
    }
}
pub fn scrub_sdp(sdp: &str) -> String {
    sdp.replace("\r\n", "\n")
        .split('\n')
        .filter(|line| !line.starts_with("a=candidate:") || safe_candidate(line))
        .collect::<Vec<_>>()
        .join("\r\n")
}
pub fn drop_candidate(line: Option<&str>) -> bool {
    line.is_some_and(|line| !line.is_empty() && !safe_candidate(line))
}
