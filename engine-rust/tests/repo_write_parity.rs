use numera_core::{models::Profile, storage::repo::ProfileRepo};
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};
mod common;

#[test]
fn single_edit_preserves_callback_results_and_atomic_store_decisions()
-> Result<(), Box<dyn std::error::Error>> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use numera_core::storage::repo::RepoError;

    let temporary = common::TempDirectory::new("repo-single-edit")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/characterization/single_edit_preserves_callback_results_and_atomic_store_decisions.json"
    ))?;
    assert_eq!(cases.len(), 28, "Single-edit observations missing");
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        let files = case["files"]
            .as_object()
            .ok_or("Single-edit inputs missing")?;
        for (name, raw) in files {
            let path = root.join(name);
            fs::create_dir_all(path.parent().ok_or("Input parent missing")?)?;
            let bytes = STANDARD.decode(raw.as_str().ok_or("Input bytes missing")?)?;
            fs::write(&path, &bytes)?;
            assert_eq!(fs::read(path)?, bytes, "Single-edit input bytes differ");
        }
        let repo = ProfileRepo::new(root.clone(), case["password"].as_str().map(str::to_owned));
        let mut calls = Vec::new();
        let mode = case["mode"].as_str().ok_or("Edit mode missing")?;
        let result = repo.edit(
            case["reference"].as_str().ok_or("Reference missing")?,
            |profile| {
                calls.push(serde_json::to_value(&*profile).map_err(std::io::Error::other)?);
                match mode {
                    "noop" => return Ok(None),
                    "replacement" => {
                        let mut replacement = profile.clone();
                        replacement.notes = "replacement α 😀".into();
                        return Ok(Some(replacement));
                    }
                    "rename" => profile.name = "renamed α".into(),
                    "clear-tags" => profile.tags.clear(),
                    "replace-secret" => profile
                        .proxy
                        .as_mut()
                        .ok_or_else(|| RepoError::Message("Proxy missing".into()))?
                        .set_password(Some("chosen-fixture-secret".into())),
                    "clear-secret" => profile
                        .proxy
                        .as_mut()
                        .ok_or_else(|| RepoError::Message("Proxy missing".into()))?
                        .set_password(Some(String::new())),
                    "remove-proxy" => profile.proxy = None,
                    _ => profile.notes = "edited 日本語".into(),
                }
                if mode == "callback-error" {
                    return Err(RepoError::Message("Callback refused edit".into()));
                }
                Ok((mode == "notes-return").then(|| profile.clone()))
            },
            1700000000.0,
        );
        assert_eq!(
            serde_json::to_value(calls)?,
            case["calls"],
            "Single-edit callback differs: case={index}"
        );
        match result {
            Ok(saved) => {
                assert!(
                    case["error"].is_null(),
                    "Single-edit refusal missing: case={index}"
                );
                assert_eq!(
                    serde_json::to_value(saved)?,
                    case["result"],
                    "Single-edit result differs: case={index}"
                );
            }
            Err(error) => assert_eq!(
                Some(error.to_string().as_str()),
                case["error"].as_str(),
                "Single-edit refusal differs: case={index}"
            ),
        }
        for (name, raw) in files {
            let before = STANDARD.decode(raw.as_str().ok_or("Input bytes missing")?)?;
            let after = fs::read(root.join(name))?;
            assert_eq!(
                Value::Bool(before == after),
                case["unchanged"][name],
                "Single-edit preservation differs: case={index}"
            );
            assert_eq!(
                serde_json::from_slice::<Value>(&after)?,
                case["written"][name],
                "Single-edit stored state differs: case={index}"
            );
            if case["unchanged"][name] == true {
                assert_eq!(
                    after, before,
                    "Single-edit replaced preserved bytes: case={index}"
                );
            }
        }
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn public_credential_conflict_guards_match_original_without_writes()
-> Result<(), Box<dyn std::error::Error>> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    use numera_core::storage::repo::RepoError;

    let temporary = common::TempDirectory::new("credential-conflict-guards")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/characterization/public_credential_conflict_guards_match_original_without_writes.json"
    ))?;
    assert_eq!(cases.len(), 101);
    for (index, case) in cases.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        let mut files = Vec::new();
        if let Some(raw) = case.get("raw") {
            let path = root.join("profiles/fixture-conflict.json");
            fs::create_dir_all(path.parent().ok_or("Profile parent missing")?)?;
            let bytes: Vec<u8> = serde_json::from_value(raw.clone())?;
            fs::write(&path, &bytes)?;
            files.push((path, bytes));
        } else {
            for (name, raw) in case["files"].as_object().ok_or("Conflict inputs missing")? {
                let path = root.join(name);
                fs::create_dir_all(path.parent().ok_or("Input parent missing")?)?;
                let bytes = STANDARD.decode(raw.as_str().ok_or("Input bytes missing")?)?;
                fs::write(&path, &bytes)?;
                files.push((path, bytes));
            }
        }
        let repo = ProfileRepo::new(root, case["password"].as_str().map(str::to_owned));
        if case.get("raw").is_none()
            && [&case["stored"], &case["twin"]]
                .iter()
                .all(|value| value.is_null() || value.is_string())
        {
            let actual = repo.refuse_unresolved_secret(
                "fixture-conflict",
                case["stored"].as_str(),
                case["twin"].as_str(),
            );
            assert_eq!(
                actual.err().map(|error| error.to_string()),
                case["direct"].as_str().map(str::to_owned),
                "Direct credential conflict differs: {index}"
            );
        }
        for (intent, expected) in case["intents"]
            .as_object()
            .ok_or("Conflict results missing")?
        {
            let result = repo.refuse_conflicting_write("fixture-conflict", intent);
            let actual = match result {
                Ok(()) => Value::Null,
                Err(RepoError::Io(error)) if error.kind() == std::io::ErrorKind::InvalidData => {
                    "UnicodeDecodeError".into()
                }
                Err(error) => error.to_string().into(),
            };
            assert_eq!(
                actual, *expected,
                "Credential intent conflict differs: {index}/{intent}"
            );
        }
        for (path, bytes) in files {
            assert_eq!(
                fs::read(path)?,
                bytes,
                "Credential guard changed stored bytes"
            );
        }
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn saving_both_copies_keeps_dashboard_fields_and_explicit_secret_intent()
-> Result<(), Box<dyn std::error::Error>> {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/repo-write.json"))?;
    let cases = oracle["cases"].as_array().ok_or("Missing write cases")?;
    assert!(!cases.is_empty());
    for (index, case) in cases.iter().enumerate() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/test-temp")
            .join(format!(
                "repo-write-{}-{index}-{}",
                std::process::id(),
                SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
            ));
        for kind in ["profiles", "dashboard"] {
            fs::create_dir_all(root.join(kind))?;
        }
        let result = (|| -> Result<(), Box<dyn std::error::Error>> {
            let mut before = Vec::new();
            for kind in ["profiles", "dashboard"] {
                let path = root.join(kind).join("fixture-write.json");
                let bytes = serde_json::to_vec(&case["initial"][kind])?;
                fs::write(&path, &bytes)?;
                before.push((kind, path, bytes));
            }
            if !case["other"].is_null() {
                fs::write(
                    root.join("profiles/fixture-other.json"),
                    serde_json::to_vec(&case["other"])?,
                )?;
            }
            let mut profile = Profile::from_record(&case["profile"])?;
            if case["intent"] != "preserve"
                && let Some(proxy) = profile.proxy.as_mut()
            {
                proxy.set_password(
                    case["profile"]["proxy"]["password"]
                        .as_str()
                        .map(str::to_owned),
                );
            }
            let repo = ProfileRepo::new(root.clone(), None);
            match repo.save(profile, 1700000000.0) {
                Ok(profile) => {
                    assert!(case["error"].is_null());
                    assert_eq!(serde_json::to_value(profile)?, case["result"]);
                }
                Err(error) => assert_eq!(Some(error.to_string().as_str()), case["error"].as_str()),
            }
            for (kind, path, bytes) in before {
                let after = fs::read(path)?;
                assert_eq!(
                    after == bytes,
                    case["unchanged"][kind]
                        .as_bool()
                        .ok_or("Missing byte observation")?
                );
                assert_eq!(
                    serde_json::from_slice::<Value>(&after)?,
                    case["written"][kind]
                );
            }
            Ok(())
        })();
        fs::remove_dir_all(root)?;
        result?;
    }
    Ok(())
}

#[test]
fn failed_second_store_write_rolls_back_first_store_exactly()
-> Result<(), Box<dyn std::error::Error>> {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/repo-write.json"))?;
    let case = &oracle["cases"][0];
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/test-temp")
        .join(format!(
            "write-failure-{}-{}",
            std::process::id(),
            SystemTime::now().duration_since(UNIX_EPOCH)?.as_nanos()
        ));
    fs::create_dir_all(root.join("profiles"))?;
    fs::create_dir_all(root.join("dashboard/fixture-write.json"))?;
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let path = root.join("profiles/fixture-write.json");
        let before = serde_json::to_vec(&case["initial"]["profiles"])?;
        fs::write(&path, &before)?;
        let repo = ProfileRepo::new(root.clone(), None);
        assert!(
            repo.save(Profile::from_record(&case["profile"])?, 1700000000.0)
                .is_err()
        );
        assert_eq!(
            fs::read(path)?,
            before,
            "Failed dashboard write lost original engine bytes"
        );
        assert!(
            root.join("dashboard/fixture-write.json").is_dir(),
            "Caller filesystem obstruction changed"
        );
        Ok(())
    })();
    fs::remove_dir_all(root)?;
    result
}
