use numera_core::storage::config::ConfigStore;
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

mod common;
#[path = "config_parity/extension_admission.rs"]
mod extension_admission;
#[path = "native_storage_characterization/support.rs"]
mod frozen;
#[path = "config_parity/strict_reads.rs"]
mod strict_reads;

#[test]
fn fallible_settings_reads_preserve_original_utf8_refusal_and_cache()
-> Result<(), Box<dyn std::error::Error>> {
    use std::time::Duration;

    let temporary = common::TempDirectory::new("fallible-settings")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/characterization/fallible_settings_reads_preserve_original_utf8_refusal_and_cache.json"
    ))?;
    assert_eq!(cases.len(), 20);
    for operation in ["extensions", "empty-update"] {
        let root = temporary.path().join(operation);
        fs::create_dir(&root)?;
        let path = root.join("config.json");
        let store = ConfigStore::new(root);
        for case in cases.iter().filter(|case| case["operation"] == operation) {
            let raw: Option<Vec<u8>> = serde_json::from_value(case["raw"].clone())?;
            if let Some(raw) = &raw {
                fs::write(&path, raw)?;
                let stamp = UNIX_EPOCH
                    + Duration::from_secs(1_800_000_000)
                    + Duration::from_nanos(case["tick"].as_u64().ok_or("Tick missing")? * 100);
                fs::OpenOptions::new()
                    .write(true)
                    .open(&path)?
                    .set_modified(stamp)?;
                assert_eq!(
                    fs::metadata(&path)?.modified()?,
                    stamp,
                    "Fixture timestamp was not retained"
                );
            } else {
                fs::remove_file(&path)?;
            }
            let result = if operation == "extensions" {
                store.default_extensions().map(Value::Array)
            } else {
                store.set_many(serde_json::Map::new()).map(Value::Object)
            };
            assert_eq!(
                result.is_ok(),
                case["result"]["ok"],
                "Fallible settings read admission differs: {operation} tick={}",
                case["tick"]
            );
            match result {
                Ok(value) => assert_eq!(
                    value, case["result"]["value"],
                    "Fallible settings cached value differs"
                ),
                Err(error) => assert!(
                    matches!(error,
                    numera_core::storage::config::ConfigError::Io(ref e)
                        if e.kind() == std::io::ErrorKind::InvalidData),
                    "Malformed settings did not report InvalidData"
                ),
            }
            assert_eq!(fs::read(&path).ok(), raw, "Settings read changed evidence");
        }
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn settings_instance_cache_matches_original_file_identity_and_detached_results()
-> Result<(), Box<dyn std::error::Error>> {
    use std::time::Duration;

    let temporary = common::TempDirectory::new("settings-instance-cache")?;
    // Immutable original observation; native operations and full assertions remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/native-storage-characterization/settings_instance_cache_matches_original_file_identity_and_detached_results.json"
    ))?;
    assert_eq!(cases.len(), 11);
    let root = temporary.path().join("native");
    fs::create_dir(&root)?;
    let path = root.join("config.json");
    let store = ConfigStore::new(root.clone());
    for (index, case) in cases.iter().enumerate() {
        let raw: Option<Vec<u8>> = serde_json::from_value(case["raw"].clone())?;
        if let Some(raw) = &raw {
            fs::write(&path, raw)?;
            let stamp = UNIX_EPOCH
                + Duration::from_secs(1_800_000_000)
                + Duration::from_nanos(case["tick"].as_u64().ok_or("Tick missing")? * 100);
            fs::OpenOptions::new()
                .write(true)
                .open(&path)?
                .set_modified(stamp)?;
            assert_eq!(
                fs::metadata(&path)?.modified()?,
                stamp,
                "Fixture timestamp was not retained"
            );
        } else {
            fs::remove_file(&path)?;
        }
        let mut first = store.read()?;
        assert_eq!(
            Value::Object(first.clone()),
            case["first"],
            "Settings cache result differs at step {index}"
        );
        if let Some(Value::Object(nested)) = first.get_mut("nested") {
            nested.insert("value".into(), "changed-return".into());
        }
        first.insert("injected".into(), serde_json::json!(["changed-return"]));
        assert_eq!(
            Value::Object(store.read()?),
            case["again"],
            "Settings cache detached result differs at step {index}"
        );
        assert_eq!(
            Value::Object(ConfigStore::new(root.clone()).read()?),
            case["independent"],
            "Independent settings instance differs at step {index}"
        );
        assert_eq!(fs::read(&path).ok(), raw, "Settings read changed evidence");
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn direct_settings_and_current_thread_replacement_match_original()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::storage::locks::DataLock;
    use serde_json::json;
    use std::{sync::mpsc, thread, time::Duration};

    let temporary = common::TempDirectory::new("direct-settings")?;
    let extension = temporary.path().join("extension");
    fs::create_dir(&extension)?;
    fs::write(extension.join("manifest.json"), b"{}")?;
    let cases = frozen::load(
        include_str!(
            "fixtures/native-storage-characterization/direct_settings_and_current_thread_replacement_match_original.json"
        ),
        "/owned-fixture",
        temporary.path(),
    )?;
    assert_eq!(cases.len(), 71, "Direct settings observations missing");
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join("native").join(index.to_string());
        fs::create_dir_all(&root)?;
        let path = root.join("config.json");
        let before = case["input"]
            .as_str()
            .map(|raw| {
                (0..raw.len())
                    .step_by(2)
                    .map(|offset| u8::from_str_radix(&raw[offset..offset + 2], 16))
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?;
        if let Some(bytes) = &before {
            fs::write(&path, bytes)?;
        }
        let store = ConfigStore::new(root.clone());
        let lock = DataLock::transaction(&root)?;
        let other = DataLock::transaction(&root.join("other"))?;
        let mode = case["mode"].as_str().ok_or("Settings lock mode missing")?;
        let guard = match mode {
            "owned" | "reentrant" => Some(lock.acquire(Duration::from_secs(5))?),
            "other-root" => Some(other.acquire(Duration::from_secs(5))?),
            "released" => {
                drop(lock.acquire(Duration::from_secs(5))?);
                None
            }
            _ => None,
        };
        if mode == "reentrant" {
            drop(lock.acquire(Duration::from_secs(5))?);
        }
        let invoke = || -> Result<Value, numera_core::storage::config::ConfigError> {
            match case["op"].as_str().unwrap_or("") {
                "set" => store.set("selected", case["value"].clone()),
                "engine" => store.set_default_engine(case["value"].clone()),
                "headless" => store.set_headless_launch(&case["value"]).map(Value::Bool),
                "headless-read" => store.headless_launch().map(Value::Bool),
                "extensions" => store
                    .set_default_extensions(
                        case["value"]
                            .as_array()
                            .expect("String extension list")
                            .iter()
                            .map(|v| v.as_str().expect("String extension entry").to_owned())
                            .collect(),
                    )
                    .map(|v| json!(v)),
                _ => store
                    .replace_locked(
                        case["value"]
                            .as_object()
                            .expect("Settings replacement object")
                            .clone(),
                    )
                    .map(Value::Object),
            }
        };
        let result = thread::scope(|scope| -> Result<_, Box<dyn std::error::Error>> {
            let (ready_send, ready) = mpsc::sync_channel(1);
            let (release, release_recv) = mpsc::sync_channel(1);
            let worker = if mode == "other-thread" {
                let root = &root;
                Some(scope.spawn(move || -> Result<(), String> {
                    let lock = DataLock::transaction(root).map_err(|e| e.to_string())?;
                    let _guard = lock
                        .acquire(Duration::from_secs(5))
                        .map_err(|e| e.to_string())?;
                    ready_send.send(()).map_err(|e| e.to_string())?;
                    release_recv
                        .recv_timeout(Duration::from_secs(5))
                        .map_err(|e| e.to_string())?;
                    Ok(())
                }))
            } else {
                None
            };
            if worker.is_some() {
                ready.recv_timeout(Duration::from_secs(5))?;
            }
            let held = lock.owned_by_current_thread()?;
            let result = invoke();
            if let Some(worker) = worker {
                release.send(())?;
                worker
                    .join()
                    .map_err(|_| "Native settings owner panicked")?
                    .map_err(std::io::Error::other)?;
            }
            assert_eq!(
                json!(held),
                case["held"],
                "Settings ownership differs: {index}"
            );
            Ok(result)
        })?;
        drop(guard);
        assert_eq!(json!(lock.owned_by_current_thread()?), case["released"]);
        match result {
            Ok(value) => {
                assert!(case["error"].is_null(), "Settings refusal differs: {index}");
                assert_eq!(
                    value, case["result"],
                    "Direct settings result differs: {index}"
                );
            }
            Err(numera_core::storage::config::ConfigError::Io(error))
                if error.kind() == std::io::ErrorKind::InvalidData =>
            {
                assert_eq!(case["error"], "UnicodeDecodeError");
            }
            Err(error) => assert_eq!(
                Some(error.source_kind()),
                case["error"].as_str(),
                "Direct settings error differs: {index}"
            ),
        }
        let after = fs::read(&path).ok();
        assert_eq!(
            json!(after == before),
            case["unchanged"],
            "Settings bytes differ: {index}"
        );
        if !case["error"].is_null() {
            assert_eq!(after, before);
        }
        let disk = after
            .as_deref()
            .and_then(|bytes| serde_json::from_slice::<Value>(bytes).ok())
            .unwrap_or(Value::Null);
        assert_eq!(
            disk, case["disk"],
            "Direct settings disk state differs: {index}"
        );
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn malformed_settings_text_refuses_without_replacing_source_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::storage::config::ConfigError;

    let temporary = common::TempDirectory::new("config-text")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/characterization/malformed_settings_text_refuses_without_replacing_source_bytes.json"
    ))?;
    assert_eq!(cases.len(), 18, "Settings text observations missing");
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join("native").join(index.to_string());
        fs::create_dir_all(&root)?;
        let path = root.join("config.json");
        let before = case["input"]
            .as_str()
            .map(|raw| {
                (0..raw.len())
                    .step_by(2)
                    .map(|offset| u8::from_str_radix(&raw[offset..offset + 2], 16))
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?;
        if let Some(bytes) = &before {
            fs::write(&path, bytes)?;
        }
        let store = ConfigStore::new(root);
        let result = store.set_many(
            case["changes"]
                .as_object()
                .ok_or("Changes missing")?
                .clone(),
        );
        assert_eq!(
            result.is_ok(),
            case["error"].is_null(),
            "Settings text mutation admission differs: case={index}"
        );
        match result {
            Ok(value) => assert_eq!(Value::Object(value), case["result"]),
            Err(ConfigError::Io(error)) => {
                assert_eq!(case["error"], "UnicodeDecodeError");
                assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
                assert!(error.to_string().contains("UTF-8"));
            }
            Err(error) => assert_eq!(Some(error.source_kind()), case["error"].as_str()),
        }
        let after = fs::read(&path).ok();
        assert_eq!(
            Value::Bool(before == after),
            case["unchanged"],
            "Settings text byte preservation differs: case={index}"
        );
        if !case["error"].is_null() {
            assert_eq!(after, before, "Refused settings mutation changed bytes");
        }
        let written = after
            .as_deref()
            .and_then(|bytes| serde_json::from_slice::<Value>(bytes).ok())
            .unwrap_or(Value::Null);
        assert_eq!(
            written, case["written"],
            "Settings text state differs: case={index}"
        );
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn settings_read_merge_and_refusal_match_source_without_losing_unknown_keys()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/config.json"))?;
    let cases = fixture["cases"].as_array().ok_or("Missing config cases")?;
    assert!(!cases.is_empty());
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-temp")
        .join(format!(
            "config-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    fs::create_dir_all(&root)?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        for case in cases {
            let path = root.join("config.json");
            if path.exists() {
                fs::remove_file(&path)?;
            }
            if let Some(raw) = case["input"].as_str() {
                fs::write(&path, raw)?;
            }
            let before = fs::read(&path).ok();
            let store = ConfigStore::new(root.clone());
            assert_eq!(Value::Object(store.read()?), case["read"]);
            let changes = case["changes"].as_object().ok_or("Missing changes")?;
            match store.set_many(changes.clone()) {
                Ok(value) => {
                    assert!(case["error_type"].is_null());
                    assert_eq!(Value::Object(value), case["result"]);
                }
                Err(error) => assert_eq!(Some(error.source_kind()), case["error_type"].as_str()),
            }
            let after = fs::read(&path).ok();
            assert_eq!(
                before == after,
                case["unchanged"]
                    .as_bool()
                    .ok_or("Missing side effect assertion")?
            );
            if case["error_type"].is_null() {
                let disk = after
                    .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok())
                    .unwrap_or(Value::Null);
                assert_eq!(disk, case["disk"]);
            }
        }
        Ok(())
    })();
    fs::remove_dir_all(root)?;
    result
}
