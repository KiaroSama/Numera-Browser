use super::adapter::Adapter;
use serde_json::{Map, Value};
use std::{io, time::Duration};

/// Read the SDK jar. Application-format normalization is a separate boundary.
pub fn read(adapter: &mut Adapter) -> io::Result<Vec<Value>> {
    let result = adapter.call("cookies-read", Map::new(), Duration::from_secs(10))?;
    let rows = result["cookies"].as_array().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "Browser cookie result missing")
    })?;
    validate(rows)?;
    Ok(rows.clone())
}

/// Preserve the original no-op and clear-before-add decisions outside the SDK.
/// Input rows have already passed the application's cookie normalization.
pub fn write(
    cookies: &[Value],
    clear: bool,
    activate: impl FnOnce() -> io::Result<Adapter>,
) -> io::Result<usize> {
    validate(cookies)?;
    if cookies.is_empty() && !clear {
        return Ok(0);
    }
    let mut adapter = activate()?;
    let result: io::Result<usize> = (|| {
        if clear {
            let result = adapter.call("cookies-clear", Map::new(), Duration::from_secs(10))?;
            confirmed(&result, "cleared")?;
        }
        if !cookies.is_empty() {
            let result = adapter.call(
                "cookies-add",
                Map::from_iter([("cookies".into(), Value::Array(cookies.to_vec()))]),
                Duration::from_secs(10),
            )?;
            confirmed(&result, "added")?;
        }
        Ok(cookies.len())
    })();
    // SDK failure must not skip native ownership settlement or fabricate success.
    let exit = adapter.close(Duration::from_secs(10))?;
    let count = result?;
    if exit != Some(0) {
        return Err(io::Error::other(
            "Browser cookie session did not close successfully",
        ));
    }
    Ok(count)
}

fn confirmed(result: &Value, key: &str) -> io::Result<()> {
    if result[key] != true {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Browser cookie operation was not confirmed",
        ));
    }
    Ok(())
}

fn validate(cookies: &[Value]) -> io::Result<()> {
    if cookies.iter().any(|cookie| !cookie.is_object()) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Normalized cookie rows must be objects",
        ));
    }
    Ok(())
}
