use numera_core::http::profile_fields::check;
use serde_json::Value;

#[test]
fn http_write_fields_preserve_original_order_and_refusal_vocabulary()
-> Result<(), Box<dyn std::error::Error>> {
    let fixture: Value = serde_json::from_str(include_str!("fixtures/http-fields.json"))?;
    let cases = fixture["cases"]
        .as_array()
        .ok_or("HTTP field cases missing")?;
    assert!(!cases.is_empty());
    for case in cases {
        assert_eq!(
            serde_json::to_value(check(&case["input"], "profile", &[])?)?,
            case["expected"],
            "{}",
            case["name"]
        );
    }
    Ok(())
}
