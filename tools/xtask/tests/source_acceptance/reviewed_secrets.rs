//! Independent reviewed-policy assertions shared by the existing three cases.
use super::*;

const TEMPLATE_SHA256: &str = "e8bacdc1cbd76ae1495ef4ec5a917030761093f793dc4a89bb512671600c5ca6";
const FIXTURE: &str = "tools/xtask/tests/fixtures/windows-signing";
const FIXTURE_PINS: [(&str, &str, u64); 3] = [
    (
        "public-fixture.der",
        "8fedb7633c7d447b6607375392c9bb02b549ed7c5fbaf7b562312f50d1edd7db",
        802,
    ),
    (
        "public-fixture.pfx",
        "4bc4ea7fa927262945f0eed6418d1a60b7f43e69d773007c79e6c7d129480782",
        2563,
    ),
    (
        "provenance.json",
        "9373b346cbc88ab826f9fb59d526c96f3cd3aa720fa5a513958cdbfb632400ae",
        937,
    ),
];
struct Expected {
    rule: &'static str,
    path: &'static str,
    accepted: &'static [&'static str],
    hashes: &'static [&'static str],
}
const GROUPS: [Expected; 10] = [
    Expected {
        rule: "generic-api-key",
        path: r"(^|/)engine-rust/tests/fixtures/(request-auth|token-store)\.json$",
        accepted: &[
            "engine-rust/tests/fixtures/request-auth.json",
            "engine-rust/tests/fixtures/token-store.json",
        ],
        hashes: &["f577f82af741d32085294020c093c7decd65544fe2f43c88bc76768245a5b409"],
    },
    Expected {
        rule: "generic-api-key",
        path: r"(^|/)engine-rust/src/http/documentation-assets/provenance\.json$",
        accepted: &["engine-rust/src/http/documentation-assets/provenance.json"],
        hashes: &[
            "ac215b3e0151a72e3e3e5b898b61827612735ed8fdf7865d7a4c90959a242e03",
            "4c0b89ca29dd4fd13eeefc41a69fee0b64d8244e5fc8e2654ac31ad7c96fc426",
        ],
    },
    Expected {
        rule: "generic-api-key",
        path: r"(^|/)engine/tests/test_portable_root\.py$",
        accepted: &["engine/tests/test_portable_root.py"],
        hashes: &["7e65f47df01dbc8677d2ced7c8ad02a06afbb21e07c1e872713d2bfaf8773128"],
    },
    Expected {
        rule: "generic-api-key",
        path: r"(^|/)desktop/src-tauri/build\.rs$",
        accepted: &["desktop/src-tauri/build.rs"],
        hashes: &["bf23e95aa0ff67388daa3728f36a013b9ddafdc2bd280decb03bacbaad9eb8a1"],
    },
    Expected {
        rule: "generic-api-key",
        path: r"(^|/)docs/native\-storage\-vault\-retirement\-map\.json$",
        accepted: &["docs/native-storage-vault-retirement-map.json"],
        hashes: &[
            "5200584a37e3b55bf316b18b7d5ed8bda0b94b5992e25c81c24feff4534f23ea",
            "d3ea574c1b277e2f9ad40d1958c72d20dc650dcb3d53825b8411f98cb620418a",
        ],
    },
    Expected {
        rule: "generic-api-key",
        path: r"(^|/)docs/source\-vault\-test\-map\.json$",
        accepted: &["docs/source-vault-test-map.json"],
        hashes: &[
            "85fb2a4998c93c78f92db8b0d1b7f19f40e1c27586c945d648f1fb703d30df2d",
            "73d5042a4513aad7615d2529e4c36bd1e7a4b0d4843a775942f706c24b2a8072",
        ],
    },
    Expected {
        rule: "generic-api-key",
        path: r"(^|/)docs/source\-http\-test\-map\.json$",
        accepted: &["docs/source-http-test-map.json"],
        hashes: &[
            "16832dd950a0133b4693539494885b9fa7bc082317ea789eb8beb70e32e7e9e4",
            "edf859b0586e67111df091c409f942cb7b83617014de8b8a103067eb9e2653dd",
            "2aba318639804e79742a6aae9f6b335a80c3ce3b3e370bc1e9c6b09563e29f13",
            "5b0944d68ae2c8e4c12f3db24e80230d7b35d8635ddd517b90373005c786e9b4",
            "6745a2d963dcc799827c9e5bdfd1aefd82c23c289da226c65de074fbebcd12d8",
        ],
    },
    Expected {
        rule: "generic-api-key",
        path: r"(^|/)engine\-rust/tests/source_storage/portable_root\.rs$",
        accepted: &["engine-rust/tests/source_storage/portable_root.rs"],
        hashes: &["7e65f47df01dbc8677d2ced7c8ad02a06afbb21e07c1e872713d2bfaf8773128"],
    },
    Expected {
        rule: "generic-api-key",
        path: r"(^|/)tools/xtask/src/ci/history\-cases\.json$",
        accepted: &["tools/xtask/src/ci/history-cases.json"],
        hashes: &[
            "0a4574fd16ed9c2e49f193283bc37806d92e33a00b59573167ac6cefc01cae76",
            "d460e14c4fc53d547c77d74c58dd50f78fcd759feb3cf219e1d501eae9c950cb",
        ],
    },
    Expected {
        rule: "pkcs12-file",
        path: r"(^|/)tools/xtask/tests/fixtures/windows-signing/public-fixture\.pfx$",
        accepted: &["tools/xtask/tests/fixtures/windows-signing/public-fixture.pfx"],
        hashes: &[],
    },
];

pub(super) fn template(root: &Path) -> Result<String> {
    let config = fs::read_to_string(root.join("scripts/ci/gitleaks.toml"))?;
    assert_eq!(ci::sha(config.as_bytes()), TEMPLATE_SHA256);
    assert_eq!(scope::REVIEWED_RULES_SHA256, TEMPLATE_SHA256);
    Ok(config)
}

/// Read the deliberately restricted reviewed TOML shape; compare every rule,
/// path and literal digest against independent expectations, including PFX's
/// intentional path-only shape. The full template also has a separate pin.
pub(super) fn groups(config: &str) -> Result<Vec<Vec<String>>> {
    assert!(!config.contains("disabledRules"));
    assert!(!config.contains("[allowlist]"));
    assert!(!config.contains("[[allowlists]]"));
    let mut rules = Vec::new();
    let mut rule = String::new();
    let mut rule_section = false;
    let mut parsed: Vec<(String, BTreeMap<String, String>)> = Vec::new();
    for line in config
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
    {
        if line == "[[rules]]" {
            rule_section = true;
            continue;
        }
        if line == "[[rules.allowlists]]" {
            parsed.push((rule.clone(), BTreeMap::new()));
            rule_section = false;
            continue;
        }
        if rule_section {
            if let Some(value) = line.strip_prefix("id = ") {
                rule = serde_json::from_str(value)?;
                rules.push(rule.clone());
            }
        } else if let Some((_, fields)) = parsed.last_mut() {
            let (key, value) = line
                .split_once(" = ")
                .ok_or("Unexpected allowlist syntax")?;
            assert!(
                [
                    "description",
                    "condition",
                    "regexTarget",
                    "paths",
                    "regexes"
                ]
                .contains(&key)
            );
            assert!(
                fields.insert(key.into(), value.into()).is_none(),
                "Duplicate allowlist field"
            );
        }
    }
    assert_eq!(rules, ["generic-api-key", "pkcs12-file"]);
    assert_eq!(parsed.len(), GROUPS.len());
    let literal = regex::Regex::new(r"\x27{3}([^\x27]+)\x27{3}")?;
    let mut all = Vec::new();
    for ((rule, fields), expected) in parsed.iter().zip(&GROUPS) {
        assert_eq!(rule, expected.rule);
        assert_eq!(fields.get("condition").map(String::as_str), Some("\"AND\""));
        assert!(
            fields
                .get("description")
                .is_some_and(|value| value.starts_with('"') && value.ends_with('"'))
        );
        let paths = literal
            .captures_iter(fields.get("paths").ok_or("Path selector missing")?)
            .map(|c| c[1].to_owned())
            .collect::<Vec<_>>();
        assert_eq!(paths, [expected.path]);
        let path = regex::Regex::new(expected.path)?;
        for accepted in expected.accepted {
            assert!(path.is_match(accepted));
            assert!(path.is_match(&format!("checkout/{accepted}")));
            let (parent, name) = accepted
                .rsplit_once('/')
                .ok_or("Expected path has no parent")?;
            let (_, extension) = name
                .rsplit_once('.')
                .ok_or("Expected path has no extension")?;
            assert!(!path.is_match(&format!("{parent}/unreviewed.{extension}")));
            assert!(!path.is_match(&format!("{accepted}.backup")));
            assert!(!path.is_match(&format!("{accepted}/child")));
            assert!(!path.is_match(&accepted.to_ascii_uppercase()));
        }
        for other in GROUPS.iter().filter(|other| other.path != expected.path) {
            assert!(!path.is_match(other.accepted[0]));
        }
        let patterns = fields
            .get("regexes")
            .map(|line| {
                literal
                    .captures_iter(line)
                    .map(|c| c[1].to_owned())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        assert_eq!(patterns.len(), expected.hashes.len());
        if expected.hashes.is_empty() {
            assert_eq!(
                fields.len(),
                3,
                "PFX exception must rely on the independent byte/history gates"
            );
            assert!(!fields.contains_key("regexTarget"));
            assert!(!fields.contains_key("regexes"));
        } else {
            assert_eq!(fields.len(), 5);
            assert_eq!(
                fields.get("regexTarget").map(String::as_str),
                Some("\"secret\"")
            );
        }
        for (pattern, expected_hash) in patterns.iter().zip(expected.hashes) {
            let body = pattern
                .strip_prefix('^')
                .and_then(|p| p.strip_suffix('$'))
                .ok_or("Unanchored exception")?;
            let value = body.replace(r"\-", "-");
            assert!(
                value
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            );
            assert_eq!(ci::sha(value.as_bytes()), *expected_hash);
            let regex = regex::Regex::new(pattern)?;
            assert!(regex.is_match(&value));
            assert!(!regex.is_match(&format!("{value}changed")));
            assert!(!regex.is_match(&format!("changed{value}")));
            assert!(!regex.is_match(&format!("{value}\n")));
        }
        all.push(patterns);
    }
    Ok(all)
}

fn install(root: &Path, source: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for (name, hash, length) in FIXTURE_PINS {
        let path = root.join(FIXTURE).join(name);
        fs::create_dir_all(path.parent().ok_or("Fixture parent missing")?)?;
        let data = fs::read(source.join(FIXTURE).join(name))?;
        assert_eq!(data.len() as u64, length);
        assert_eq!(ci::sha(&data), hash);
        fs::write(&path, data)?;
        files.push(path);
    }
    Ok(files)
}
fn git(root: &Path, args: &[&str]) -> Result<()> {
    ci::git(root, args).map(|_| ())
}
fn commit(root: &Path, message: &str) -> Result<()> {
    git(
        root,
        &[
            "-c",
            "user.name=Reviewed fixture control",
            "-c",
            "user.email=fixture@example.invalid",
            "-c",
            "commit.gpgSign=false",
            "commit",
            "--quiet",
            "-m",
            message,
        ],
    )
}

pub(super) fn signing_guards(source: &Path) -> Result<()> {
    let t = Temp::new()?;
    let root = t.path();
    let files = install(root, source)?;
    assert_eq!(scope::check_utf8(&files)?, 3);
    let bytes = fs::read(&files[1])?;
    let record = fs::read(&files[2])?;
    let mut changed = bytes.clone();
    changed[0] ^= 1;
    fs::write(&files[1], &changed)?;
    assert!(scope::check_utf8(&files).is_err());
    let mut mutable: Value = serde_json::from_slice(&record)?;
    mutable["files"]["public-fixture.pfx"]["sha256"] = ci::sha(&changed).into();
    fs::write(&files[2], serde_json::to_vec(&mutable)?)?;
    assert!(
        scope::check_utf8(&files).is_err(),
        "Mutable provenance must not rewrite native pins"
    );
    fs::write(&files[1], &bytes)?;
    fs::write(&files[2], &record)?;
    assert_eq!(scope::check_utf8(&files)?, 3);
    git(root, &["init", "--quiet"])?;
    assert!(
        scope::history_signing_material(root).is_err(),
        "Unborn history is incomplete"
    );
    git(root, &["add", "--", FIXTURE])?;
    commit(root, "Approved public fixture")?;
    assert_eq!(scope::history_signing_material(root)?, 3);
    let head = ci::git(root, &["rev-parse", "HEAD"])?;
    fs::write(root.join(".git/shallow"), head)?;
    assert!(scope::history_signing_material(root).is_err());
    fs::remove_file(root.join(".git/shallow"))?;
    let foreign = root.join("unreviewed/private.pfx");
    fs::create_dir_all(foreign.parent().ok_or("Foreign fixture parent missing")?)?;
    fs::write(&foreign, &bytes)?;
    git(root, &["add", "--", "unreviewed/private.pfx"])?;
    let destination = root.join("refused-staging");
    assert!(scope::stage_tracked(root, &destination).is_err());
    assert!(!destination.exists());
    git(
        root,
        &["rm", "--cached", "--quiet", "--", "unreviewed/private.pfx"],
    )?;
    fs::write(&files[1], &changed)?;
    git(root, &["add", "--", FIXTURE])?;
    commit(root, "Unreviewed historical payload")?;
    fs::write(&files[1], &bytes)?;
    git(root, &["add", "--", FIXTURE])?;
    commit(root, "Restore approved payload")?;
    assert!(
        scope::history_signing_material(root).is_err(),
        "Restoration must not hide historical bytes"
    );
    git(root, &["rm", "--quiet", "-r", "--", FIXTURE])?;
    commit(root, "Delete current public fixtures")?;
    assert!(
        scope::history_signing_material(root).is_err(),
        "Deletion must not hide historical bytes"
    );
    Ok(())
}
