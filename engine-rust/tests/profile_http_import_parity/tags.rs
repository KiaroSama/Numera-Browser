use numera_core::importers;
#[allow(dead_code)]
#[path = "../common/http_fixture.rs"]
mod http_fixture;
use serde_json::{Value, json};

fn observations(mode: &str) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    let name = match mode {
        "tags" => "import-tags",
        "refusals" => "import-refusals",
        _ => return Err("Unknown importer observation".into()),
    };
    Ok(serde_json::from_value(http_fixture::oracle(name)?)?)
}

fn compare(rows: &[Value], symptom: &str) -> Result<(), Box<dyn std::error::Error>> {
    for (index, case) in rows.iter().enumerate() {
        let raw = case["input"].clone();
        let (dashboard, issues) = importers::profile(&raw, 42, 1000.0)?;
        assert_eq!(dashboard, case["dashboard"], "{symptom}: {index}/dashboard");
        assert_eq!(json!(issues), case["issues"], "{symptom}: {index}/issues");
        let model = match importers::model(&raw, 42, 1000.0) {
            Ok((profile, issues)) => {
                assert_eq!(profile.id.len(), 12);
                assert!(
                    profile
                        .id
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
                );
                assert_eq!(json!(issues), case["issues"]);
                let mut record = serde_json::to_value(profile)?;
                record["id"] = "generated".into();
                json!({"ok":record})
            }
            Err(error) => json!({"error":error}),
        };
        assert_eq!(model, case["model"], "{symptom}: {index}/model");
        assert_eq!(raw, case["input"], "Native importer changed input");
    }
    Ok(())
}

#[test]
fn importer_tags_match_original_whitespace_and_group_display()
-> Result<(), Box<dyn std::error::Error>> {
    let rows = observations("tags")?;
    assert_eq!(rows.len(), 38, "Importer tag observations missing");
    compare(&rows, "Imported tag display differs")
}

#[test]
fn importer_refusals_retain_all_original_ordered_problems() -> Result<(), Box<dyn std::error::Error>>
{
    let rows = observations("refusals")?;
    assert_eq!(rows.len(), 4, "Complete importer refusals missing");
    compare(&rows, "Complete importer refusal differs")
}
