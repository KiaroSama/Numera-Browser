use numera_core::storage::repo::ProfileRepo;
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
mod common;

#[test]
fn repository_settings_cache_matches_original_across_secret_reads()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::storage::{dashboard::DashboardStore, pool::ProxyPool};
    use std::time::Duration;

    let temporary = common::TempDirectory::new("repository-settings-cache")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let observed: Value = serde_json::from_str(include_str!(
        "fixtures/characterization/repository_settings_cache_matches_original_across_secret_reads.json"
    ))?;
    let fixture = &observed["fixture"];
    let cases = observed["cases"].as_array().ok_or("Cache cases missing")?;
    assert_eq!(cases.len(), 48);
    for kind in ["status", "profile", "dashboard", "pool"] {
        let root = temporary.path().join("native").join(kind);
        fs::create_dir_all(root.join("profiles"))?;
        fs::create_dir_all(root.join("dashboard"))?;
        let inputs = [
            (
                root.join("profiles").join("fixture00001.json"),
                serde_json::to_vec(&fixture["engine"])?,
            ),
            (
                root.join("dashboard").join("fixture00001.json"),
                serde_json::to_vec(&fixture["dashboard"])?,
            ),
            (
                root.join("proxies.json"),
                serde_json::to_vec(&serde_json::json!([
                {"id":"proxy00001","pass":fixture["dashboard"]["proxy"]["pass"]}]))?,
            ),
        ];
        for (path, bytes) in &inputs {
            fs::write(path, bytes)?;
        }
        let password = fixture["password"]
            .as_str()
            .ok_or("Fixture password missing")?;
        let repo = ProfileRepo::new(root.clone(), Some(password.into()));
        let dashboard = DashboardStore::new(root.clone(), Some(password.into()));
        let pool = ProxyPool::new(root.clone(), Some(password.into()));
        let read = |repo: &ProfileRepo,
                    dashboard: &DashboardStore,
                    pool: &ProxyPool|
         -> Result<Value, Box<dyn std::error::Error>> {
            Ok(match kind {
                "status" => repo.vault()?.unwrap_or(Value::Null),
                "profile" => repo
                    .get("fixture00001")?
                    .ok_or("Profile missing")?
                    .proxy
                    .ok_or("Proxy missing")?
                    .to_record()["password"]
                    .clone(),
                "dashboard" => dashboard.get("fixture00001")?.ok_or("Dashboard missing")?["proxy"]
                    ["pass"]
                    .clone(),
                _ => pool.list()?.first().ok_or("Pool entry missing")?["pass"].clone(),
            })
        };
        let path = root.join("config.json");
        let mut retained_safety_cases = 0;
        for (index, case) in cases.iter().filter(|case| case["kind"] == kind).enumerate() {
            let raw: Option<Vec<u8>> = serde_json::from_value(case["raw"].clone())?;
            if let Some(bytes) = &raw {
                fs::write(&path, bytes)?;
                let stamp = UNIX_EPOCH
                    + Duration::from_secs(1_800_000_000)
                    + Duration::from_nanos(case["tick"].as_u64().ok_or("Tick missing")? * 100);
                fs::OpenOptions::new()
                    .write(true)
                    .open(&path)?
                    .set_modified(stamp)?;
                assert_eq!(fs::metadata(&path)?.modified()?, stamp);
            } else {
                fs::remove_file(&path)?;
            }
            let result = read(&repo, &dashboard, &pool)?;
            let independent = read(
                &ProfileRepo::new(root.clone(), Some(password.into())),
                &DashboardStore::new(root.clone(), Some(password.into())),
                &ProxyPool::new(root.clone(), Some(password.into())),
            )?;
            // Retain the native never-ciphertext safeguard where the original
            // returns stored ciphertext because the vault descriptor is absent.
            let compare = |actual: Value, expected: &Value| {
                if kind == "profile" && expected == &fixture["engine"]["proxy"]["password"] {
                    assert!(actual.is_null(), "Missing vault exposed stored ciphertext");
                    true
                } else {
                    assert_eq!(
                        actual, *expected,
                        "Repository settings cache differs: kind={kind} step={index}"
                    );
                    false
                }
            };
            if compare(result, &case["result"]) {
                retained_safety_cases += 1;
            }
            compare(independent, &case["independent"]);
            assert_eq!(fs::read(&path).ok(), raw, "Cache read changed settings");
            for (path, bytes) in &inputs {
                assert_eq!(fs::read(path)?, *bytes);
            }
        }
        assert_eq!(
            retained_safety_cases,
            if kind == "profile" { 3 } else { 0 },
            "Original/native absent-vault safety differences changed"
        );
        let before = fs::read(&path)?;
        let journal = root.join("vault-migration.json");
        fs::write(&journal, b"{")?;
        assert!(
            read(&repo, &dashboard, &pool).is_err(),
            "Warmed cache bypassed unresolved journal"
        );
        assert_eq!(fs::read(&journal)?, b"{");
        assert_eq!(fs::read(&path)?, before);
        fs::remove_file(journal)?;
    }
    temporary.cleanup()?;
    Ok(())
}

#[path = "repo_parity/settings_errors.rs"]
mod settings_errors;

#[test]
fn public_divergence_reports_all_ordered_fields_without_mutation()
-> Result<(), Box<dyn std::error::Error>> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use numera_core::storage::repo::RepoError;

    let temporary = common::TempDirectory::new("repo-divergence")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/characterization/public_divergence_reports_all_ordered_fields_without_mutation.json"
    ))?;
    assert_eq!(cases.len(), 47, "Divergence observations missing");
    assert_eq!(
        cases
            .iter()
            .filter(|case| case["error"] == "UnicodeDecodeError")
            .count(),
        4,
        "Original divergence text refusals missing"
    );
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        let files = case["files"]
            .as_object()
            .ok_or("Divergence inputs missing")?;
        for (name, bytes) in files {
            if !bytes.is_null() {
                let path = root.join(name);
                fs::create_dir_all(path.parent().ok_or("Input parent missing")?)?;
                let expected = STANDARD.decode(bytes.as_str().ok_or("Input bytes missing")?)?;
                fs::write(&path, &expected)?;
                assert_eq!(fs::read(path)?, expected, "Divergence input bytes differ");
            }
        }
        let repo = ProfileRepo::new(root.clone(), case["password"].as_str().map(str::to_owned));
        let result = repo.divergence("fixture00001", 1700000000.0);
        let invalid = matches!(&result, Err(RepoError::Io(error)) if error.kind() == std::io::ErrorKind::InvalidData);
        assert_eq!(
            invalid,
            case["error"] == "UnicodeDecodeError",
            "Divergence text admission differs: {}",
            case["label"]
        );
        if !invalid {
            assert_eq!(
                serde_json::to_value(result?)?,
                case["result"],
                "Complete ordered divergence differs: {}",
                case["label"]
            );
        }
        for (name, bytes) in files {
            let path = root.join(name);
            if bytes.is_null() {
                assert!(!path.try_exists()?, "Divergence created an absent record");
            } else {
                assert_eq!(
                    fs::read(path)?,
                    STANDARD.decode(bytes.as_str().ok_or("Input bytes missing")?)?,
                    "Divergence rewrote stored bytes: {}",
                    case["label"]
                );
            }
        }
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn dual_store_derivation_preserves_viewport_credentials_and_unchanged_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/repo.json"))?;
    let cases = fixture["cases"]
        .as_array()
        .ok_or("Missing reconciliation cases")?;
    assert!(!cases.is_empty());
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-temp")
        .join(format!(
            "repo-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    fs::create_dir_all(root.join("profiles"))?;
    fs::create_dir_all(root.join("dashboard"))?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        for case in cases {
            let paths = [
                root.join("profiles/fixture00001.json"),
                root.join("dashboard/fixture00001.json"),
            ];
            for (path, key) in paths.iter().zip(["engine", "dashboard"]) {
                if path.exists() {
                    fs::remove_file(path)?;
                }
                if !case[key].is_null() {
                    fs::write(path, serde_json::to_vec(&case[key])?)?;
                }
            }
            let before: Vec<_> = paths.iter().map(fs::read).map(Result::ok).collect();
            fs::write(
                root.join("config.json"),
                serde_json::to_vec(&case["config"])?,
            )?;
            let repo = ProfileRepo::new(root.clone(), case["password"].as_str().map(str::to_owned));
            let out = if case["operation"] == "adopt" {
                repo.adopt("fixture00001", 1700000000.0).map(Value::Bool)
            } else {
                repo.materialize("fixture00001", 1700000000.0).map(|p| {
                    p.map_or(Value::Null, |p| {
                        serde_json::to_value(p).expect("Profile serialization")
                    })
                })
            };
            match out {
                Ok(value) => {
                    assert!(case["error"].is_null());
                    assert_eq!(value, case["result"]);
                }
                Err(error) => assert_eq!(Some(error.to_string().as_str()), case["error"].as_str()),
            }
            for (index, path) in paths.iter().enumerate() {
                let after = fs::read(path).ok();
                assert_eq!(
                    before[index] == after,
                    case["unchanged"][index]
                        .as_bool()
                        .ok_or("Missing byte observation")?
                );
                let disk = after
                    .map(|bytes| serde_json::from_slice::<Value>(&bytes))
                    .transpose()?
                    .unwrap_or(Value::Null);
                assert_eq!(disk, case["written"][index]);
            }
        }
        Ok(())
    })();
    fs::remove_dir_all(root)?;
    result
}
