//! Build and check the benchmark baseline with synthetic profile data.
use super::{Result, SENTINEL, fail};
use serde_json::{Value, json};
use std::{
    fs, io,
    path::{Path, PathBuf},
};
pub struct Baseline {
    pub root: PathBuf,
    pub id: String,
    pub payload: Value,
}
pub fn fixture_expiry(now: u64) -> Result<u64> {
    // Chromium caps persistent cookies at400 days. Keep a single fixture
    // deadline inside that range so the browser need not rewrite its value.
    now.checked_add(30 * 24 * 60 * 60).ok_or_else(|| {
        io::Error::other("Browser fixture expiry exceeds its timestamp bound").into()
    })
}
/// Keep full, type-sensitive parity while reporting only bounded field/type
/// metadata. Cookie names, values, domains and arbitrary vendor keys stay private.
pub fn require_cookie_persistence(observed: &Value, expected: &Value) -> Result<()> {
    if crate::acceptance::require_equal(observed, expected, "Native checkpoint cookie persistence")
        .is_ok()
    {
        return Ok(());
    }
    fn kind(value: Option<&Value>) -> &'static str {
        match value {
            None => "missing",
            Some(Value::Null) => "null",
            Some(Value::Bool(_)) => "boolean",
            Some(Value::String(_)) => "string",
            Some(Value::Array(_)) => "array",
            Some(Value::Object(_)) => "object",
            Some(Value::Number(number)) if number.to_string().contains(['.', 'e', 'E']) => "float",
            Some(Value::Number(_)) => "integer",
        }
    }
    let (Some(actual), Some(wanted)) = (observed.as_array(), expected.as_array()) else {
        return fail(format!(
            "Native checkpoint cookie persistence differs expected={} actual={} (values redacted)",
            kind(Some(expected)),
            kind(Some(observed))
        ));
    };
    let mut changes = Vec::new();
    for index in 0..actual.len().max(wanted.len()).min(4) {
        let (row, seed) = (actual.get(index), wanted.get(index));
        if row == seed {
            continue;
        }
        if kind(row) != kind(seed) {
            changes.push(format!(
                "row={index} expected={} actual={}",
                kind(seed),
                kind(row)
            ));
        } else {
            let mut described = false;
            for field in [
                "name",
                "value",
                "domain",
                "path",
                "expires",
                "secure",
                "httpOnly",
                "sameSite",
                "partitionKey",
            ] {
                let before = seed.and_then(|row| row.get(field));
                let after = row.and_then(|row| row.get(field));
                if before != after && changes.len() < 8 {
                    changes.push(format!(
                        "row={index} field={field} expected={} actual={}",
                        kind(before),
                        kind(after)
                    ));
                    described = true;
                }
            }
            if !described && changes.len() < 8 {
                changes.push(format!(
                    "row={index} other-fields differ (keys and values redacted)"
                ));
            }
        }
        if changes.len() >= 8 {
            break;
        }
    }
    fail(format!(
        "Native checkpoint cookie persistence differs expected_rows={} actual_rows={} changes=[{}] (values redacted)",
        wanted.len(),
        actual.len(),
        changes.join("; ")
    ))
}
pub(super) fn require_export_metadata(
    exported: &Value,
    destination: &Path,
    label: &str,
) -> Result<()> {
    let expected = json!({"ok":true,"count":2,"file":destination,"format":"json"});
    if let Err(error) = crate::acceptance::require_equal(exported, &expected, label) {
        // Report only fixed comparisons and a bounded count, never output values or paths.
        let count = match exported["count"].as_u64() {
            Some(count @ 0..=2) => count.to_string(),
            Some(_) => ">2".into(),
            None => "non-unsigned-integer".into(),
        };
        return fail(format!(
            "{error} expected_count=2 actual_count={count} ok_matches={} file_matches={} format_matches={} field_count_matches={}",
            exported["ok"] == expected["ok"],
            exported["file"] == expected["file"],
            exported["format"] == expected["format"],
            exported.as_object().is_some_and(|fields| fields.len() == 4),
        ));
    }
    Ok(())
}

pub fn baseline(
    parent: &Path,
    engine: &str,
    mut execute: impl FnMut(&Path, &[String]) -> Result<crate::acceptance::Captured>,
) -> Result<Baseline> {
    let root = parent.join("baseline");
    let mut command = |args: Vec<String>| -> Result<Value> {
        let result = execute(&root, &args)?;
        if result.exit != 0 || !result.stderr.is_empty() {
            let allow = [
                "ValueError",
                "RuntimeError",
                "OSError",
                "FileNotFoundError",
                "PermissionError",
                "TimeoutError",
                "Error",
                "TargetClosedError",
                "ImportError",
                "ModuleNotFoundError",
                "NativeSdkError",
            ];
            let kind = result
                .stderr
                .lines()
                .take(512)
                .filter_map(|l| l.split_once(':').map(|(kind, _)| kind))
                .filter(|kind| allow.contains(kind))
                .last()
                .unwrap_or("unknown");
            let operation = if args.first().is_some_and(|a| a == "cookies") {
                format!("cookies-{}", args[1])
            } else {
                "create".into()
            };
            return fail(format!(
                "Native checkpoint browser baseline failed operation={operation} exit={} type={kind} stdout_bytes={} stderr_bytes={}",
                result.exit,
                result.stdout.len(),
                result.stderr.len()
            ));
        }
        super::super::decode(result.stdout.as_bytes())
    };
    let created = command(vec![
        "create".into(),
        "Browser benchmark fixture".into(),
        "--seed".into(),
        "42".into(),
        "--engine".into(),
        engine.into(),
        "--json".into(),
    ])?;
    let id = created["profile"]["id"]
        .as_str()
        .filter(|v| {
            v.len() == 12
                && v.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        })
        .ok_or("Native checkpoint profile identity invalid")?
        .to_owned();
    let expiry = fixture_expiry(
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)?
            .as_secs(),
    )?;
    // The checkpoint's CLI normalizes epoch values to floats before export.
    // Seed that canonical type rather than relaxing the strict comparison.
    let cookie = json!({"name":"persistent-fixture","value":"synthetic","domain":".fixture.test","path":"/","expires":expiry as f64,"secure":false,"httpOnly":true,"sameSite":"Lax"});
    let mut second = cookie.clone();
    second["name"] = json!("second-fixture");
    second["httpOnly"] = json!(false);
    let cookies = json!([cookie, second]);
    let input = parent.join("cookie input.json");
    fs::write(&input, serde_json::to_vec(&json!({"cookies":cookies}))?)?;
    let seeded = command(vec![
        "cookies".into(),
        "import".into(),
        id.clone(),
        input.to_string_lossy().into_owned(),
        "--clear".into(),
        "--json".into(),
    ])?;
    crate::acceptance::require_equal(
        &seeded,
        &json!({"ok":true,"imported":2,"cleared":true}),
        "Native checkpoint cookie seeding",
    )?;
    let destination = parent.join("baseline cookies.json");
    let exported = command(vec![
        "cookies".into(),
        "export".into(),
        id.clone(),
        destination.to_string_lossy().into_owned(),
        "--format".into(),
        "json".into(),
        "--json".into(),
    ])?;
    require_export_metadata(&exported, &destination, "Native checkpoint export metadata")?;
    let payload = super::super::decode(&fs::read(destination)?)?;
    let mut observed = payload["cookies"]
        .as_array()
        .cloned()
        .ok_or("Native checkpoint cookies missing")?;
    observed.sort_by_key(|row| row["name"].as_str().unwrap_or("").to_owned());
    require_cookie_persistence(&Value::Array(observed), &cookies)?;
    if payload["origins"] != json!([]) {
        return fail("Native checkpoint cookie persistence origins differ");
    }
    let marker = root.join("user-data").join(&id).join("sentinel.bin");
    fs::create_dir_all(marker.parent().ok_or("Session parent missing")?)?;
    fs::write(marker, SENTINEL)?;
    Ok(Baseline { root, id, payload })
}
