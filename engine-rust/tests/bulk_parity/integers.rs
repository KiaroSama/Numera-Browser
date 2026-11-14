use super::{Value, bulk, common, json};
use numera_core::{
    cli,
    ownership::registry::Registry,
    storage::{config::ConfigStore, repo::ProfileRepo},
};
use std::{collections::BTreeMap, fs, time::Duration};

pub(super) fn check() -> Result<(), Box<dyn std::error::Error>> {
    use sha2::Digest;
    let raw = include_str!("../fixtures/cli-characterization/bulk-integer-contract.json");
    let contract: Value = serde_json::from_str(raw)?;
    let provenance: Value = serde_json::from_str(include_str!(
        "../fixtures/cli-characterization-manifest.json"
    ))?;
    let hash = sha2::Sha256::digest(raw.as_bytes())
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect::<String>();
    assert_eq!(hash, provenance["bulk_integer_contract"]["sha256"]);
    for case in contract["cases"]
        .as_array()
        .ok_or("Integer bulk observations missing")?
    {
        let temporary = common::TempDirectory::new("bulk-integer-contract")?;
        let root = temporary.path();
        let repo = ProfileRepo::open(root.into(), None, None)?;
        let config = ConfigStore::new(root.into());
        config.set_many(serde_json::Map::from_iter([(
            "default_engine".into(),
            "playwright".into(),
        )]))?;
        let before = fs::read(root.join("config.json"))?;
        let installed = BTreeMap::from([("playwright".into(), true)]);
        let sessions = Registry::new(root.into(), "fixture-host".into(), None)?;
        let application = cli::application::Application {
            root,
            home: root,
            repo: &repo,
            installed: &installed,
            host: "fixture-host",
            password: None,
            environment_token: false,
            sessions: &sessions,
            browser: None,
        };
        let argv = [
            "numera",
            "bulk-create",
            "--count",
            case["count"].as_str().ok_or("Count text missing")?,
            "--start",
            case["start"].as_str().ok_or("Start text missing")?,
            "--pad",
            case["pad"].as_str().ok_or("Padding text missing")?,
            "--seed",
            "42",
            "--prefix",
            "Wide",
            "--json",
        ];
        let mut registry = cli::registry();
        registry.build();
        let parsed = cli::parse(argv)?;
        let args = cli::reflection::values(&registry, &parsed);
        let result = application.execute(
            args.as_object().ok_or("Bulk arguments missing")?,
            1000.0,
            |_| panic!("Bulk fixture prompted"),
        );
        if let Some(message) = case["error"].as_str() {
            assert_eq!(case["error_type"], "OverflowError");
            assert_eq!(
                result
                    .err()
                    .ok_or("Overflowed padding was accepted")?
                    .to_string(),
                message
            );
            assert!(repo.list(1000.0)?.is_empty());
        } else {
            let expected = case["profiles"]
                .as_array()
                .ok_or("Expected profiles missing")?;
            let outcome = result?;
            assert_eq!(outcome.exit, 0);
            assert_eq!(outcome.payload["created"], expected.len());
            let ids = outcome.payload["ids"]
                .as_array()
                .ok_or("Created IDs missing")?;
            for (index, (id, expected)) in ids.iter().zip(expected).enumerate() {
                let profile = repo
                    .get(id.as_str().ok_or("Created ID not text")?)?
                    .ok_or("Created record missing")?;
                let mut actual = serde_json::to_value(&profile)?;
                actual["id"] = format!("fixture{index}").into();
                assert_eq!(&actual, expected);
                let twin: Value = serde_json::from_slice(&fs::read(
                    root.join("dashboard").join(format!("{}.json", profile.id)),
                )?)?;
                assert_eq!(twin["name"], expected["name"]);
            }
            let mut index = 0;
            let generated = bulk::generate(
                bulk::GenerateBatch {
                    count: expected.len(),
                    seed: 42i64,
                    oses: &[],
                    locales: &[],
                    chrome_versions: &[],
                    engine: "playwright",
                    prefix: "Wide",
                    start: case["start"]
                        .as_str()
                        .ok_or("Start missing")?
                        .parse::<num_bigint::BigInt>()?,
                    pad: case["pad"]
                        .as_str()
                        .and_then(|v| v.parse::<isize>().ok())
                        .unwrap_or(0)
                        .max(0) as usize,
                    tags: &[],
                    mobile: false,
                },
                || {
                    let id = format!("fixture{index}");
                    index += 1;
                    Ok(id)
                },
                1000.0,
            )?;
            assert_eq!(serde_json::to_value(generated)?, case["profiles"]);
        }
        assert_eq!(fs::read(root.join("config.json"))?, before);
        assert!(sessions.shutdown(Duration::from_secs(5)).is_empty());
        drop(sessions);
        temporary.cleanup()?;
    }
    // An impossible positive allocation is refused before creating profiles;
    // arbitrary negative counts above are already clamped before conversion.
    let temporary = common::TempDirectory::new("bulk-allocation-refusal")?;
    let root = temporary.path();
    let repo = ProfileRepo::open(root.into(), None, None)?;
    let config = ConfigStore::new(root.into());
    let installed = BTreeMap::new();
    let profiles = cli::profile_commands::Profiles {
        repo: &repo,
        config: &config,
        installed: &installed,
        host: "fixture-host",
    };
    let impossible: Value = serde_json::from_str("340282366920938463463374607431768211456")?;
    assert!(
        profiles
            .bulk_create(
                json!({"count":impossible,"seed":42})
                    .as_object()
                    .ok_or("Allocation arguments missing")?,
                1000.0
            )
            .is_err()
    );
    assert!(repo.list(1000.0)?.is_empty());
    temporary.cleanup()?;
    Ok(())
}
