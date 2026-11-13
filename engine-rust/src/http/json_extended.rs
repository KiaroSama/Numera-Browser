//! Preserve CPython's three nonfinite JSON constants at the HTTP boundary.
//! All other syntax is validated by serde_json before values are restored.
use serde_json::Value;
use std::collections::{HashMap, HashSet};

fn strings(value: &Value, seen: &mut HashSet<String>) {
    match value {
        Value::String(value) => {
            seen.insert(value.clone());
        }
        Value::Array(values) => {
            for value in values {
                strings(value, seen);
            }
        }
        Value::Object(object) => {
            for (key, value) in object {
                seen.insert(key.clone());
                strings(value, seen);
            }
        }
        _ => {}
    }
}
fn restore(value: &mut Value, markers: &HashMap<String, &str>) {
    match value {
        Value::String(text) => {
            if let Some(number) = markers.get(text) {
                *value = Value::Number(serde_json::Number::from_string_unchecked(
                    (*number).to_owned(),
                ));
            }
        }
        Value::Array(values) => {
            for value in values {
                restore(value, markers);
            }
        }
        Value::Object(object) => {
            for value in object.values_mut() {
                restore(value, markers);
            }
        }
        _ => {}
    }
}
/// Accept exactly NaN, Infinity and -Infinity as the reference JSON decoder does.
/// Marker selection includes decoded keys and strings, including Unicode escapes.
pub fn parse(text: &str) -> Result<Value, serde_json::Error> {
    let bytes = text.as_bytes();
    let mut spans = Vec::new();
    let mut index = 0;
    let mut quoted = false;
    while index < bytes.len() {
        if quoted {
            match bytes[index] {
                b'\\' => {
                    index = (index + 2).min(bytes.len());
                    continue;
                }
                b'"' => quoted = false,
                _ => {}
            }
            index += 1;
            continue;
        }
        if bytes[index] == b'"' {
            quoted = true;
            index += 1;
            continue;
        }
        let tail = &bytes[index..];
        let constant = ["-Infinity", "Infinity", "NaN"]
            .into_iter()
            .find(|constant| tail.starts_with(constant.as_bytes()));
        if let Some(constant) = constant {
            spans.push((index, index + constant.len(), constant));
            index += constant.len();
        } else {
            index += 1;
        }
    }
    if spans.is_empty() {
        return serde_json::from_str(text);
    }
    let replace = |values: &[String]| {
        let mut rewritten = String::with_capacity(text.len());
        let mut previous = 0;
        for ((start, end, _), value) in spans.iter().zip(values) {
            rewritten.push_str(&text[previous..*start]);
            rewritten.push_str(value);
            previous = *end;
        }
        rewritten.push_str(&text[previous..]);
        rewritten
    };
    let masked = replace(&vec!["null".to_owned(); spans.len()]);
    let first = match serde_json::from_str::<Value>(&masked) {
        Ok(value) => value,
        Err(_) => return serde_json::from_str(text),
    };
    let mut seen = HashSet::new();
    strings(&first, &mut seen);
    let mut markers = HashMap::new();
    let mut replacements = Vec::new();
    let mut nonce = 0u64;
    for (_, _, constant) in &spans {
        let marker = loop {
            let marker = format!("__numera_nonfinite_{nonce}__");
            nonce += 1;
            if seen.insert(marker.clone()) {
                break marker;
            }
        };
        replacements.push(serde_json::to_string(&marker)?);
        markers.insert(marker, *constant);
    }
    let mut value = serde_json::from_str(&replace(&replacements))?;
    restore(&mut value, &markers);
    Ok(value)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn constants_retain_number_identity_without_interpreting_quoted_or_escaped_strings() {
        let value=parse(r#"{"a":[NaN,Infinity,-Infinity],"__numera_nonfinite_0__":"__numera_nonfinite_1__","quoted":"NaN","escaped":"\u005f_numera_nonfinite_2__","literal":"-Infinity"}"#).expect("source JSON constants");
        assert_eq!(value["a"][0].as_number().expect("NaN").as_str(), "NaN");
        assert_eq!(
            value["a"][1].as_number().expect("infinity").as_str(),
            "Infinity"
        );
        assert_eq!(
            value["a"][2]
                .as_number()
                .expect("negative infinity")
                .as_str(),
            "-Infinity"
        );
        assert_eq!(value["quoted"], "NaN");
        assert_eq!(value["literal"], "-Infinity");
        assert_eq!(value["escaped"], "__numera_nonfinite_2__");
        assert_eq!(value["__numera_nonfinite_0__"], "__numera_nonfinite_1__");
    }
    #[test]
    fn nonfinite_support_does_not_relax_json_delimiters_keys_comments_or_token_suffixes() {
        for text in [
            "[NaNx]",
            "[Infinity0]",
            "[-Infinity.0]",
            "[+Infinity]",
            "[nan]",
            "[NaN,]",
            "{NaN:1}",
            "{\"x\":NaN //comment\n}",
            "{\"x\":Infinity} trailing",
            "[NaN Infinity]",
        ] {
            assert!(parse(text).is_err(), "Accepted invalid JSON: {text}");
        }
        for text in ["null", "{\"x\":1.25}", "[true,false,\"NaN\"]"] {
            assert_eq!(
                parse(text).expect("ordinary JSON"),
                serde_json::from_str::<Value>(text).expect("ordinary JSON")
            );
        }
    }
}
