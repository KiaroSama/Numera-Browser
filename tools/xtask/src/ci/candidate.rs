//! Candidate isolation, exact compiler evidence and safe retained diagnostics.
use super::{Result, decode, fail, regular};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};
pub const BINARY: &str = "numera-native-candidate";
/// Cargo emits ordinary Windows paths while fs::canonicalize may return the
/// verbatim extended prefix. Compare existing source identities using the OS.
pub fn same_path(left: &Path, right: &Path) -> bool {
    match (left.canonicalize(), right.canonicalize()) {
        (Ok(left), Ok(right)) => left == right,
        _ => left == right,
    }
}
pub fn within_path(path: &Path, root: &Path) -> bool {
    match (path.canonicalize(), root.canonicalize()) {
        (Ok(path), Ok(root)) => path.starts_with(root),
        _ => path.starts_with(root),
    }
}
pub fn targets(metadata: &Value, root: &Path) -> Result<()> {
    let packages = metadata["packages"]
        .as_array()
        .ok_or("Candidate package metadata missing")?;
    let package = packages
        .iter()
        .find(|package| package["name"] == "numera-core")
        .ok_or("Candidate package missing")?;
    let defaults = package["features"]["default"]
        .as_array()
        .ok_or("Candidate default features missing")?
        .iter()
        .filter_map(Value::as_str)
        .collect::<BTreeSet<_>>();
    if !defaults.is_empty() && defaults != BTreeSet::from(["native-product", "native-browser"]) {
        return fail("Candidate acceptance must remain isolated from default application builds");
    }
    let expected = [
        (BINARY, "examples/native_candidate.rs", "native-candidate"),
        (
            "numera-browser-runtime",
            "src/bin/browser_runtime.rs",
            "native-browser",
        ),
        ("numera-engine", "src/bin/engine.rs", "native-product"),
        (
            "numera-http-observer",
            "src/bin/http_observer.rs",
            "native-product",
        ),
    ];
    let mut seen = BTreeSet::new();
    for target in package["targets"]
        .as_array()
        .ok_or("Candidate targets missing")?
    {
        if !target["kind"]
            .as_array()
            .is_some_and(|kinds| kinds.iter().any(|kind| kind == "bin"))
        {
            continue;
        }
        let name = target["name"]
            .as_str()
            .ok_or("Candidate binary name missing")?;
        let (_, source, feature) = expected
            .iter()
            .find(|(expected, _, _)| *expected == name)
            .ok_or("Unexpected candidate binary")?;
        if !seen.insert(name)
            || target["required-features"] != serde_json::json!([feature])
            || target["test"] != false
            || !target["src_path"].as_str().is_some_and(|path| {
                same_path(Path::new(path), &root.join("engine-rust").join(source))
            })
        {
            return fail(
                "Candidate target must remain explicit, source-bound and outside default tests",
            );
        }
    }
    if seen != expected.iter().map(|(name, _, _)| *name).collect() {
        return fail("Candidate/application binary registration is incomplete");
    }
    Ok(())
}
pub fn artifact(root: &Path, report: &[u8], release: bool) -> Result<PathBuf> {
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .map(|path| {
            if path.is_absolute() {
                path
            } else {
                root.join(path)
            }
        })
        .unwrap_or_else(|| root.join("engine-rust/target"));
    artifact_in_target(root, report, release, &target)
}
/// Explicit Cargo target identity also supports scoped test fixtures without
/// mutating the environment of a parallel native test harness.
pub fn artifact_in_target(
    root: &Path,
    report: &[u8],
    release: bool,
    target: &Path,
) -> Result<PathBuf> {
    let mut found = Vec::new();
    let mut finished = None;
    for line in report
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        let value = decode(line)?;
        if value["reason"] == "build-finished" {
            if finished.is_some() {
                return fail("Duplicate candidate compiler completion");
            }
            finished = value["success"].as_bool();
        }
        if value["reason"] == "compiler-artifact" && value["target"]["name"] == BINARY {
            let path = PathBuf::from(
                value["executable"]
                    .as_str()
                    .ok_or("Candidate executable missing")?,
            );
            if !value["manifest_path"].as_str().is_some_and(|path| {
                same_path(Path::new(path), &root.join("engine-rust/Cargo.toml"))
            }) || !value["target"]["kind"]
                .as_array()
                .is_some_and(|kinds| kinds.iter().any(|kind| kind == "bin"))
                || !value["features"].as_array().is_some_and(|features| {
                    features.iter().any(|feature| feature == "native-candidate")
                })
                || value["profile"]["test"] != false
                || !within_path(&path, target)
            {
                return fail("Candidate artifact is not the enabled current source executable");
            }
            if release
                && (value["profile"]["opt_level"]
                    .as_str()
                    .is_none_or(|level| !["1", "2", "3", "s", "z"].contains(&level))
                    || value["profile"]["debug_assertions"] != false)
            {
                return fail("Candidate release artifact must have its current optimized profile");
            }
            found.push(path);
        }
    }
    if finished != Some(true) || found.len() != 1 {
        return fail("Candidate compilation did not finish with exactly one executable");
    }
    Ok(found.remove(0))
}
/// Resolve the effective parent cache before isolating HOME. Discovery is
/// read-only; this never creates a cache or obtains an SDK/browser implicitly.
pub fn bind_sdk_cache(
    environment: &mut BTreeMap<String, String>,
    engine: &str,
    platform: &str,
    resolve: impl FnOnce() -> Result<PathBuf>,
) -> Result<()> {
    if platform != "win32" || engine != "camoufox" {
        return Ok(());
    }
    let root = resolve()?;
    if !root.is_absolute() || !root.is_dir() {
        return fail("Camoufox cache root is unavailable");
    }
    regular(&root, true)?;
    let root = root
        .to_str()
        .filter(|root| !root.is_empty())
        .ok_or("Camoufox cache root is invalid")?;
    environment.insert("WIN_PD_OVERRIDE_LOCAL_APPDATA".into(), root.into());
    Ok(())
}
pub fn effective_windows_cache(environment: &BTreeMap<String, String>) -> Result<PathBuf> {
    for key in ["WIN_PD_OVERRIDE_LOCAL_APPDATA", "LOCALAPPDATA"] {
        if let Some(path) = environment.get(key) {
            let path = PathBuf::from(path);
            if path.is_absolute() && path.is_dir() {
                return Ok(path);
            }
        }
    }
    fail("Camoufox effective parent cache root is unavailable")
}
pub fn diagnostics(logs: &Path) -> Result<Vec<String>> {
    diagnostics_checked(logs, |path| {
        regular(path, false)?;
        Ok(())
    })
}
pub fn diagnostics_checked(
    logs: &Path,
    mut check: impl FnMut(&Path) -> Result<()>,
) -> Result<Vec<String>> {
    if !logs.try_exists()? {
        return Ok(Vec::new());
    }
    let timing = regex::Regex::new(
        r"^(?:SDK timing operation=(?:open|cleanup)|Native timing phase=(?:startup|prepare|metadata|execute|shutdown|browser_spawn|browser_hello|browser_open|browser_cookies_read|browser_close|browser_settle)) elapsed_ns=([0-9]{1,20})$",
    )?;
    let failure = regex::Regex::new(
        r"^Operation failed operation=(open|cleanup) type=([A-Za-z_][A-Za-z0-9_]*)$",
    )?;
    let allowed = [
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
    let mut paths = fs::read_dir(logs)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.sort();
    let mut events = Vec::new();
    let mut omitted = 0;
    for path in paths {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if !["browser-adapter_", "native-cli_", "native-browser_"]
            .iter()
            .any(|prefix| name.starts_with(prefix))
            || !name.ends_with(".log")
            || check(&path).is_err()
            || fs::metadata(&path)?.len() > 65536
        {
            omitted += 1;
            continue;
        }
        for line in fs::read_to_string(path)?.lines() {
            if events.len() >= 64 {
                break;
            }
            let Some((_, message)) = line.rsplit_once("] ") else {
                continue;
            };
            if ["Camoufox SDK import complete", "Startup"].contains(&message) {
                events.push(message.into());
            } else if let Some(captured) = timing.captures(message) {
                if captured[1].parse::<u64>().is_ok() {
                    events.push(message.into());
                }
            } else if let Some(captured) = failure.captures(message) {
                let kind = if allowed.contains(&&captured[2]) {
                    &captured[2]
                } else {
                    "unknown"
                };
                events.push(format!(
                    "Operation failed operation={} type={kind}",
                    &captured[1]
                ));
            }
        }
    }
    events.insert(
        0,
        format!(
            "Candidate failure diagnostics events={} omitted={omitted}",
            events.len()
        ),
    );
    Ok(events)
}
