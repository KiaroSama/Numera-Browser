use numera_xtask::ci::{self, workflow};
use serde_json::{Value, json};
use std::{collections::BTreeSet, fs, path::Path};

const COMPOSITES: &[&str] = &[
    "/.github/actions/native-candidate-acceptance",
    "/.github/actions/source-module-checkpoints",
];

fn configuration() -> ci::Result<Value> {
    workflow::parse(&fs::read_to_string(
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.github/dependabot.yml"),
    )?)
}

fn external_action(value: &Value) -> bool {
    match value {
        Value::Object(fields) => {
            fields
                .get("uses")
                .and_then(Value::as_str)
                .is_some_and(|value| {
                    value.contains('@') && !value.starts_with("./") && !value.starts_with("$/")
                })
                || fields.values().any(external_action)
        }
        Value::Array(values) => values.iter().any(external_action),
        _ => false,
    }
}

#[test]
fn every_nested_external_action_manifest_has_a_dependabot_update_entry() -> ci::Result<()> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let configuration = configuration()?;
    workflow::dependency_updates(&configuration)?;
    let updates = configuration["updates"]
        .as_array()
        .ok_or("Update list missing")?;
    let mut found = BTreeSet::new();
    for entry in fs::read_dir(root.join(".github/actions"))? {
        let directory = entry?.path();
        if !directory.is_dir() {
            continue;
        }
        for filename in ["action.yml", "action.yaml"] {
            let path = directory.join(filename);
            if !path.is_file() || !external_action(&workflow::parse(&fs::read_to_string(path)?)?) {
                continue;
            }
            let name = directory
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or("Action name invalid")?;
            let expected = format!("/.github/actions/{name}");
            assert_eq!(
                updates
                    .iter()
                    .filter(|row| row["package-ecosystem"] == "github-actions"
                        && row["directory"] == expected)
                    .count(),
                1,
                "The root workflow scan does not cover {expected}"
            );
            found.insert(expected);
        }
    }
    assert_eq!(
        found,
        COMPOSITES.iter().map(|name| (*name).to_owned()).collect()
    );
    Ok(())
}

#[test]
fn dependency_contract_refuses_missing_disabled_or_ambiguous_composite_entries() -> ci::Result<()> {
    let configuration = configuration()?;
    for directory in COMPOSITES {
        for damage in ["missing", "duplicate", "unknown", "unscheduled", "disabled"] {
            let mut changed = configuration.clone();
            let updates = changed["updates"]
                .as_array_mut()
                .ok_or("Update list missing")?;
            let index = updates
                .iter()
                .position(|row| {
                    row["package-ecosystem"] == "github-actions" && row["directory"] == *directory
                })
                .ok_or("Composite update entry missing")?;
            match damage {
                "missing" => {
                    updates.remove(index);
                }
                "duplicate" => updates.push(updates[index].clone()),
                "unknown" => updates[index]["directory"] = json!("/.github/actions/unreviewed"),
                "unscheduled" => updates[index]["schedule"] = json!({}),
                "disabled" => updates[index]["open-pull-requests-limit"] = json!(0),
                _ => unreachable!(),
            }
            assert!(
                workflow::dependency_updates(&changed).is_err(),
                "Accepted {damage}: {directory}"
            );
        }
    }
    Ok(())
}
