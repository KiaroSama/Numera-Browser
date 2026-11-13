//! The actual bootstrap's shared pure contracts, exercised offline.
use super::*;
use ci::gitleaks_contract as contract;
type ContractResult = Result<(), Box<dyn std::error::Error>>;

fn configuration() -> ContractResult {
    use std::io::Read;
    let packed = include_bytes!("../../fixtures/security-scan/official-8.30.1.toml.gz");
    assert_eq!(
        contract::hash(packed),
        "da1838bd7cedb1bbd297163435c1675e777babb98ddf641a653f309733bd1fd1"
    );
    let mut decoder = flate2::bufread::GzDecoder::new(std::io::Cursor::new(packed));
    let mut official = Vec::new();
    decoder.by_ref().take(97732).read_to_end(&mut official)?;
    assert_eq!(
        decoder.into_inner().position(),
        packed.len() as u64,
        "Upstream input has unreviewed trailing data"
    );
    let provenance: Value =
        serde_json::from_str(include_str!("../../fixtures/security-scan/provenance.json"))?;
    assert_eq!(contract::hash(&official), contract::OFFICIAL_HASH);
    assert_eq!(provenance["sha256"], contract::OFFICIAL_HASH);
    assert_eq!(provenance["bytes"], official.len());
    assert_eq!(provenance["version"], contract::VERSION);
    let reviewed = fs::read(root().join("scripts/ci/gitleaks.toml"))?;
    assert_eq!(contract::hash(&reviewed), contract::REVIEWED_HASH);
    assert_eq!(contract::REVIEWED_HASH, scope::REVIEWED_RULES_SHA256);
    let destination = "C:\\owned scanner\\quoted\"name\\official.toml";
    let (sanitized, config) = contract::configuration(&official, &reviewed, destination)?;
    let encoded = format!("path = {}", serde_json::to_string(destination)?);
    assert!(config.contains(&encoded));
    assert!(!config.contains("useDefault = true"));
    // Changing the inherited file path must be the only change to the complete
    // reviewed rule source, including its exact path/value-bound exceptions.
    assert_eq!(
        contract::hash(config.replace(&encoded, "useDefault = true").as_bytes()),
        contract::REVIEWED_HASH
    );
    let ids = regex::Regex::new(r#"(?m)^id = "([^"]+)"$"#)?;
    assert_eq!(
        ids.captures_iter(std::str::from_utf8(&official)?)
            .map(|row| row[1].to_owned())
            .collect::<Vec<_>>(),
        ids.captures_iter(&sanitized)
            .map(|row| row[1].to_owned())
            .collect::<Vec<_>>()
    );
    assert!(sanitized.contains("paths = []"));
    // A path-based detection rule is retained: only suppression paths go away.
    assert!(sanitized.contains("id = \"pkcs12-file\"\ndescription = \"Found a PKCS #12 file"));
    assert!(sanitized.contains("path = '''(?i)(?:^|\\/)[^\\/]+\\.p(?:12|fx)$'''"));
    for block in sanitized.split("[[rules.allowlists]]").skip(1) {
        let block = block
            .split("[[")
            .next()
            .ok_or("Rule suppression block missing")?;
        assert!(
            !block
                .lines()
                .any(|line| line.trim_start().starts_with("paths ="))
        );
    }
    let mut changed_official = official.to_vec();
    changed_official[0] ^= 1;
    let mut changed_reviewed = reviewed.clone();
    changed_reviewed[0] ^= 1;
    for (upstream, local) in [
        (&[][..], reviewed.as_slice()),
        (official.as_slice(), &[][..]),
        (changed_official.as_slice(), reviewed.as_slice()),
        (official.as_slice(), changed_reviewed.as_slice()),
        (&[][..], &[][..]),
    ] {
        assert!(contract::configuration(upstream, local, destination).is_err());
    }
    let injection = "/owned/\"\nuseDefault = true\nextra = \"";
    let (_, config) = contract::configuration(&official, &reviewed, injection)?;
    assert!(config.contains(&format!("path = {}", serde_json::to_string(injection)?)));
    assert!(
        !config
            .lines()
            .any(|line| matches!(line, "useDefault = true" | "extra = \""))
    );
    Ok(())
}

fn sanitizer() -> ContractResult {
    let input = "title = 'public control'\n[allowlist]\npaths = [\n'''skip-everything'''\n]\n[[rules]]\nid = 'file-check'\npath = '''\\.pfx$'''\n[[rules.allowlists]]\npaths = ['''unsafe-directory''']\nregexes = ['''unsafe-context''']\n[[rules]]\nid = 'text-check'\nregex = '''still-detect'''\n[[rules.allowlists]]\nregexTarget = 'secret'\nregexes = ['''known-public-word''']\n";
    let expected = "title = 'public control'\n[allowlist]\npaths = []\n[[rules]]\nid = 'file-check'\npath = '''\\.pfx$'''\n[[rules]]\nid = 'text-check'\nregex = '''still-detect'''\n[[rules.allowlists]]\nregexTarget = 'secret'\nregexes = ['''known-public-word''']\n";
    assert_eq!(contract::sanitize(input)?, expected);
    assert_eq!(
        contract::sanitize(&input.replace('\n', "\r\n"))?.replace("\r\n", "\n"),
        expected
    );
    for unknown_layout in [
        "title = 'missing upstream exclusions'\n".to_owned(),
        input.replace(
            "paths = [\n'''skip-everything'''\n]",
            "paths = ['''changed-layout''']",
        ),
        input.to_owned() + "\n[[allowlists]]\npaths = [\n'''second-global-list'''\n]\n",
    ] {
        assert!(contract::sanitize(&unknown_layout).is_err());
    }
    Ok(())
}

fn finding() -> Value {
    let mut row =
        json!({"RuleID":"rule-one","File":"src/example.rs","StartLine":7,"Commit":"a".repeat(40)});
    for (index, field) in [
        "Secret",
        "Match",
        "Message",
        "CommitMessage",
        "Description",
        "Author",
        "Email",
    ]
    .iter()
    .enumerate()
    {
        row[*field] = format!(
            "{}-{index}",
            ["private", "fixture", "must", "stay", "out"].join("-")
        )
        .into();
    }
    row
}
fn reports() -> ContractResult {
    let good = finding();
    assert!(contract::report(0, &[], &[], b"[]")?.is_empty());
    assert_eq!(
        contract::report(2, &[], &[], &serde_json::to_vec(&json!([good]))?)?,
        vec![good.clone()]
    );
    for (exit, stdout, stderr, body) in [
        (0, &[][..], &[][..], serde_json::to_vec(&json!([good]))?),
        (2, &[][..], &[][..], b"[]".to_vec()),
        (1, &[][..], &[][..], b"[]".to_vec()),
        (-1, &[][..], &[][..], b"[]".to_vec()),
        (137, &[][..], &[][..], b"[]".to_vec()),
        (0, b"unexpected diagnostic", &[][..], b"[]".to_vec()),
        (0, &[][..], b"unexpected diagnostic", b"[]".to_vec()),
    ] {
        assert!(contract::report(exit, stdout, stderr, &body).is_err());
    }
    for body in [
        b"{".as_slice(),
        b"null",
        b"{}",
        b"true",
        b"17",
        b"[null]",
        b"[17]",
        b"[[]]",
    ] {
        assert!(contract::report(2, &[], &[], body).is_err());
    }
    for field in ["RuleID", "File", "StartLine", "Commit"] {
        let mut missing = good.clone();
        missing
            .as_object_mut()
            .ok_or("Finding object missing")?
            .remove(field);
        assert!(contract::report(2, &[], &[], &serde_json::to_vec(&json!([missing]))?).is_err());
        for value in [Value::Null, json!(false), json!([]), json!({})] {
            let mut bad = good.clone();
            bad[field] = value;
            assert!(contract::report(2, &[], &[], &serde_json::to_vec(&json!([bad]))?).is_err());
        }
    }
    for field in ["RuleID", "File", "Commit"] {
        let mut bad = good.clone();
        bad[field] = 7.into();
        assert!(contract::report(2, &[], &[], &serde_json::to_vec(&json!([bad]))?).is_err());
    }
    for value in [json!(-1), json!(1.25), json!("7")] {
        let mut bad = good.clone();
        bad["StartLine"] = value;
        assert!(contract::report(2, &[], &[], &serde_json::to_vec(&json!([bad]))?).is_err());
    }
    for value in [
        "a".repeat(39),
        "a".repeat(41),
        "A".repeat(40),
        "g".repeat(40),
        format!(" {}", "a".repeat(40)),
    ] {
        let mut bad = good.clone();
        bad["Commit"] = value.into();
        assert!(contract::report(2, &[], &[], &serde_json::to_vec(&json!([bad]))?).is_err());
    }
    let mut duplicate = good.clone();
    duplicate["Secret"] = "different ignored field".into();
    assert!(
        contract::report(2, &[], &[], &serde_json::to_vec(&json!([good, duplicate]))?).is_err()
    );
    let mut distinct = good.clone();
    distinct["Commit"] = "b".repeat(40).into();
    assert_eq!(
        contract::report(2, &[], &[], &serde_json::to_vec(&json!([good, distinct]))?)?.len(),
        2
    );
    Ok(())
}

fn receipts() -> ContractResult {
    let nonce = "owned-attempt-17";
    let git = contract::hash(b"public git binary control");
    let shim = contract::hash(b"public shim binary control");
    let receipt = json!({"schema":1,"completed":true,"exit":0,"nonce":nonce,"git_sha256":git,"shim_sha256":shim});
    contract::receipt(&receipt, nonce, &git, &shim)?;
    for value in [Value::Null, json!(true), json!(17), json!([])] {
        assert!(contract::receipt(&value, nonce, &git, &shim).is_err());
    }
    for field in [
        "schema",
        "completed",
        "exit",
        "nonce",
        "git_sha256",
        "shim_sha256",
    ] {
        let mut missing = receipt.clone();
        missing
            .as_object_mut()
            .ok_or("Receipt object missing")?
            .remove(field);
        assert!(contract::receipt(&missing, nonce, &git, &shim).is_err());
    }
    for (field, value) in [
        ("schema", json!(2)),
        ("schema", json!("1")),
        ("completed", json!(false)),
        ("exit", json!(1)),
        ("exit", json!(-9)),
        ("exit", json!("0")),
        ("nonce", json!("another-attempt")),
        ("nonce", Value::Null),
        ("git_sha256", json!("different binary")),
        ("shim_sha256", json!("different shim")),
    ] {
        let mut bad = receipt.clone();
        bad[field] = value;
        assert!(contract::receipt(&bad, nonce, &git, &shim).is_err());
    }
    assert!(contract::receipt(&receipt, "another-attempt", &git, &shim).is_err());
    assert!(
        contract::receipt(
            &receipt,
            nonce,
            &contract::hash(b"replacement binary"),
            &shim
        )
        .is_err()
    );
    assert!(
        contract::receipt(&receipt, nonce, &git, &contract::hash(b"replacement shim")).is_err()
    );
    Ok(())
}

fn summaries() -> ContractResult {
    let row = finding();
    let text = contract::summary(std::slice::from_ref(&row), "exact introduced commit range");
    assert!(text.contains("| rule-one | src/example.rs | 7 |"));
    assert!(text.contains("**1 potential secrets detected.**"));
    for field in [
        "Secret",
        "Match",
        "Message",
        "CommitMessage",
        "Description",
        "Author",
        "Email",
    ] {
        assert!(
            !text.contains(row[field].as_str().ok_or("Private control missing")?),
            "Summary exposed an excluded finding field"
        );
    }
    let mut escaped = row.clone();
    escaped["RuleID"] = "rule<one>&\"|\n".into();
    escaped["File"] = "src/a|b<&\".rs\r\nnext".into();
    let escaped = contract::summary(&[escaped], "fixed scope");
    for expected in [
        "rule&lt;one&gt;&amp;&quot;&#124;",
        "src/a&#124;b&lt;&amp;&quot;.rs  next",
    ] {
        assert!(escaped.contains(expected));
    }
    assert!(!escaped.contains("rule<one>"));
    assert_eq!(
        escaped
            .lines()
            .filter(|line| line.starts_with("| "))
            .count(),
        3
    );
    let rows = (0..201)
        .map(|index| {
            let mut row = row.clone();
            row["File"] = format!("src/item-{index}.rs").into();
            row
        })
        .collect::<Vec<_>>();
    let bounded = contract::summary(&rows, "fixed scope");
    assert!(bounded.contains("**201 potential secrets detected.**"));
    assert!(bounded.contains("src/item-199.rs"));
    assert!(!bounded.contains("src/item-200.rs"));
    assert!(bounded.contains("first200 locations"));
    assert!(contract::summary(&[], "fixed scope").contains("No matching credentials found"));
    Ok(())
}

fn fixture_pins() -> ContractResult {
    assert_eq!(
        contract::FIXTURES
            .iter()
            .map(|row| row.0)
            .collect::<Vec<_>>(),
        [
            "public-fixture.der",
            "public-fixture.pfx",
            "provenance.json"
        ]
    );
    for (name, hash, size) in contract::FIXTURES {
        let bytes = fs::read(root().join(FIXTURE).join(name))?;
        assert_eq!(bytes.len() as u64, size);
        assert_eq!(contract::hash(&bytes), hash);
    }
    Ok(())
}
pub(super) fn check() -> TestResult {
    (|| -> ContractResult {
        configuration()?;
        sanitizer()?;
        reports()?;
        receipts()?;
        summaries()?;
        fixture_pins()?;
        Ok(())
    })()
    .map_err(|error| io::Error::other(error.to_string()).into())
}
