fn live_contracts_2_0() -> TestResult {
    let registry = "registry+https://github.com/rust-lang/crates.io-index";
    let local = json!({"name":"numera-core","version":"0.36.0","source":null});
    let packages = vec![
        local.clone(),
        json!({"name":"axum","version":"0.8.9","source":registry}),
        json!({"name":"cargo-audit","version":"0.22.0","source":registry}),
        json!({"name":"ring","version":"0.17.14","source":registry}),
    ];
    let retained = locked_dependencies(&json!({"packages":packages}), &["numera-core"])?;
    assert_eq!(retained, packages[1..]);
    let mut invalid = Vec::new();
    invalid.push(packages[1..].to_vec());
    let mut p = packages.clone();
    p.push(local);
    invalid.push(p);
    let mut p = packages.clone();
    p.push(json!({"name":"private-lib","version":"1.0.0","source":null}));
    invalid.push(p);
    let mut p = packages.clone();
    p.push(json!({"name":"path-lib","version":"1.0.0","source":"path+/other"}));
    invalid.push(p);
    let mut p = packages.clone();
    p.push(json!({"name":"unresolved","version":">=1","source":registry}));
    invalid.push(p);
    for p in invalid {
        assert!(locked_dependencies(&json!({"packages":p}), &["numera-core"]).is_err());
    }

    Ok(())
}

fn live_contracts_2_1() -> TestResult {
    let t = Temp::new()?;
    let text = t.path().join("source.rs");
    let binary = t.path().join("image.png");
    fs::write(&text, "let value = \"Ω\";\n")?;
    fs::write(&binary, b"\x89PNG\xff\x00")?;
    assert_eq!(scope::check_utf8(&[text.clone(), binary.clone()])?, 1);
    fs::write(&text, b"let value = \"\xff\";\n")?;
    assert!(scope::check_utf8(&[text, binary]).is_err());

    Ok(())
}

fn live_contracts_2_2() -> TestResult {
    let damage = "None";
    let expected = vec![
        "one::renders_native_component".to_owned(),
        "two::renders_native_component".into(),
    ];
    let mut report = evidence::Current::wasm(expected.clone())?;
    report.accept("running 2 tests");
    for (index, name) in expected.iter().enumerate() {
        if false && index == 1 {
            continue;
        }
        report.accept(&format!(
            "test {name} ... {}",
            if false && index == 0 { "ignored" } else { "ok" }
        ));
    }
    report.accept(
        "test result: ok. 2 passed; 0 failed; 0 ignored; 0 filtered out; finished in 0.01s",
    );
    assert_eq!(report.verify(0).is_ok(), damage == "None");

    Ok(())
}

fn live_contracts_2_3() -> TestResult {
    let damage = "missing";
    let expected = vec![
        "one::renders_native_component".to_owned(),
        "two::renders_native_component".into(),
    ];
    let mut report = evidence::Current::wasm(expected.clone())?;
    report.accept("running 2 tests");
    for (index, name) in expected.iter().enumerate() {
        if true && index == 1 {
            continue;
        }
        report.accept(&format!(
            "test {name} ... {}",
            if false && index == 0 { "ignored" } else { "ok" }
        ));
    }
    report.accept(
        "test result: ok. 2 passed; 0 failed; 0 ignored; 0 filtered out; finished in 0.01s",
    );
    assert_eq!(report.verify(0).is_ok(), damage == "None");

    Ok(())
}

fn live_contracts_2_4() -> TestResult {
    let damage = "skipped";
    let expected = vec![
        "one::renders_native_component".to_owned(),
        "two::renders_native_component".into(),
    ];
    let mut report = evidence::Current::wasm(expected.clone())?;
    report.accept("running 2 tests");
    for (index, name) in expected.iter().enumerate() {
        if false && index == 1 {
            continue;
        }
        report.accept(&format!(
            "test {name} ... {}",
            if true && index == 0 { "ignored" } else { "ok" }
        ));
    }
    report.accept(
        "test result: ok. 2 passed; 0 failed; 0 ignored; 0 filtered out; finished in 0.01s",
    );
    assert_eq!(report.verify(0).is_ok(), damage == "None");

    Ok(())
}

fn live_contracts_2_5() -> TestResult {
    let root = source_root();
    let config = reviewed_secrets::template(&root)?;
    assert!(config.contains("useDefault = true"));
    assert!(!config.contains("disabledRules"));
    let patterns = reviewed_secrets::groups(&config)?;
    reviewed_secrets::signing_guards(&root)?;
    let assets = root.join("engine-rust/src/http/documentation-assets");
    let provenance = ci::decode(&fs::read(assets.join("provenance.json"))?)?;
    for name in ["openapi.json", "oauth2-redirect.html"] {
        assert_eq!(
            ci::digest(&assets.join(name))?,
            provenance["sha256"][name]
                .as_str()
                .ok_or("asset hash missing")?
        );
    }
    use base64::Engine as _;
    let pem = fs::read_to_string(root.join("docs/release-signing-key.pem"))?;
    let body = pem
        .lines()
        .filter(|s| !s.starts_with("---"))
        .collect::<String>();
    let der = base64::engine::general_purpose::STANDARD.decode(body)?;
    assert_eq!(
        &der[..12],
        &[
            0x30, 0x2a, 0x30, 0x05, 0x06, 0x03, 0x2b, 0x65, 0x70, 0x03, 0x21, 0x00
        ]
    );
    let public = der[12..]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    assert_eq!(
        patterns[3].first().ok_or("public allowlist missing")?,
        &format!("^{public}$")
    );
    assert!(
        fs::read_to_string(root.join("desktop/src-tauri/src/updates/manifest.rs"))?
            .contains(&format!("pub const PUBLIC_KEY: &str = \"{public}\";"))
    );

    Ok(())
}

fn live_contracts_2_6() -> TestResult {
    let t = Temp::new()?;
    let base = t.path().join("official.toml");
    let output = t.path().join("strict.toml");
    let official = "[allowlist]\ndescription = \"official values\"\npaths = [\n    \"package-lock.json\",\n    \"asset[0-9].json\",\n]\nregexes = [\"^false$\"]\n[[rules]]\nid = \"required-rule\"\nregex = \"required-detection\"\npath = \"required-selector\"\n[[rules.allowlists]]\npaths = [\"hidden.txt\"]\nregexes = [\"^official-constant$\"]\n[[rules.allowlists]]\npaths = [\"path-only.txt\"]\n[[rules]]\nid = \"another-required-rule\"\nregex = \"another-detection\"\n";
    let payload = official.as_bytes().to_vec();
    fs::write(&base, &payload)?;
    assert_eq!(fs::read(&base)?, payload);
    assert!(
        scope::prepare_secret_config_checked(&source_root(), &base, &output, &"0".repeat(64))
            .err()
            .ok_or("wrong checksum accepted")?
            .to_string()
            .contains("checksum")
    );
    assert_eq!(fs::read(&base)?, payload);
    assert!(!output.exists());
    scope::prepare_secret_config_checked(&source_root(), &base, &output, &ci::sha(&payload))?;
    let changed = fs::read_to_string(&base)?;
    assert_eq!(changed.matches("paths = []").count(), 1);
    for removed in [
        "package-lock.json",
        "hidden.txt",
        "path-only.txt",
        "[[rules.allowlists]]",
        "official-constant",
    ] {
        assert!(!changed.contains(removed));
    }
    for required in [
        "another-detection",
        "required-detection",
        "path = \"required-selector\"",
        "regexes = [\"^false$\"]",
    ] {
        assert!(changed.contains(required));
    }
    let config = fs::read_to_string(output)?;
    assert!(!config.contains("useDefault"));
    assert!(config.contains("path = "));
    reviewed_secrets::groups(&config)?;
    let expected = reviewed_secrets::template(&source_root())?.replace(
        "useDefault = true",
        &format!("path = {}", serde_json::to_string(&base.canonicalize()?)?),
    );
    assert_eq!(config, expected);

    Ok(())
}

fn live_contracts_2_7() -> TestResult {
    let t = Temp::new()?;
    let base = t.path().join("official.toml");
    let output = t.path().join("strict.toml");
    let official = "[allowlist]\ndescription = \"official values\"\npaths = [\n    \"package-lock.json\",\n    \"asset[0-9].json\",\n]\nregexes = [\"^false$\"]\n[[rules]]\nid = \"required-rule\"\nregex = \"required-detection\"\npath = \"required-selector\"\n[[rules.allowlists]]\npaths = [\"hidden.txt\"]\nregexes = [\"^official-constant$\"]\n[[rules.allowlists]]\npaths = [\"path-only.txt\"]\n[[rules]]\nid = \"another-required-rule\"\nregex = \"another-detection\"\n";
    let payload = official.replace('\n', "\r\n").into_bytes();
    fs::write(&base, &payload)?;
    assert_eq!(fs::read(&base)?, payload);
    assert!(
        scope::prepare_secret_config_checked(&source_root(), &base, &output, &"0".repeat(64))
            .err()
            .ok_or("wrong checksum accepted")?
            .to_string()
            .contains("checksum")
    );
    assert_eq!(fs::read(&base)?, payload);
    assert!(!output.exists());
    scope::prepare_secret_config_checked(&source_root(), &base, &output, &ci::sha(&payload))?;
    let changed = fs::read_to_string(&base)?;
    assert_eq!(changed.matches("paths = []").count(), 1);
    for removed in [
        "package-lock.json",
        "hidden.txt",
        "path-only.txt",
        "[[rules.allowlists]]",
        "official-constant",
    ] {
        assert!(!changed.contains(removed));
    }
    for required in [
        "another-detection",
        "required-detection",
        "path = \"required-selector\"",
        "regexes = [\"^false$\"]",
    ] {
        assert!(changed.contains(required));
    }
    let config = fs::read_to_string(output)?;
    assert!(!config.contains("useDefault"));
    assert!(config.contains("path = "));
    reviewed_secrets::groups(&config)?;
    let expected = reviewed_secrets::template(&source_root())?.replace(
        "useDefault = true",
        &format!("path = {}", serde_json::to_string(&base.canonicalize()?)?),
    );
    assert_eq!(config, expected);

    Ok(())
}
