use numera_core::{
    cli::{self, profile_commands::Profiles},
    fingerprint::{GenerateOptions, generate},
    models::Profile,
    storage::{config::ConfigStore, repo::ProfileRepo},
};
use serde_json::{Number, Value, json};
use std::{collections::BTreeMap, fs};
mod common;

#[test]
fn complete_integer_seed_identities_and_commands_match_original()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("integer-seed")?;
    // Frozen execution of the immutable source; native assertions below remain live.
    let document: Value = serde_json::from_str(include_str!(
        "fixtures/characterization/complete_integer_seed_identities_and_commands_match_original.json"
    ))?;
    let installed = BTreeMap::from([("playwright".into(), true)]);
    let mut registry = cli::registry();
    registry.build();
    let commands = document["commands"].as_array().ok_or("Commands missing")?;
    assert_eq!(commands.len(), 26);
    let mut refused = 0;
    for (index, case) in commands.iter().enumerate() {
        let root = temporary.path().join(format!("native-{index}"));
        let repo = ProfileRepo::open(root.clone(), None, None)?;
        let config = ConfigStore::new(root.clone());
        let config_path = root.join("config.json");
        let initial = b"{\"default_engine\":\"camoufox\",\"extra\":{\"keep\":true}}";
        fs::write(&config_path, initial)?;
        let services = Profiles {
            repo: &repo,
            config: &config,
            installed: &installed,
            host: "fixture",
        };
        let argv = case["argv"]
            .as_array()
            .ok_or("Arguments missing")?
            .iter()
            .map(|v| v.as_str().ok_or("Argument not text"))
            .collect::<Result<Vec<_>, _>>()?;
        let matches = registry
            .clone()
            .try_get_matches_from(std::iter::once("numera").chain(argv))?;
        let values = cli::reflection::values(&registry, &matches);
        let args = values.as_object().ok_or("Parsed arguments missing")?;
        let result = if values["command"] == "create" {
            services.create(args, 1000.0)
        } else {
            services.bulk_create(args, 1000.0)
        };
        let mut payload = if case["error"].is_null() {
            // This assertion must reject the historical bulk i64 guard.
            assert!(
                result.is_ok(),
                "Integer seed command admission differs: {}",
                case["argv"]
            );
            let outcome = result?;
            assert_eq!(
                i64::from(outcome.exit),
                case["exit"].as_i64().ok_or("Exit missing")?
            );
            outcome.payload
        } else {
            refused += 1;
            assert_eq!(case["error"]["type"], "WriteRefused");
            let message = match result {
                Err(error) => error.to_string(),
                Ok(_) => panic!("Invalid persisted seed was accepted"),
            };
            assert_eq!(
                message, case["error"]["message"],
                "Integer seed write refusal"
            );
            Value::Null
        };
        // Observe the write before any repository read can reconcile its twins.
        let mut records = json!({});
        for kind in ["profiles", "dashboard"] {
            let path = root.join(kind);
            let mut rows = Vec::<Value>::new();
            if path.exists() {
                for entry in fs::read_dir(path)? {
                    let path = entry?.path();
                    if path.extension().is_some_and(|ext| ext == "json") {
                        rows.push(serde_json::from_slice(&fs::read(path)?)?);
                    }
                }
            }
            rows.sort_by_key(|record| record["name"].to_string());
            records[kind] = Value::Array(rows);
        }
        let ids = records["profiles"]
            .as_array()
            .ok_or("Written records missing")?
            .iter()
            .enumerate()
            .map(|(index, record)| {
                record["id"]
                    .as_str()
                    .map(|id| (id.to_owned(), format!("fixture{index}")))
                    .ok_or("Written profile ID missing")
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        normalize_ids(&mut payload, &ids);
        normalize_ids(&mut records, &ids);
        assert_eq!(payload, case["payload"], "Integer seed command payload");
        assert_eq!(records, case["records"], "Integer seed persisted records");
        assert_eq!(Value::Object(config.read()?), case["config"]);
        assert_eq!(
            json!(fs::read(config_path)? == initial),
            case["configUnchanged"]
        );
    }
    assert_eq!(refused, 8, "Original write-policy boundary coverage");
    let identities = document["identities"]
        .as_array()
        .ok_or("Identities missing")?;
    assert_eq!(identities.len(), 14);
    for case in identities {
        let seed: Number = serde_json::from_value(case["seed"].clone())?;
        let fingerprint = generate(GenerateOptions {
            seed,
            os: Some("windows"),
            locale: Some("en-US"),
            country: None,
            mobile: false,
        })?;
        assert_eq!(
            serde_json::to_value(&fingerprint)?,
            case["fingerprint"],
            "Complete integer identity"
        );
        assert_eq!(
            numera_core::identity_script::config(&fingerprint)?,
            case["config"]
        );
        let profile = Profile::from_record(&json!({"id":"fixture0", "name":"Fixture",
            "fingerprint":fingerprint, "created_at":1000.0,"updated_at":1000.0}))?;
        assert_eq!(serde_json::to_value(&profile)?, case["profile"]);
        let dashboard = numera_core::conversion::to_dashboard(&profile);
        assert_eq!(dashboard, case["dashboard"]);
        assert_eq!(
            serde_json::to_value(numera_core::conversion::to_engine(&dashboard, 1000.0)?)?,
            case["converted"]
        );
        let mut twin = profile.clone();
        twin.id = "fixture1".into();
        twin.fingerprint.seed =
            serde_json::from_value(case["collisions"][0]["members"][1]["seed"].clone())?;
        assert_eq!(
            json!(numera_core::collision::find(&[
                profile.clone(),
                twin.clone()
            ])),
            case["collisions"]
        );
        twin.fingerprint.seed = profile.fingerprint.seed.clone();
        assert_eq!(
            json!(numera_core::collision::find(&[profile, twin])),
            case["identical"]
        );
    }
    for text in ["1.0", "1e20", "-0.5"] {
        let seed: Number = serde_json::from_str(text)?;
        assert!(
            generate(GenerateOptions {
                seed,
                os: None,
                locale: None,
                country: None,
                mobile: false,
            })
            .is_err()
        );
    }
    temporary.cleanup()?;
    Ok(())
}

fn normalize_ids(value: &mut Value, ids: &BTreeMap<String, String>) {
    match value {
        Value::String(text) => {
            if let Some(replacement) = ids.get(text) {
                *text = replacement.clone();
            }
        }
        Value::Array(items) => items.iter_mut().for_each(|v| normalize_ids(v, ids)),
        Value::Object(items) => items.values_mut().for_each(|v| normalize_ids(v, ids)),
        _ => {}
    }
}
