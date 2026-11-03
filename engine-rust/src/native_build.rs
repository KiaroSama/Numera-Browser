//! Non-secret identity of an explicitly built acceptance candidate, never a release signature.
use serde_json::{Value, json};
use std::io;

pub fn identity() -> io::Result<Value> {
    record(option_env!("GITHUB_SHA"), option_env!("GITHUB_RUN_ID"))
}

fn record(source: Option<&str>, generation: Option<&str>) -> io::Result<Value> {
    let invalid = || io::Error::other("Native candidate build identity is missing or invalid");
    let source = source.ok_or_else(invalid)?;
    let generation = generation.ok_or_else(invalid)?;
    if source.len() != 40
        || !source
            .bytes()
            .all(|value| value.is_ascii_digit() || (b'a'..=b'f').contains(&value))
        || generation.is_empty()
        || generation.len() > 16
        || generation.starts_with('0')
        || !generation.bytes().all(|value| value.is_ascii_digit())
    {
        return Err(invalid());
    }
    let generation: u64 = generation.parse().map_err(|_| invalid())?;
    Ok(json!({
        "schema":1,"owner":"KiaroSama","product":"Numera Browser",
        "version":env!("CARGO_PKG_VERSION"),"source_sha":source,"generation":generation,
        "application_runtime":"native-candidate","acceptance":true
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_identity_requires_canonical_compile_time_metadata() -> io::Result<()> {
        let source = "a".repeat(40);
        assert_eq!(record(Some(&source), Some("42"))?["generation"], 42);
        for (source, generation) in [
            (None, Some("42")),
            (Some(source.as_str()), None),
            (Some("unknown"), Some("42")),
            (Some(source.as_str()), Some("0")),
            (Some(source.as_str()), Some("042")),
            (Some(source.as_str()), Some("42\n")),
            (Some(source.as_str()), Some("-1")),
        ] {
            assert!(record(source, generation).is_err());
        }
        assert!(record(Some(&"A".repeat(40)), Some("42")).is_err());
        Ok(())
    }
}
