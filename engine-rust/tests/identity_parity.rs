use numera_core::{
    browser::hints,
    identity::{self, headers, metadata, validate},
    identity_script,
    models::Fingerprint,
};
use serde_json::{Value, json};

#[test]
fn shared_identity_helpers_match_original_versions_platforms_and_consumers()
-> Result<(), Box<dyn std::error::Error>> {
    use sha2::Digest;
    let manifest: Value =
        serde_json::from_str(include_str!("fixtures/characterization/manifest.json"))?;
    let mut names = std::collections::BTreeSet::new();
    for row in manifest["cases"]
        .as_array()
        .ok_or("Characterization manifest missing")?
    {
        let name = row["fixture"]
            .as_str()
            .ok_or("Characterization fixture name missing")?;
        assert!(name.ends_with(".json") && !name.contains(['/', '\\']) && names.insert(name));
        let bytes = std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("tests/fixtures/characterization")
                .join(name),
        )?;
        let digest: String = sha2::Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        assert_eq!(
            digest, row["fixture_sha256"],
            "Frozen source observation bytes changed: {name}"
        );
        assert!(matches!(
            row["source_commit"].as_str(),
            Some(
                "13b6d9c6973281fe123353eaec05b29d65781a2d"
                    | "3bf1d839fd33e42a10b982bd8607a7052ea7c4cd"
            )
        ));
    }
    // Frozen execution of the immutable source; native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/characterization/shared_identity_helpers_match_original_versions_platforms_and_consumers.json"
    ))?;
    assert_eq!(cases.len(), 123, "Shared identity observations missing");
    for (index, case) in cases.iter().enumerate() {
        let input = case["input"].clone();
        let fingerprint = Fingerprint::from_record(&input)?;
        assert_eq!(
            identity::ua_brands(&fingerprint),
            case["brands"],
            "Shared brands differ: {index}"
        );
        assert_eq!(
            identity::sec_ch_ua(&fingerprint),
            case["header"].as_str().ok_or("Header missing")?,
            "Standalone hint header differs: {index}"
        );
        assert_eq!(
            identity::sends_client_hints(&fingerprint),
            case["sends"].as_bool().ok_or("Hint predicate missing")?,
            "Hint predicate differs: {index}"
        );
        assert_eq!(
            identity::full_version(&fingerprint),
            case["full"].as_str().ok_or("Full version missing")?,
            "Full version differs: {index}"
        );
        assert_eq!(
            metadata(&fingerprint)?,
            case["metadata"],
            "Shared metadata differs: {index}"
        );
        assert_eq!(
            headers(&fingerprint)?,
            case["headers"],
            "Shared headers differ: {index}"
        );
        assert_eq!(
            identity_script::config(&fingerprint)?,
            case["config"],
            "Shared page config differs: {index}"
        );
        let policy = hints::policy(&fingerprint)?;
        assert_eq!(
            policy["commands"][0]["payload"]["userAgentMetadata"], case["metadata"],
            "CDP metadata differs: {index}"
        );
        let sends = case["sends"].as_bool().ok_or("Hint predicate missing")?;
        assert_eq!(
            policy["strip_headers_prefixes"],
            if sends {
                json!([])
            } else {
                json!(["sec-ch-ua"])
            },
            "CDP hint omission differs: {index}"
        );
        assert_eq!(
            policy["commands"]
                .as_array()
                .ok_or("CDP commands missing")?
                .len(),
            if sends { 1 } else { 2 }
        );
        assert_eq!(
            serde_json::to_value(&fingerprint)?,
            input,
            "Native identity input changed: {index}"
        );
    }
    Ok(())
}

#[test]
fn identity_diagnostics_and_hints_match_source() -> Result<(), Box<dyn std::error::Error>> {
    let document: Value = serde_json::from_str(include_str!("fixtures/identity.json"))?;
    let cases = document["cases"]
        .as_array()
        .ok_or("Missing identity contract fixtures")?;
    assert!(
        !cases.is_empty(),
        "Identity contract oracle must not be empty"
    );
    for case in cases {
        let fingerprint = Fingerprint::from_record(&case["input"])?;
        let actual = json!({"metadata": metadata(&fingerprint)?, "headers": headers(&fingerprint)?, "issues": validate(&fingerprint)?});
        assert_eq!(
            actual, case["expected"],
            "Identity contract {}",
            case["name"]
        );
    }
    Ok(())
}
