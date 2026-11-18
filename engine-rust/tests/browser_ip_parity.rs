use numera_core::{browser::ip_identity, models::Profile};
use serde_json::Value;

#[test]
fn only_marked_identity_signals_follow_observed_exit_country()
-> Result<(), Box<dyn std::error::Error>> {
    let document: Value = serde_json::from_str(include_str!("fixtures/browser-ip.json"))?;
    let cases = document["cases"]
        .as_array()
        .ok_or("IP identity cases missing")?;
    assert_eq!(cases.len(), 33);
    for case in cases {
        let profile = Profile::from_record(&case["input"])?;
        assert_eq!(
            u64::from(ip_identity::needs_lookup(&profile)),
            case["lookups"].as_u64().ok_or("Lookup count missing")?
        );
        let actual = ip_identity::resolve(&profile, case["country"].as_str())?;
        assert_eq!(
            serde_json::to_value(actual)?,
            case["expected"],
            "IP identity {}",
            case["name"]
        );
    }
    Ok(())
}

#[test]
fn timezone_grades_match_original_proxy_and_trust_boundaries()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::browser::{network_grades, trust_grades};
    use serde_json::json;
    let document: Value = serde_json::from_str(include_str!(
        "fixtures/network-characterization/timezone-grades.json"
    ))?;
    let rows = document["rows"].as_array().ok_or("Timezone rows missing")?;
    assert_eq!(rows.len(), 103, "Timezone observations missing");
    let leaks = network_grades::leaks(&document["ice"], Some("203.0.113.1"))?;
    let dns = numera_core::browser::probe_grades::dns(&document["dns"], None)?;
    let tls = numera_core::browser::probe_grades::tls(&document["tls"])?;
    for (index, row) in rows.iter().enumerate() {
        let mut profile = Profile::from_record(&document["profile"])?;
        profile.fingerprint.timezone = row["right"]
            .as_str()
            .ok_or("Profile timezone missing")?
            .into();
        let mut seen = document["seen"].clone();
        seen["timezone"] = row["left"].clone();
        let before_profile = serde_json::to_value(&profile)?;
        let before_seen = seen.clone();
        let info = json!({"ip":"203.0.113.1","country":null,"city":null,"timezone":row["left"]});
        assert_eq!(
            network_grades::proxy(&profile, &info, &leaks, &dns, &tls, 125)?,
            row["proxy"],
            "Timezone proxy grade differs: {index}"
        );
        assert_eq!(
            trust_grades::grade(&profile.fingerprint, &seen, "playwright")?,
            row["trust"],
            "Timezone trust grade differs: {index}"
        );
        assert_eq!(serde_json::to_value(&profile)?, before_profile);
        assert_eq!(seen, before_seen);
    }
    Ok(())
}
