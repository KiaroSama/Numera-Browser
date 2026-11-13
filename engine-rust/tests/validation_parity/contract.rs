use numera_core::{
    models::Profile,
    write_contract::{self, Input},
};
use serde_json::{Value, json};

#[test]
fn public_write_contract_shapes_contexts_and_refusals_match_original()
-> Result<(), Box<dyn std::error::Error>> {
    // Frozen execution of the immutable source; native assertions below remain live.
    let cases: Vec<Value> = serde_json::from_str(include_str!(
        "../fixtures/characterization/public_write_contract_shapes_contexts_and_refusals_match_original.json"
    ))?;
    assert_eq!(cases.len(), 270, "Write-contract observations missing");
    fn outcome<T: serde::Serialize>(result: Result<T, write_contract::Error>) -> Value {
        match result {
            Ok(value) => json!({"ok":value}),
            Err(error) => json!({"error":match &error {
                write_contract::Error::Refused(_) => "WriteRefused",
                write_contract::Error::UnknownShape(_) => "ValueError",
                write_contract::Error::Policy(_) => "PolicyError",
            },"message":error.to_string()}),
        }
    }
    for (index, case) in cases.iter().enumerate() {
        let raw = case["input"].clone();
        let model = if case["model"] == true {
            Some(Profile::from_record(&raw)?)
        } else {
            None
        };
        let before = model.as_ref().map(serde_json::to_value).transpose()?;
        let intent = model
            .as_ref()
            .and_then(|profile| profile.proxy.as_ref())
            .map(|proxy| proxy.password_intent());
        let input = model.as_ref().map_or(Input::Json(&raw), Input::Profile);
        let kind = case["kind"].as_str();
        let context = case["where"].as_str().ok_or("Context missing")?;
        assert_eq!(
            outcome(write_contract::shape(input, kind)),
            case["shape"],
            "Write shape differs: {index}"
        );
        assert_eq!(
            outcome(write_contract::profile_problems(input, context, kind)),
            case["problems"],
            "Contextual validation differs: {index}"
        );
        assert_eq!(
            outcome(write_contract::require(input, context, kind)),
            case["require"],
            "Typed write refusal differs: {index}"
        );
        assert_eq!(
            outcome(write_contract::require_flat_numbers(input, context)),
            case["flat"],
            "Flat write refusal differs: {index}"
        );
        assert_eq!(raw, case["input"], "Raw validation input changed");
        assert_eq!(
            model.as_ref().map(serde_json::to_value).transpose()?,
            before,
            "Model validation input changed"
        );
        assert_eq!(
            model
                .as_ref()
                .and_then(|profile| profile.proxy.as_ref())
                .map(|proxy| proxy.password_intent()),
            intent,
            "Validation changed credential intent"
        );
    }
    Ok(())
}
