use numera_core::{
    models::{Fingerprint, Profile},
    proxy::Proxy,
};
use serde_json::{Value, json};

#[test]
fn models_and_proxy_intent_match_source_oracle() -> Result<(), Box<dyn std::error::Error>> {
    let document: Value = serde_json::from_str(include_str!("fixtures/models.json"))?;
    let cases = document["cases"]
        .as_array()
        .ok_or("Fixture cases must be an array")?;
    assert!(!cases.is_empty(), "Source oracle must not be empty");
    for case in cases {
        let actual = match case["operation"]
            .as_str()
            .ok_or("Missing fixture operation")?
        {
            "fingerprint" => serde_json::to_value(Fingerprint::from_record(&case["input"])?)?,
            "profile" => serde_json::to_value(Profile::from_record(&case["input"])?)?,
            "proxy" => {
                let mut proxy = Proxy::from_record(
                    &case["input"],
                    case["stored"].as_bool().ok_or("Missing storage origin")?,
                )?;
                if case["assignment"] != "untouched" {
                    proxy.set_password(case["assignment"].as_str().map(str::to_owned));
                }
                json!({"record": proxy.to_record(), "intent": proxy.password_intent(), "playwright": proxy.to_playwright()})
            }
            other => return Err(format!("Unknown oracle operation: {other}").into()),
        };
        assert_eq!(
            actual, case["expected"],
            "Source parity case {}",
            case["name"]
        );
    }
    Ok(())
}

#[test]
fn optional_proxy_reads_match_original_without_rewriting_storage()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::{ownership::capture, storage::repo::ProfileRepo};
    use std::{fs, path::Path, process::Command, time::Duration};

    let temporary = common::TempDirectory::new("optional proxy α")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Source interpreter required")?,
    );
    command
        .args([
            "-I",
            "-c",
            r#"
import json,sys
from pathlib import Path
repository,root=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera.fingerprint import generate
from numera.models import Profile
from numera.store import ProfileStore
from run_logging import run_log
with run_log('optional-proxy-oracle',root/'logs') as log:
    variants=[('missing',None),('null',None),('false',False),('integer-zero',0),
              ('float-zero',0.0),('negative-zero',-0.0),('empty-string',''),
              ('empty-list',[]),('empty-map',{}),
              ('valid',{'server':'http://127.0.0.1:8080'}),
              ('true',True),('nonzero',1),('text','invalid'),
              ('nonempty-list',[0]),('invalid-map',{'unexpected':True})]
    fingerprint=generate(seed=42,os='windows').to_dict()
    store=ProfileStore(root)
    observed=[]
    for index,(name,proxy) in enumerate(variants):
        record={'id':f'fixture{index:02d}','name':name,'fingerprint':fingerprint,
                'created_at':1700000000.0,'updated_at':1700000001.0}
        if name!='missing':
            record['proxy']=proxy
        path=root/'profiles'/f"{record['id']}.json"
        path.write_text(json.dumps(record,ensure_ascii=True),encoding='utf-8')
        before=path.read_bytes()
        row={'name':name,'input':record}
        for key,read in [('model',lambda:Profile.from_dict(record)),
                         ('stored',lambda:store.get(record['id']))]:
            try:
                row[key]=read().to_dict()
                row[key+'_error']=False
            except (TypeError,AttributeError):
                row[key]=None
                row[key+'_error']=True
        row['unchanged']=path.read_bytes()==before
        observed.append(row)
    log.info('Observed optional proxy cases=%s',len(observed))
    print(json.dumps(observed,ensure_ascii=True))
"#,
        ])
        .arg(repository)
        .arg(temporary.path().join("original"))
        .env("PYTHONUTF8", "1");
    let captured = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 131072,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(captured.exit, 0, "Original optional-proxy observer failed");
    let cases: Vec<Value> = serde_json::from_slice(&captured.stdout)?;
    assert_eq!(cases.len(), 15, "Original proxy observations missing");
    let root = temporary.path().join("native");
    fs::create_dir_all(root.join("profiles"))?;
    let repo = ProfileRepo::new(root.clone(), None);
    for case in cases {
        let input = &case["input"];
        let id = input["id"].as_str().ok_or("Profile ID missing")?;
        let path = root.join("profiles").join(format!("{id}.json"));
        let before = serde_json::to_vec(input)?;
        fs::write(&path, &before)?;
        let model = Profile::from_record(input);
        assert_eq!(
            model.is_err(),
            case["model_error"] == true,
            "{} model verdict",
            case["name"]
        );
        if let Ok(profile) = model {
            assert_eq!(
                serde_json::to_value(profile)?,
                case["model"],
                "{} model",
                case["name"]
            );
        }
        let stored = repo.get(id);
        assert_eq!(
            stored.is_err(),
            case["stored_error"] == true,
            "{} read verdict",
            case["name"]
        );
        if let Ok(profile) = stored {
            assert_eq!(
                serde_json::to_value(profile)?,
                case["stored"],
                "{} read",
                case["name"]
            );
        }
        assert_eq!(
            case["unchanged"], true,
            "Original read rewrote {}",
            case["name"]
        );
        assert_eq!(
            fs::read(path)?,
            before,
            "Native read rewrote {}",
            case["name"]
        );
    }
    temporary.cleanup()?;
    Ok(())
}

fn policy_gaps(record: &Value, table: &Value) -> Result<Value, Box<dyn std::error::Error>> {
    use std::collections::BTreeSet;

    let record = record
        .as_object()
        .ok_or("Serialized model must be an object")?;
    let table = table.as_object().ok_or("Field policy must be an object")?;
    let fields: BTreeSet<_> = record.keys().collect();
    let rules: BTreeSet<_> = table.keys().collect();
    let unexplained: BTreeSet<_> = table
        .iter()
        .filter(|(_, rule)| {
            matches!(rule["kind"].as_str(), Some("owned" | "normalized"))
                && rule["why"].as_str().is_none_or(|why| {
                    why.chars()
                        .all(|c| c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}'))
                })
        })
        .map(|(name, _)| name)
        .collect();
    Ok(json!({
        "unpoliced": fields.difference(&rules).collect::<Vec<_>>(),
        "phantom": rules.difference(&fields).collect::<Vec<_>>(),
        "unexplained": unexplained,
    }))
}

#[test]
fn serialized_model_fields_have_exact_explained_source_policies()
-> Result<(), Box<dyn std::error::Error>> {
    let oracle: Value = serde_json::from_str(include_str!("fixtures/models.json"))?;
    let case = oracle["cases"]
        .as_array()
        .ok_or("Model cases missing")?
        .iter()
        .find(|case| case["operation"] == "profile")
        .ok_or("Original profile fixture missing")?;
    let profile = Profile::from_record(&case["input"])?;
    let proxy = profile
        .proxy
        .as_ref()
        .ok_or("Original proxy fixture missing")?;
    let record = serde_json::to_value(&profile)?;
    assert_eq!(
        serde_json::to_value(proxy)?,
        proxy.to_record(),
        "Proxy intent metadata leaked into storage"
    );
    let policy: Value = serde_json::from_str(include_str!("../src/field_policy.json"))?;
    let empty = json!({"unpoliced": [], "phantom": [], "unexplained": []});
    for (name, record) in [
        ("Profile", &record),
        ("Fingerprint", &record["fingerprint"]),
        ("Proxy", &record["proxy"]),
    ] {
        assert_eq!(
            policy_gaps(record, &policy["policy"]["tables"][name])?,
            empty,
            "Serialized {name} field policy differs"
        );
    }
    Ok(())
}

#[test]
fn policy_coverage_guard_detects_new_removed_and_unexplained_fields()
-> Result<(), Box<dyn std::error::Error>> {
    let record = json!({"new_field": null, "owned": null, "normalized": null});
    for reason in [
        "",
        " \t\r\n",
        "\u{1c}\u{1d}\u{1e}\u{1f}",
        "\u{85}\u{a0}\u{2003}\u{3000}",
    ] {
        let policy = json!({
            "gone": {"kind": "text"},
            "owned": {"kind": "owned", "why": reason},
            "normalized": {"kind": "normalized", "why": reason},
        });
        assert_eq!(
            policy_gaps(&record, &policy)?,
            json!({
                "unpoliced": ["new_field"],
                "phantom": ["gone"],
                "unexplained": ["normalized", "owned"],
            })
        );
    }
    assert_eq!(
        policy_gaps(
            &json!({"owned": null}),
            &json!({"owned": {"kind": "owned", "why": "Stamped by writer"}})
        )?,
        json!({"unpoliced": [], "phantom": [], "unexplained": []})
    );
    Ok(())
}

mod common;
