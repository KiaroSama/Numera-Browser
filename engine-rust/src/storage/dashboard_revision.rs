use super::repo::RepoError;
use crate::{conversion::truth, pool::decimal_ascii, validation};
use serde_json::Value;

// Revisions are legacy persisted values, not newly validated API input. Python
// int(value or 0) accepts booleans/text/floats and arbitrary-size integers.
pub(super) fn next(value: &Value) -> Result<Value, RepoError> {
    let integer = if !truth(value) {
        "0".into()
    } else {
        match value {
            Value::Bool(value) => if *value { "1" } else { "0" }.into(),
            Value::Number(value) => {
                let text = value.to_string();
                if text.contains(['.', 'e', 'E']) {
                    let number = value
                        .as_f64()
                        .filter(|number| number.is_finite())
                        .ok_or_else(|| {
                            RepoError::Message("cannot convert float infinity to integer".into())
                        })?;
                    format!("{:.0}", number.trunc())
                } else {
                    text
                }
            }
            Value::String(text) => parse(text).ok_or_else(|| {
                RepoError::Message(format!(
                    "invalid literal for int() with base 10: {}",
                    validation::repr(value)
                ))
            })?,
            other => {
                let kind = match other {
                    Value::Array(_) => "list",
                    Value::Object(_) => "dict",
                    _ => "NoneType",
                };
                return Err(RepoError::Message(format!(
                    "int() argument must be a string, a bytes-like object or a real number, not '{kind}'"
                )));
            }
        }
    };
    let negative = integer.starts_with('-');
    let magnitude = integer
        .trim_start_matches(['-', '+'])
        .trim_start_matches('0');
    let mut digits = if magnitude.is_empty() {
        vec![b'0']
    } else {
        magnitude.as_bytes().to_vec()
    };
    if negative && magnitude != "1" && !magnitude.is_empty() {
        for digit in digits.iter_mut().rev() {
            if *digit > b'0' {
                *digit -= 1;
                break;
            }
            *digit = b'9';
        }
    } else if negative && magnitude == "1" {
        digits = vec![b'0'];
    } else {
        let mut carry = true;
        for digit in digits.iter_mut().rev() {
            if *digit < b'9' {
                *digit += 1;
                carry = false;
                break;
            }
            *digit = b'0';
        }
        if carry {
            digits.insert(0, b'1');
        }
    }
    let number = String::from_utf8(digits)
        .map_err(|_| RepoError::Message("Revision increment produced invalid digits".into()))?;
    let number = number.trim_start_matches('0');
    let number = if number.is_empty() {
        "0".into()
    } else if negative && magnitude != "1" && !magnitude.is_empty() {
        format!("-{number}")
    } else {
        number.to_owned()
    };
    serde_json::from_str(&number)
        .map_err(|_| RepoError::Message("Revision cannot be serialized".into()))
}

fn parse(text: &str) -> Option<String> {
    let text = text.trim();
    let (negative, body) = if let Some(body) = text.strip_prefix('-') {
        (true, body)
    } else {
        (false, text.strip_prefix('+').unwrap_or(text))
    };
    let mut digits = String::new();
    let mut previous_digit = false;
    for character in body.chars() {
        if character == '_' {
            if !previous_digit {
                return None;
            }
            previous_digit = false;
        } else {
            digits.push_str(&decimal_ascii(&character.to_string())?);
            previous_digit = true;
        }
    }
    if !previous_digit {
        return None;
    }
    if negative {
        digits.insert(0, '-');
    }
    Some(digits)
}
