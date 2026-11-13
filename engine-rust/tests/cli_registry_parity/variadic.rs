use numera_core::cli;
use serde_json::Value;

#[test]
fn variadic_positionals_preserve_original_option_separated_runs()
-> Result<(), Box<dyn std::error::Error>> {
    // Frozen execution of the immutable source; native assertions below remain live.
    let document: Value = serde_json::from_str(include_str!(
        "../fixtures/characterization/variadic_positionals_preserve_original_option_separated_runs.json"
    ))?;
    let occurrences = document["occurrences"]
        .as_u64()
        .ok_or("Occurrences missing")?;
    assert_eq!(occurrences, 8, "Original variadic inventory differs");
    let cases = document["rows"].as_array().ok_or("Variadic rows missing")?;
    assert_eq!(cases.len() as u64, occurrences * 21 + 8);
    let mut registry = cli::registry();
    registry.build();
    for (index, case) in cases.iter().enumerate() {
        let argv = case["argv"]
            .as_array()
            .ok_or("Variadic arguments missing")?
            .iter()
            .map(|v| v.as_str().ok_or("Argument not text"))
            .collect::<Result<Vec<_>, _>>()?;
        let actual = cli::parse(std::iter::once("numera").chain(argv));
        assert_eq!(
            i64::from(actual.as_ref().err().map_or(0, clap::Error::exit_code)),
            case["exit"].as_i64().ok_or("Original exit missing")?,
            "CLI variadic run admission differs: case={index}"
        );
        if let Ok(matches) = actual {
            assert_eq!(
                cli::reflection::values(&registry, &matches),
                case["values"],
                "CLI variadic value differs: case={index}"
            );
        }
    }
    Ok(())
}
