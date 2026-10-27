use super::*;
use ed25519_dalek::{Signer, SigningKey};
use serde_json::json;
use std::{fs, path::PathBuf};

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target/update-tests")
            .join(files::nonce().expect("Fixture nonce"));
        fs::create_dir_all(&root).expect("Fixture root");
        Self(root)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn key() -> SigningKey {
    SigningKey::from_bytes(&[7; 32])
}
fn file(path: &str, bytes: &[u8]) -> manifest::InstalledFile {
    manifest::InstalledFile::Regular(manifest::RegularFile {
        path: path.into(),
        size: bytes.len() as u64,
        sha256: manifest::digest(bytes),
        executable: false,
    })
}
fn build(root: &std::path::Path, generation: u64, bytes: &[u8]) -> manifest::Build {
    fs::create_dir_all(root).expect("Build root");
    fs::write(
        root.join(if cfg!(windows) {
            "numera.exe"
        } else {
            "numera"
        }),
        bytes,
    )
    .expect("Shell fixture");
    let build = manifest::Build {
        schema: 1,
        owner: "KiaroSama".into(),
        product: manifest::PRODUCT.into(),
        version: "0.36.0".into(),
        source_sha: format!("{generation:040x}"),
        generation,
        platform: manifest::platform().expect("Platform").into(),
        minimum_protocol: 1,
        components: json!({"sdk":{},"browsers":{}}),
        files: vec![file(
            if cfg!(windows) {
                "numera.exe"
            } else {
                "numera"
            },
            bytes,
        )],
    };
    let bytes = serde_json::to_vec(&build).expect("Build JSON");
    fs::write(root.join("BUILD.json"), &bytes).expect("Build inventory");
    fs::write(root.join("BUILD.sig"), key().sign(&bytes).to_bytes()).expect("Build signature");
    build
}
#[test]
fn hex_identity_is_derived_from_every_input_byte_and_rejects_noncanonical_text() {
    assert_eq!(
        manifest::decode_hex::<4>("00a1fe7b").unwrap(),
        [0, 161, 254, 123]
    );
    assert_eq!(manifest::decode_hex::<2>("abcd").unwrap(), [171, 205]);
    for invalid in ["", "abc", "abcde", "ABCD", "ab g", "éab"] {
        assert!(
            manifest::decode_hex::<2>(invalid).is_err(),
            "accepted {invalid:?}"
        );
    }
}

#[test]
fn discovery_uses_the_published_raw_signature_asset_name() {
    let source = "a".repeat(40);
    let (metadata, signature) = download::manifest_urls(&source);
    assert_eq!(metadata,format!("https://github.com/KiaroSama/Numera-Browser/releases/download/build-{source}/update-manifest.json"));
    assert_eq!(signature,format!("https://github.com/KiaroSama/Numera-Browser/releases/download/build-{source}/update-manifest.sig"));
}
#[test]
fn signed_manifest_rejects_tamper_wrong_key_replay_and_unsupported_platform() {
    let platform = manifest::platform().expect("Platform");
    let value = json!({"schema":1,"repository":manifest::REPOSITORY,"product":manifest::PRODUCT,"version":"0.36.0","source_sha":"a".repeat(40),"generation":2,"created_at":1000,"expires_at":2000,"minimum_protocol":1,"components":{(platform):{"sdk":{},"browsers":{}}},"assets":[{"platform":platform,"name":"numera-fixture.zip","size":10,"sha256":"b".repeat(64),"expanded_size":20,"max_members":4,"archive_root":manifest::PRODUCT,"build_manifest_sha256":"c".repeat(64)}]});
    let bytes = serde_json::to_vec(&value).expect("Manifest");
    let signature = key().sign(&bytes).to_bytes();
    let public = key().verifying_key().to_bytes();
    assert!(manifest::signed_manifest(&bytes, &signature, &public, platform, 1, 1200).is_ok());
    let mut changed = bytes.clone();
    changed.push(b' ');
    assert!(manifest::signed_manifest(&changed, &signature, &public, platform, 1, 1200).is_err());
    assert!(manifest::signed_manifest(
        &bytes,
        &signature,
        &SigningKey::from_bytes(&[8; 32]).verifying_key().to_bytes(),
        platform,
        1,
        1200
    )
    .is_err());
    assert!(manifest::signed_manifest(&bytes, &signature, &public, platform, 2, 1200).is_err());
    assert!(
        manifest::signed_manifest(&bytes, &signature, &public, "unsupported", 1, 1200).is_err()
    );
    assert!(manifest::signed_manifest(&bytes, &signature, &public, platform, 1, 2001).is_err());
}
#[test]
fn signed_build_must_match_the_exact_approved_offer_identity_and_inventory() {
    let public = key().verifying_key().to_bytes();
    let original = json!({
        "schema": 1, "owner": "KiaroSama", "product": manifest::PRODUCT,
        "version": "0.36.0", "source_sha": "a".repeat(40), "generation": 2,
        "platform": "linux-x64", "minimum_protocol": 1,
        "components": {"sdk": {}, "browsers": {}},
        "files": [{"path": "numera", "size": 3, "sha256": manifest::digest(b"old"),
                   "executable": true}]
    });
    let check = |candidate: &serde_json::Value, approved_digest: Option<&str>| {
        let bytes = serde_json::to_vec(candidate).expect("Build inventory");
        let digest = manifest::digest(&bytes);
        let build = manifest::signed_build(
            &bytes,
            &key().sign(&bytes).to_bytes(),
            &public,
            candidate["platform"].as_str().expect("Build platform"),
        )
        .expect("Candidate has a valid trusted build signature and inventory");
        let offer = json!({
            "schema": 1, "repository": manifest::REPOSITORY, "product": manifest::PRODUCT,
            "version": "0.36.0", "source_sha": "a".repeat(40), "generation": 2,
            "created_at": 1000, "expires_at": 2000, "minimum_protocol": 1,
            "components": {"linux-x64": {"sdk": {}, "browsers": {}}},
            "assets": [{"platform": "linux-x64", "name": "numera-fixture.tar.gz",
                        "size": 10, "sha256": "b".repeat(64), "expanded_size": 20,
                        "max_members": 4, "archive_root": manifest::PRODUCT,
                        "build_manifest_sha256": approved_digest.unwrap_or(&digest)}]
        });
        let metadata = serde_json::to_vec(&offer).expect("Approved offer");
        let offer = manifest::signed_manifest(
            &metadata,
            &key().sign(&metadata).to_bytes(),
            &public,
            "linux-x64",
            1,
            1200,
        )
        .expect("Offer has a valid trusted signature and compatible generation");
        offer.validate_build(&build, &digest)
    };
    assert!(check(&original, None).is_ok());
    for (field, replacement) in [
        ("version", json!("0.36.1")),
        ("source_sha", json!("d".repeat(40))),
        ("generation", json!(3)),
        (
            "components",
            json!({"sdk": {"playwright": "changed"}, "browsers": {}}),
        ),
        (
            "components",
            json!({"sdk": {}, "browsers": {"playwright": {"revision": "changed"}}}),
        ),
    ] {
        let mut changed = original.clone();
        changed[field] = replacement;
        // The offer authenticates this changed inventory too: only its identity differs.
        assert!(
            check(&changed, None).is_err(),
            "accepted mismatched {field}"
        );
    }
    let mut wrong_platform = original.clone();
    wrong_platform["platform"] = json!("windows-x64");
    wrong_platform["files"][0]["path"] = json!("numera.exe");
    assert!(check(&wrong_platform, None).is_err());
    let approved = manifest::digest(&serde_json::to_vec(&original).expect("Original inventory"));
    let mut changed_inventory = original.clone();
    changed_inventory["files"][0]["sha256"] = json!(manifest::digest(b"new"));
    assert!(check(&changed_inventory, Some(&approved)).is_err());
    assert!(check(&changed_inventory, None).is_ok());
}
#[test]
fn journaled_activation_and_rollback_preserve_real_format_mutable_data(
) -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Scratch::new();
    let root = fixture.0.join("portable with spaces");
    let old = build(&root, 1, b"old immutable shell");
    fs::create_dir_all(root.join("data/profiles"))?;
    let profile=br#"{"id":"fixture","name":"Preserved","proxy":{"password":"synthetic-fixture"},"unknown":{"retained":true}}"#;
    fs::write(root.join("data/profiles/fixture.json"), profile)?;
    let journal = transaction::create(&root, &"a".repeat(64))?;
    let next = build(&journal.staged(), 2, b"new immutable shell");
    let mut journal = journal;
    transaction::activate(&mut journal, &old, &next)?;
    assert_eq!(fs::read(root.join("data/profiles/fixture.json"))?, profile);
    assert!(transaction::validate_files(&root, &next).is_ok());
    transaction::rollback(&mut journal, &old, &next)?;
    assert!(transaction::validate_files(&root, &old).is_ok());
    assert_eq!(fs::read(root.join("data/profiles/fixture.json"))?, profile);
    Ok(())
}
#[test]
fn unsigned_install_and_cancelled_unapproved_download_do_not_mutate_installation(
) -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Scratch::new();
    let service = Service::new(
        fixture.0.clone(),
        1,
        "synthetic-private-token".into(),
        Arc::new(|| {}),
    )?;
    assert!(!service.status().current.portable);
    assert!(service.download("unapproved", false).is_err());
    assert!(service.install("unapproved", false).is_err());
    assert!(!fixture.0.join("BUILD.json").exists());
    assert!(
        !fs::read_dir(&fixture.0)?.any(|entry| entry.is_ok_and(|entry| entry
            .file_name()
            .to_string_lossy()
            .starts_with(".numera-update-")))
    );
    Ok(())
}
#[test]
fn accepted_cancellation_cannot_become_an_installable_ready_state(
) -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Scratch::new();
    let service = Service::new(fixture.0.clone(), 1, "fixture".into(), Arc::new(|| {}))?;
    service.set("downloading", "Fixture download");
    service.cancel("")?;
    assert!(service.finish_download().is_err());
    assert_ne!(service.status().state, "ready");
    Ok(())
}
#[test]
fn accepted_generation_state_is_not_an_interrupted_transaction(
) -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Scratch::new();
    transaction::commit_floor(&fixture.0, 5)?;
    assert!(recovery::pending(&fixture.0)?.is_none());
    assert_eq!(transaction::generation_floor(&fixture.0, 1)?, 5);
    transaction::commit_floor(&fixture.0, 2)?;
    assert_eq!(transaction::generation_floor(&fixture.0, 1)?, 5);
    Ok(())
}
#[test]
fn committed_generation_survives_a_lost_final_journal_save(
) -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Scratch::new();
    let mut journal = transaction::create(&fixture.0, &"a".repeat(64))?;
    journal.phase = "awaiting-ready".into();
    journal.ticket = "b".repeat(64);
    journal.data_root = fixture.0.join("data");
    journal.save()?;
    files::json(
        &journal.directory.join("commit.json"),
        &json!({
            "nonce": journal.nonce, "sourceSha": "c".repeat(40), "generation": 2
        }),
    )?;
    assert_eq!(worker::committed(&journal)?, Some(("c".repeat(40), 2)));
    assert!(recovery::pending(&fixture.0)?.is_some());
    let loaded = transaction::Journal::load(&journal.directory.join("journal.json"))?;
    assert_eq!(worker::committed(&loaded)?, Some(("c".repeat(40), 2)));
    files::json(
        &journal.directory.join("commit.json"),
        &json!({
            "nonce": "d".repeat(64), "sourceSha": "c".repeat(40), "generation": 2
        }),
    )?;
    assert!(worker::committed(&loaded).is_err());
    Ok(())
}
#[test]
fn rollback_remains_pending_until_private_resume_is_confirmed(
) -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Scratch::new();
    let root = fixture.0.join("rollback");
    let old = build(&root, 1, b"old");
    let mut journal = transaction::create(&root, &"a".repeat(64))?;
    journal.ticket = "b".repeat(64);
    journal.data_root = root.join("data");
    let next = build(&journal.staged(), 2, b"next");
    transaction::activate(&mut journal, &old, &next)?;
    transaction::rollback(&mut journal, &old, &next)?;
    assert_eq!(journal.phase, "rollback-pending");
    assert!(recovery::pending(&root)?.is_some());
    transaction::rollback(&mut journal, &old, &next)?;
    transaction::validate_files(&root, &old)?;
    Ok(())
}
#[test]
fn unsupported_file_directory_transition_refuses_before_mutation(
) -> Result<(), Box<dyn std::error::Error>> {
    let fixture = Scratch::new();
    let root = fixture.0.join("file-directory-change");
    let mut old = build(&root, 1, b"old");
    fs::write(root.join("resource"), b"old resource")?;
    old.files.push(file("resource", b"old resource"));
    let mut journal = transaction::create(&root, &"a".repeat(64))?;
    let mut next = build(&journal.staged(), 2, b"next");
    fs::create_dir(journal.staged().join("resource"))?;
    fs::write(journal.staged().join("resource/data"), b"next resource")?;
    next.files.push(file("resource/data", b"next resource"));
    assert!(transaction::activate(&mut journal, &old, &next).is_err());
    transaction::validate_files(&root, &old)?;
    assert!(!journal.backup().exists());
    Ok(())
}
#[test]
fn file_inventory_refuses_mutable_paths_and_escaping_links() {
    for path in [
        "data/profiles/a.json",
        "../numera",
        "engine/../../data/a",
        "C:/numera.exe",
        "engine/secret\0",
        ".updates/runtime.lock",
        ".numera-update-generation.json",
        ".numera-update-transaction/journal.json",
    ] {
        assert!(manifest::safe_path(path).is_err(), "{path}");
    }
    assert!(manifest::safe_path("engine/sdk/runtime.dll").is_ok());
    assert!(manifest::safe_link("engine/link", "../../outside").is_err());
    assert!(manifest::safe_link("engine/lib/link", "../shared.dll").is_ok());
}
