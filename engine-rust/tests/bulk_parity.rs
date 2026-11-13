use numera_core::{bulk, models::Profile};
use serde_json::{Value, json};
#[path = "cli_characterization/mod.rs"]
mod cli_characterization;
mod common;
#[path = "bulk_parity/selection.rs"]
mod selection;
fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .map(|v| v.as_str().expect("Source string list").to_owned())
        .collect()
}

#[test]
fn bulk_generation_patch_and_selection_match_original() -> Result<(), Box<dyn std::error::Error>> {
    let root = common::TempDirectory::new("bulk-source")?;
    let document = cli_characterization::load("bulk_parity", root.path())?;

    root.cleanup()?;
    for case in document["generation"]
        .as_array()
        .ok_or("Bulk generations missing")?
    {
        let options = &case["options"];
        let oses = strings(&options["oses"]);
        let locales = strings(&options["locales"]);
        let versions = strings(&options["chrome_versions"]);
        let tags = strings(&options["tags"]);
        let mut id = 0;
        let profiles = bulk::generate(
            bulk::GenerateBatch {
                count: options["count"].as_u64().ok_or("Count missing")? as usize,
                seed: options["seed"].as_i64().ok_or("Seed missing")?,
                oses: &oses,
                locales: &locales,
                chrome_versions: &versions,
                engine: "playwright",
                prefix: options["name_prefix"].as_str().unwrap_or("Profile"),
                start: options["start_index"].as_i64().unwrap_or(1),
                pad: options["pad"].as_u64().unwrap_or(4) as usize,
                tags: &tags,
                mobile: options["mobile"] == true,
            },
            || {
                let value = format!("fixture{id}");
                id += 1;
                Ok(value)
            },
            1000.0,
        )?;
        assert_eq!(serde_json::to_value(profiles)?, case["expected"]);
    }
    for case in document["changes"]
        .as_array()
        .ok_or("Bulk changes missing")?
    {
        let mut profile = Profile::from_record(&case["input"])?;
        let changed = bulk::apply(&mut profile, &case["patch"], Some(123))?;
        assert_eq!(json!(changed), case["changed"]);
        assert_eq!(serde_json::to_value(profile)?, case["expected"]);
    }
    let profiles = document["profiles"]
        .as_array()
        .ok_or("Selection profiles missing")?
        .iter()
        .map(Profile::from_record)
        .collect::<Result<Vec<_>, _>>()?;
    for case in document["selections"]
        .as_array()
        .ok_or("Bulk selections missing")?
    {
        let options = &case["options"];
        let selected = bulk::select(
            &profiles,
            &strings(&options["refs"]),
            options["tag"].as_str(),
            options["everything"] == true,
        );
        assert_eq!(
            json!(selected.into_iter().map(|p| p.id).collect::<Vec<_>>()),
            case["expected"]
        );
    }
    Ok(())
}
