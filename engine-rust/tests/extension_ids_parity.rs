use numera_core::storage::extensions;
use serde_json::Value;

#[test]
fn extension_ids_keep_source_refusals_and_url_recognition() -> Result<(), Box<dyn std::error::Error>>
{
    let fixture: Value = serde_json::from_str(include_str!("fixtures/extension-ids.json"))?;
    let cases = fixture["cases"].as_array().ok_or("ID cases missing")?;
    assert!(!cases.is_empty());
    for case in cases {
        let text = case["input"].as_str().ok_or("Input missing")?;
        match extensions::extension_id(text) {
            Ok(id) => {
                assert!(case["error"].is_null(), "{text}");
                assert_eq!(id, case["result"], "{text}");
            }
            Err(error) => assert_eq!(
                Some(error.to_string().as_str()),
                case["error"].as_str(),
                "{text}"
            ),
        }
        assert_eq!(
            Value::Bool(extensions::is_store_url(text)),
            case["is_store_url"]
        );
    }
    Ok(())
}
