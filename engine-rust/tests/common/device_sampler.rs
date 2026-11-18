//! Independent pinned-source vectors for the public sampler and its production
//! fingerprint consumer. These assertions extend existing source IDs only.
use numera_core::devices::{self, SamplingError, SeededSampler};
use serde_json::{Value, json};
use sha2::Digest;
fn vectors() -> Value {
    serde_json::from_str(include_str!("../fixtures/device-sampler.json"))
        .expect("pinned device sampler vectors")
}
fn weights(value: &Value) -> Option<Vec<f64>> {
    value.as_array().map(|values| {
        values
            .iter()
            .map(|value| match value.as_str() {
                Some("nan") => f64::NAN,
                Some("+inf") => f64::INFINITY,
                Some("-inf") => f64::NEG_INFINITY,
                _ => value.as_f64().expect("numeric weight"),
            })
            .collect()
    })
}
pub fn weighted_vectors() {
    let document = vectors();
    for row in document["weighted"].as_array().expect("weighted vectors") {
        let seed = row["seed"].as_i64().expect("seed");
        let mut sampler = SeededSampler::new(seed);
        let pool = row["pool"].as_array().expect("generic pool");
        let weights = weights(&row["weights"]);
        let actual = match devices::weighted(&mut sampler, pool, weights.as_deref()) {
            Ok(value) => json!({"ok":value}),
            Err(error) => {
                let category = match error {
                    SamplingError::EmptyPool => "IndexError",
                    SamplingError::NonPositiveTotal | SamplingError::NonFiniteTotal => "ValueError",
                    _ => panic!("unexpected sampler error: {error}"),
                };
                json!({"error":category,"message":error.to_string()})
            }
        };
        assert_eq!(
            actual, row["expected"],
            "source weighted {} seed={seed}",
            row["name"]
        );
        let next = (0..17).collect::<Vec<_>>();
        let actual = devices::weighted(&mut sampler, &next, None).expect("subsequent uniform draw");
        assert_eq!(
            json!(actual),
            row["next_uniform"],
            "weighted changed RNG consumption: {} seed={seed}",
            row["name"]
        );
    }
    #[cfg(target_pointer_width = "64")]
    for row in document["large_uniform"]
        .as_array()
        .expect("large uniform vectors")
    {
        let mut sampler = SeededSampler::new(row["seed"].as_i64().expect("seed"));
        let values = vec![(); row["length"].as_u64().expect("length") as usize];
        devices::weighted(&mut sampler, &values, None).expect("large zero-sized uniform pool");
        let next = (0..17).collect::<Vec<_>>();
        assert_eq!(
            json!(devices::weighted(&mut sampler, &next, None).expect("next uniform")),
            row["next_uniform"]
        );
    }
    let pool = [("first", 1u8), ("second", 2)];
    assert_eq!(
        *devices::weighted(&mut SeededSampler::new(0), &pool, None).expect("typed tuple pool"),
        ("second", 2)
    );
    let fractional = serde_json::Number::from_f64(1.5).expect("finite number");
    assert!(matches!(
        SeededSampler::from_number(&fractional),
        Err(SamplingError::InvalidSeed)
    ));
}
pub fn font_vectors() {
    let document = vectors();
    for row in document["fonts"].as_array().expect("font vectors") {
        let seed = row["seed"].as_i64().expect("seed");
        let os = row["os"].as_str().expect("OS");
        let mut sampler = SeededSampler::new(seed);
        assert_eq!(
            json!(devices::sample_fonts(&mut sampler, os).expect("sample fonts")),
            row["expected"],
            "font OS={os:?} seed={seed}"
        );
        let next = (0..17).collect::<Vec<_>>();
        assert_eq!(
            json!(devices::weighted(&mut sampler, &next, None).expect("subsequent draw")),
            row["next_uniform"],
            "font selection changed RNG consumption OS={os:?} seed={seed}"
        );
    }
}
fn canonical(value: Value) -> Value {
    match value {
        Value::Object(fields) => {
            let sorted = fields
                .into_iter()
                .collect::<std::collections::BTreeMap<_, _>>();
            let mut result = serde_json::Map::new();
            for (key, value) in sorted {
                result.insert(key, canonical(value));
            }
            Value::Object(result)
        }
        Value::Array(values) => Value::Array(values.into_iter().map(canonical).collect()),
        other => other,
    }
}
pub fn generated_identities() {
    let document = vectors();
    for row in document["identities"]
        .as_array()
        .expect("seeded identity vectors")
    {
        let seed = row["seed"].as_i64().expect("seed");
        let os = row["os"].as_str();
        let actual =
            numera_core::fingerprint::generate(numera_core::fingerprint::GenerateOptions {
                seed,
                os,
                ..Default::default()
            })
            .expect("production identity generation");
        let value = canonical(serde_json::to_value(actual).expect("identity JSON"));
        let bytes = serde_json::to_vec(&value).expect("canonical identity JSON");
        let digest = sha2::Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(
            digest, row["sha256"],
            "entire seeded identity changed seed={seed} os={os:?}"
        );
    }
}
