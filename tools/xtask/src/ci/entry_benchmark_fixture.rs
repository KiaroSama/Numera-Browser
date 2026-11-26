//! Build synthetic fixtures and verify the measured workload did not corrupt them.
use super::super::{Result, fail, migration};
use super::{FIRST, OUTPUT_LIMIT, PASSWORD, SECRET, SENTINEL, Scenario, browser};
use crate::acceptance::{self, Captured};
use numera_core::storage::locks::DataLock;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};
fn linked(meta: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        meta.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        meta.file_type().is_symlink()
    }
}
/// Extend the shared migration snapshot with the source's lock/session bytes.
/// No lock files or session claims are silently discarded as measurement noise.
pub fn protected_bytes(root: &Path) -> Result<BTreeMap<String, Vec<u8>>> {
    fn walk(
        root: &Path,
        path: &Path,
        out: &mut BTreeMap<String, Vec<u8>>,
        depth: usize,
    ) -> Result<()> {
        if depth > 128 || out.len() > 100_000 {
            return fail("Entry benchmark protected input exceeds traversal bound");
        }
        let meta = match fs::symlink_metadata(path) {
            Ok(meta) => meta,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        if linked(&meta) {
            return fail("Entry benchmark contains a linked protected path");
        }
        if meta.is_dir() {
            for entry in fs::read_dir(path)? {
                walk(root, &entry?.path(), out, depth + 1)?;
            }
        } else if meta.is_file() {
            out.insert(
                path.strip_prefix(root)?
                    .to_string_lossy()
                    .replace('\\', "/"),
                fs::read(path)?,
            );
        } else {
            return fail("Entry benchmark protected input is not regular");
        }
        Ok(())
    }
    // Inspect before the shared helper follows any user-data path.
    let mut complete = BTreeMap::new();
    for name in [
        "config.json",
        "dashboard-config.json",
        "proxies.json",
        "profiles",
        "dashboard",
        "sessions",
        "user-data",
    ] {
        walk(root, &root.join(name), &mut complete, 0)?;
    }
    for (name, bytes) in migration::protected(root)? {
        if complete.get(&name) != Some(&bytes) {
            return fail("Entry benchmark protected snapshot changed while observed");
        }
    }
    Ok(complete)
}

pub struct Baseline {
    pub plain: PathBuf,
    pub encrypted: PathBuf,
    pub profile: Value,
}
/// Seed with the checkpoint's actual create/vault commands; only the source's
/// deterministic32-record expansion is performed by the runner itself.
pub fn baseline(
    parent: &Path,
    mut execute: impl FnMut(&Path, &[OsString], Option<&str>) -> Result<Captured>,
) -> Result<Baseline> {
    let plain = parent.join("baseline");
    let args = [
        "create",
        "Benchmark fixture",
        "--seed",
        "42",
        "--engine",
        "playwright",
        "--proxy",
        "http://fixture:synthetic-benchmark-value@127.0.0.1:8080",
        "--json",
    ]
    .map(OsString::from);
    let output = execute(&plain, &args, None)?;
    let created = checked_output(output, "baseline create")?;
    if created["ok"] != true {
        return fail("Entry benchmark baseline creation did not succeed");
    }
    let profile = &created["profile"];
    let id = profile_id(profile)?;
    for directory in ["profiles", "dashboard"] {
        let source = plain.join(directory).join(format!("{id}.json"));
        let raw = super::super::decode(&fs::read(&source)?)?;
        if directory == "profiles" {
            acceptance::require_equal(&raw, profile, "Entry baseline persisted profile")?;
            if raw["proxy"]["password"] != SECRET {
                return fail("Entry benchmark baseline lost its synthetic credential");
            }
        } else if raw["id"] != id || raw["name"] != profile["name"] {
            return fail("Entry benchmark baseline dashboard identity differs");
        }
        for index in 0..32 {
            let mut value = raw
                .as_object()
                .ok_or("Entry baseline record is not an object")?
                .clone();
            value.insert("id".into(), json!(format!("{index:012x}")));
            value.insert("name".into(), json!(format!("Benchmark {index:02}")));
            fs::write(
                source
                    .parent()
                    .ok_or("Baseline parent missing")?
                    .join(format!("{index:012x}.json")),
                serde_json::to_vec(&value)?,
            )?;
        }
        if !(0..32).any(|index| id == format!("{index:012x}")) {
            fs::remove_file(source)?;
        }
    }
    let marker = plain
        .join("user-data")
        .join(FIRST)
        .join("Default/synthetic.bin");
    fs::create_dir_all(marker.parent().ok_or("Baseline session parent missing")?)?;
    fs::write(marker, SENTINEL)?;
    let encrypted = parent.join("vault baseline");
    browser::copy_tree(&plain, &encrypted)?;
    let output = execute(
        &encrypted,
        &["vault".into(), "enable".into(), "--json".into()],
        Some(PASSWORD),
    )?;
    acceptance::require_equal(
        &checked_output(output, "baseline vault enable")?,
        &json!({"ok":true,"enabled":true,"encrypted":64}),
        "Entry benchmark encrypted baseline",
    )?;
    let profile = super::super::decode(&fs::read(
        plain.join("profiles").join(format!("{FIRST}.json")),
    )?)?;
    Ok(Baseline {
        plain,
        encrypted,
        profile,
    })
}
pub(super) fn checked_output(output: Captured, operation: &str) -> Result<Value> {
    if output.exit != 0 || !output.stderr.is_empty() || output.stdout.len() > OUTPUT_LIMIT {
        return fail(format!(
            "Entry benchmark refused operation={operation} exit={} stdout_bytes={} stderr_bytes={}",
            output.exit,
            output.stdout.len(),
            output.stderr.len()
        ));
    }
    super::super::decode(output.stdout.as_bytes())
}
fn profile_id(profile: &Value) -> Result<&str> {
    profile["id"]
        .as_str()
        .filter(|id| {
            id.len() == 12
                && id
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
        })
        .ok_or_else(|| io::Error::other("Entry benchmark profile identity is invalid").into())
}
fn require_ids(payload: &Value) -> Result<()> {
    let rows = payload
        .as_array()
        .ok_or("Entry benchmark profile listing is not an array")?;
    let expected = (0..32)
        .map(|index| format!("{index:012x}"))
        .collect::<BTreeSet<_>>();
    let mut ids = BTreeSet::new();
    for row in rows {
        let id = row["id"]
            .as_str()
            .ok_or("Entry benchmark listed identity is not text")?;
        if !row.is_object() || !ids.insert(id.to_owned()) {
            return fail("Entry benchmark listing duplicated a profile");
        }
    }
    if rows.len() != 32 || ids != expected {
        return fail("Entry benchmark listing omitted a baseline profile");
    }
    Ok(())
}
fn creation(root: &Path, before: &BTreeMap<String, Vec<u8>>, payload: &Value) -> Result<Value> {
    if payload["ok"] != true {
        return fail("Entry benchmark measured creation did not succeed");
    }
    let profile = &payload["profile"];
    let id = numera_core::storage::paths::safe_id(
        profile["id"]
            .as_str()
            .ok_or("Entry benchmark creation identity is not text")?,
    )?;
    let stored =
        super::super::decode(&fs::read(root.join("profiles").join(format!("{id}.json")))?)?;
    migration::require_profile(&stored, profile)?;
    acceptance::require_equal(&stored, profile, "Entry measured creation persistence")?;
    let previous = before
        .get("config.json")
        .map(|bytes| super::super::decode(bytes))
        .transpose()?
        .unwrap_or(json!({}));
    let mut settings = previous
        .as_object()
        .ok_or("Entry benchmark previous config is not an object")?
        .clone();
    settings.insert("default_engine".into(), json!("playwright"));
    acceptance::require_equal(
        &super::super::decode(&fs::read(root.join("config.json"))?)?,
        &Value::Object(settings),
        "Entry benchmark requested settings",
    )?;
    let mut after = protected_bytes(root)?;
    match before.get("config.json") {
        Some(bytes) => {
            after.insert("config.json".into(), bytes.clone());
        }
        None => {
            after.remove("config.json");
        }
    }
    for directory in ["profiles", "dashboard"] {
        let name = format!("{directory}/{id}.json");
        if before.contains_key(&name) || after.remove(&name).is_none() {
            return fail("Entry benchmark creation overwrote or omitted a profile copy");
        }
        let record = root.join(&name);
        let lock = DataLock::for_file(&record)?;
        // DataLock hashes the source-compatible canonical spelling. On Windows
        // it removes the verbatim prefix that canonicalize adds to `root`.
        // The parent location is fixed here; only its actual lock name is used.
        let lock_name = lock
            .path()
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or("Entry creation lock filename is not text")?;
        let relative = format!("{directory}/.locks/{lock_name}");
        if !before.contains_key(&relative) && after.get(&relative).is_some_and(Vec::is_empty) {
            after.remove(&relative);
        }
    }
    if after != *before {
        return fail("Entry benchmark creation changed an unrelated protected record");
    }
    let mut dashboard = super::super::decode(&fs::read(
        root.join("dashboard").join(format!("{id}.json")),
    )?)?
    .as_object()
    .ok_or("Entry benchmark dashboard is not an object")?
    .clone();
    if dashboard.get("id") != profile.get("id") {
        return fail("Entry benchmark creation dashboard identity differs");
    }
    dashboard.remove("id");
    dashboard.remove("_created");
    let mut profile = profile
        .as_object()
        .ok_or("Created profile is not an object")?
        .clone();
    for key in ["id", "created_at", "updated_at"] {
        profile.remove(key);
    }
    let mut normalized = payload
        .as_object()
        .ok_or("Creation payload is not an object")?
        .clone();
    normalized.insert("profile".into(), Value::Object(profile));
    normalized.insert("dashboard".into(), Value::Object(dashboard));
    Ok(Value::Object(normalized))
}
fn backup(path: &Path, payload: &Value, expected: &Value) -> Result<Value> {
    acceptance::require_equal(
        payload,
        &json!({"ok":true,"id":FIRST,"name":"Benchmark 00","file":path}),
        "Entry benchmark backup verdict",
    )?;
    let mut zip = zip::ZipArchive::new(fs::File::open(path)?)?;
    let mut members = BTreeMap::new();
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index)?;
        if entry.is_dir() {
            continue;
        }
        if entry.size() > 1_048_576 || members.len() >= 2 {
            return fail("Entry benchmark backup contains unexpected session data");
        }
        let name = entry.name()?.into_owned();
        let mut bytes = Vec::new();
        entry.by_ref().take(1_048_577).read_to_end(&mut bytes)?;
        if bytes.len() > 1_048_576 || members.insert(name, bytes).is_some() {
            return fail("Entry benchmark backup contains duplicate or oversized data");
        }
    }
    let profile = super::super::decode(
        &members
            .remove("profile.json")
            .ok_or("Entry benchmark backup profile missing")?,
    )?;
    acceptance::require_equal(&profile, expected, "Entry benchmark backed up profile")?;
    if members != BTreeMap::from([("user-data/Default/synthetic.bin".into(), SENTINEL.to_vec())]) {
        return fail("Entry benchmark backup lost or added session data");
    }
    Ok(profile)
}
/// Production mutation/parity validation is also callable by existing negative
/// controls, without replacing the measured command or manufacturing evidence.
pub fn observation(
    template: &Path,
    workspace: &Path,
    scenario: Scenario,
    expected: &Value,
    execute: impl FnOnce(&Path, &Path) -> Result<browser::Sample>,
) -> Result<(browser::Sample, Value)> {
    protected_bytes(template)?;
    let root = workspace.join("data with spaces");
    browser::copy_tree(template, &root)?;
    let root = root.canonicalize()?;
    let before = protected_bytes(&root)?;
    let destination = workspace.join("backup.zip");
    let sample = execute(&root, &destination)?;
    if sample.elapsed_ns == 0 || sample.bytes == 0 || sample.metric.is_empty() {
        return fail("Entry benchmark sample is not a positive lifetime observation");
    }
    let payload = if scenario == Scenario::Create {
        creation(&root, &before, &sample.payload)?
    } else {
        if protected_bytes(&root)? != before {
            return fail("Read-only entry benchmark changed protected input bytes");
        }
        match scenario {
            Scenario::Backup => backup(&destination, &sample.payload, expected)?,
            Scenario::HttpList => {
                require_ids(&sample.payload)?;
                sample.payload.clone()
            }
            Scenario::List => {
                if sample.payload["total"] != 32 {
                    return fail("Entry benchmark CLI count differs");
                }
                require_ids(&sample.payload["profiles"])?;
                sample.payload.clone()
            }
            Scenario::VaultRead => {
                acceptance::require_equal(&sample.payload, expected, "Entry decrypted profile")?;
                sample.payload.clone()
            }
            Scenario::Create => unreachable!(),
        }
    };
    acceptance::require_equal(&payload, &payload, "Entry benchmark finite payload")?;
    Ok((sample, payload))
}
