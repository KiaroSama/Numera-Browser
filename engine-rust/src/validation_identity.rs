//! Standalone validation helpers for optional per-profile MAC address and
//! device name fields so historical `validation.rs` snapshots keep compiling.

pub(crate) fn valid_mac_address(value: &str) -> bool {
    value.len() == 17
        && value.bytes().enumerate().all(|(index, byte)| {
            if index % 3 == 2 {
                byte == b':'
            } else {
                matches!(byte, b'0'..=b'9' | b'A'..=b'F')
            }
        })
}

pub(crate) fn valid_device_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}
