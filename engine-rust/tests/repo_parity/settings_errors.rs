use super::{ProfileRepo, Value, common, fs};
use numera_core::{
    models::Profile,
    storage::{dashboard::DashboardStore, pool::ProxyPool, repo::RepoError},
};
use serde_json::{Map, json};

#[test]
fn storage_settings_errors_match_original_lazy_reads_and_preserve_bytes()
-> Result<(), Box<dyn std::error::Error>> {
    let temporary = common::TempDirectory::new("storage-settings-errors")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "../fixtures/characterization/storage_settings_errors_match_original_lazy_reads_and_preserve_bytes.json"
    ))?;
    assert_eq!(cases.len(), 189);
    for (index, case) in cases.iter().enumerate() {
        let operation = case["operation"].as_str().ok_or("Operation missing")?;
        let root = temporary.path().join(format!("native-{index}"));
        let engine = &case["engine"];
        let dash = &case["dashboard"];
        let row = json!({"id":"proxy00001","host":"127.0.0.1","port":"8080","type":"http",
            "pass":if operation == "pool-update-empty" { Value::Null } else { case["secret"].clone() }});
        let raw: Option<Vec<u8>> = serde_json::from_value(case["raw"].clone())?;
        let inputs = [
            (
                "profiles/fixture00001.json",
                Some(serde_json::to_vec(engine)?),
            ),
            (
                "dashboard/fixture00001.json",
                Some(serde_json::to_vec(dash)?),
            ),
            ("proxies.json", Some(serde_json::to_vec(&json!([row]))?)),
            ("config.json", raw.clone()),
        ];
        for (name, bytes) in &inputs {
            if let Some(bytes) = bytes {
                let path = root.join(name);
                fs::create_dir_all(path.parent().ok_or("Input parent missing")?)?;
                fs::write(path, bytes)?;
            }
        }
        let repo = ProfileRepo::new(root.clone(), None);
        let dashboard = DashboardStore::new(root.clone(), None);
        let pool = ProxyPool::new(root.clone(), None);
        let changes = json!({"future":7})
            .as_object()
            .ok_or("Changes missing")?
            .clone();
        let result = (|| -> Result<Value, RepoError> {
            Ok(match operation {
                "vault" => repo.vault()?.unwrap_or(Value::Null),
                "profile" | "profile-empty" => json!(repo.get("fixture00001")?),
                "dashboard-plain" | "dashboard-envelope" => {
                    dashboard.get("fixture00001")?.unwrap_or(Value::Null)
                }
                "pool-plain" | "pool-envelope" => Value::Array(pool.list()?),
                "pool-withheld" => pool.secret_withheld("proxy00001")?.into(),
                "profile-withheld" => repo.secret_withheld("fixture00001")?.into(),
                "divergence" => json!(repo.divergence("fixture00001", 1700000000.0)?),
                "refuse" | "refuse-missing" => {
                    repo.refuse_unresolved_secret(
                        "fixture00001",
                        Some("fixture-secret"),
                        if operation == "refuse-missing" {
                            None
                        } else {
                            case["secret"].as_str()
                        },
                    )?;
                    Value::Null
                }
                "materialize"
                | "materialize-empty"
                | "materialize-null"
                | "materialize-no-proxy"
                | "materialize-kept"
                | "materialize-kept-envelope" => {
                    json!(repo.materialize("fixture00001", 1700000000.0)?)
                }
                "repo-save" => json!(repo.save(
                    Profile::from_record(engine).map_err(|e| RepoError::Message(e.to_string()))?,
                    1700000000.0
                )?),
                "repo-config" => {
                    json!(repo.save_many(Vec::new(), changes.clone(), "reject", 1700000000.0)?)
                }
                "dashboard-save" | "dashboard-config" => dashboard.save(
                    dash.clone(),
                    if operation == "dashboard-config" {
                        changes.clone()
                    } else {
                        Map::new()
                    },
                    None,
                    Some(if operation == "dashboard-config" {
                        "preserve"
                    } else {
                        "replace"
                    }),
                    1700000000.0,
                )?,
                "pool-update" | "pool-update-empty" => pool
                    .update(
                        "proxy00001",
                        json!({"status":"checked"})
                            .as_object()
                            .ok_or_else(|| RepoError::Message("Patch missing".into()))?
                            .clone(),
                    )?
                    .unwrap_or(Value::Null),
                "pool-delete" | "pool-delete-envelope" => pool.delete("proxy00001")?.into(),
                "pool-add-empty" => Value::Array(pool.add_many(Vec::new())?),
                _ => return Err(RepoError::Message("Unknown test operation".into())),
            })
        })();
        let invalid = matches!(&result, Err(RepoError::Io(error)) if error.kind() == std::io::ErrorKind::InvalidData);
        assert_eq!(
            invalid,
            case["error"] == "UnicodeDecodeError",
            "Storage settings admission differs: operation={operation} case={index}"
        );
        match result {
            Ok(value) => {
                assert!(case["error"].is_null());
                assert_eq!(value, case["result"], "Storage result differs: {operation}");
            }
            Err(error) if !invalid => {
                assert_eq!(Some(error.to_string().as_str()), case["error"].as_str())
            }
            Err(_) => {}
        }
        for (slot, (name, before)) in inputs.iter().enumerate() {
            let after = fs::read(root.join(name)).ok();
            assert_eq!(
                json!(after == *before),
                case["unchanged"][slot],
                "Storage byte preservation differs: {operation} {name}"
            );
            if *name == "config.json" {
                if case["config"].is_null() {
                    assert_eq!(after, raw, "Settings bytes changed");
                } else {
                    assert_eq!(
                        serde_json::from_slice::<Value>(&after.ok_or("Config missing")?)?,
                        case["config"]
                    );
                }
            } else {
                assert_eq!(
                    after
                        .map(|bytes| serde_json::from_slice::<Value>(&bytes))
                        .transpose()?
                        .unwrap_or(Value::Null),
                    case["written"][*name],
                    "Stored result differs: operation={operation} case={index} file={name}"
                );
            }
        }
    }
    temporary.cleanup()?;
    Ok(())
}
