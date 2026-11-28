#![cfg(feature = "native-product")]

use numera_core::{
    fingerprint::{GenerateOptions, generate},
    models::Profile,
    ownership::capture::{self, Limits},
    storage::{pool::ProxyPool, repo::ProfileRepo},
    vault,
};
use serde_json::{Value, json};
use std::{fs, path::Path, process::Command, time::Duration};
mod common;

const MASTER: &str = "synthetic-vault-master";
const ORIGINAL: &str = "synthetic-original-proxy-password";
const REPLACEMENT: &str = "synthetic-replacement-proxy-password";

fn encrypted_pool(root: &Path) -> Result<String, Box<dyn std::error::Error>> {
    ProfileRepo::open(root.into(), None, None)?;
    let rows = ProxyPool::new(root.into(), None).add_many(vec![json!({
        "type":"HTTP", "host":"proxy.invalid", "port":"8080", "user":"fixture",
        "pass":ORIGINAL, "country":"US"
    })])?;
    vault::transaction::enable(root, MASTER)?;
    Ok(rows[0]["id"].as_str().ok_or("Pool ID missing")?.into())
}

fn command(
    root: &Path,
    explicit_master: Option<&str>,
    inherited_master: Option<&str>,
    arguments: &[&str],
) -> Result<(i32, Value), Box<dyn std::error::Error>> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_numera-engine"));
    command
        .arg("--data-dir")
        .arg(root)
        .env_remove("NUMERA_PASSWORD")
        .env_remove("NUMERA_NATIVE_SUPERVISE")
        .env_remove("NUMERA_NATIVE_JOB_GATE")
        .env("NUMERA_LOG_DIR", root.join("logs"));
    if let Some(master) = explicit_master {
        command.arg("--password").arg(master);
    }
    if let Some(master) = inherited_master {
        command.env("NUMERA_PASSWORD", master);
    }
    command.args(arguments).arg("--json");
    let output = capture::run(
        &mut command,
        Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(30),
            bytes: 1024 * 1024,
            cleanup: Duration::from_secs(5),
        },
    )?;
    Ok((output.exit, serde_json::from_slice(&output.stdout)?))
}

fn assert_replacement(root: &Path, id: &str) -> Result<(), Box<dyn std::error::Error>> {
    let row = ProxyPool::new(root.into(), Some(MASTER.into()))
        .get(id)?
        .ok_or("Edited proxy missing")?;
    assert!(
        row["pass"] == REPLACEMENT,
        "Replacement was not saved under the vault key"
    );
    let stored: Value = serde_json::from_slice(&fs::read(root.join("proxies.json"))?)?;
    assert!(
        stored[0]["pass"].as_str().is_some_and(vault::is_envelope),
        "Edited proxy password was stored outside the vault"
    );
    let bytes = fs::read_to_string(root.join("proxies.json"))?;
    for secret in [MASTER, ORIGINAL, REPLACEMENT] {
        assert!(
            !bytes.contains(secret),
            "Proxy storage disclosed a credential"
        );
    }
    Ok(())
}

#[test]
fn pool_edit_keeps_explicit_vault_password_separate_from_proxy_password()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("pool-explicit-master")?;
    let id = encrypted_pool(temporary.path())?;
    let (exit, _) = command(
        temporary.path(),
        Some(MASTER),
        Some("synthetic-wrong-inherited-master"),
        &["pool", "edit", &id, "--pass", REPLACEMENT],
    )?;
    assert_eq!(
        exit, 0,
        "Proxy edit discarded the explicit vault credential"
    );
    assert_replacement(temporary.path(), &id)?;
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn pool_edit_keeps_inherited_vault_password_separate_from_proxy_password()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("pool-inherited-master")?;
    let id = encrypted_pool(temporary.path())?;
    let (exit, _) = command(
        temporary.path(),
        None,
        Some(MASTER),
        &["pool", "edit", &id, "--pass", REPLACEMENT],
    )?;
    assert_eq!(
        exit, 0,
        "Proxy edit replaced the inherited vault credential"
    );
    assert_replacement(temporary.path(), &id)?;
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn pool_metadata_edit_retains_the_explicit_vault_unlock() -> Result<(), Box<dyn std::error::Error>>
{
    let temporary = common::TempDirectory::new("pool-metadata-master")?;
    let id = encrypted_pool(temporary.path())?;
    let (exit, result) = command(
        temporary.path(),
        Some(MASTER),
        None,
        &["pool", "edit", &id, "--label", "renamed"],
    )?;
    assert_eq!(exit, 0);
    assert!(
        result["proxy"]["pass"] == ORIGINAL,
        "Non-secret edit lost the supplied vault unlock"
    );
    assert_eq!(result["proxy"]["label"], "renamed");
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn pool_proxy_password_is_never_accepted_as_vault_authorization()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("pool-unauthorized-master")?;
    let id = encrypted_pool(temporary.path())?;
    let before = fs::read(temporary.path().join("proxies.json"))?;
    let (exit, _) = command(
        temporary.path(),
        None,
        None,
        &["pool", "edit", &id, "--pass", MASTER],
    )?;
    assert_ne!(
        exit, 0,
        "Proxy field was used as an implicit vault credential"
    );
    assert_eq!(fs::read(temporary.path().join("proxies.json"))?, before);
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn mcp_pool_edit_preserves_the_existing_password_input_schema()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("pool-mcp-master")?;
    let id = encrypted_pool(temporary.path())?;
    let arguments = json!({"id":id, "password":REPLACEMENT});
    let result = numera_core::mcp_command::call(
        Path::new(env!("CARGO_BIN_EXE_numera-engine")),
        Some(temporary.path()),
        Some(MASTER),
        "pool_edit",
        arguments.as_object().ok_or("MCP arguments missing")?,
    )?;
    assert_eq!(result["isError"], false);
    assert_eq!(
        result["_meta"]["exitCode"], 0,
        "MCP proxy edit used the proxy password to unlock the vault"
    );
    assert_replacement(temporary.path(), &id)?;
    temporary.cleanup()?;
    Ok(())
}

fn profile(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let repository = ProfileRepo::open(root.into(), None, None)?;
    let fingerprint = generate(GenerateOptions {
        seed: 42,
        ..Default::default()
    })?;
    repository.save(
        Profile::from_record(&json!({
            "id":"profile", "name":"Fixture", "fingerprint":fingerprint,
            "created_at":1000.0, "updated_at":1000.0
        }))?,
        1000.0,
    )?;
    Ok(())
}

#[test]
fn automatic_pool_assignment_excludes_archived_credentials_before_unlock_checks()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("pool-archived-default")?;
    profile(temporary.path())?;
    ProxyPool::new(temporary.path().into(), None).add_many(vec![
        json!({"type":"HTTP", "host":"retired.invalid", "port":"8080", "pass":ORIGINAL, "archived":true}),
        json!({"type":"HTTP", "host":"active.invalid", "port":"8080", "pass":null}),
    ])?;
    vault::transaction::enable(temporary.path(), MASTER)?;
    let (exit, result) = command(temporary.path(), None, None, &["pool", "assign", "--all"])?;
    assert_eq!(exit, 0, "Archived credentials blocked automatic assignment");
    assert_eq!(result["pool"], 1);
    let saved = ProfileRepo::new(temporary.path().into(), None)
        .get("profile")?
        .ok_or("Assigned profile missing")?;
    assert_eq!(
        saved.proxy.ok_or("Assigned proxy missing")?.server,
        "http://active.invalid:8080"
    );
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn all_archived_pool_refuses_automatic_assignment_but_allows_explicit_selection()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("pool-all-archived")?;
    profile(temporary.path())?;
    let rows = ProxyPool::new(temporary.path().into(), None).add_many(vec![
        json!({"type":"HTTP", "host":"retired.invalid", "port":"8080", "archived":true}),
    ])?;
    let before = fs::read(temporary.path().join("profiles/profile.json"))?;
    let (exit, result) = command(temporary.path(), None, None, &["pool", "assign", "--all"])?;
    assert_ne!(exit, 0, "Automatic assignment reused an archived proxy");
    assert!(
        result["error"]
            .as_str()
            .is_some_and(|text| text.contains("archived"))
    );
    assert_eq!(
        fs::read(temporary.path().join("profiles/profile.json"))?,
        before
    );
    let id = rows[0]["id"].as_str().ok_or("Archived proxy ID missing")?;
    let (exit, result) = command(
        temporary.path(),
        None,
        None,
        &["pool", "assign", "--all", "--proxy-id", id],
    )?;
    assert_eq!(exit, 0, "Explicit archived proxy selection was refused");
    assert_eq!(result["pool"], 1);
    assert_eq!(
        result["profiles"][0]["proxy"],
        "http://retired.invalid:8080"
    );
    temporary.cleanup()?;
    Ok(())
}
