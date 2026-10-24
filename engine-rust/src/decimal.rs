use crate::pool::decimal_ascii;

/// Python's base-ten text grammar shared by legacy revisions and dimensions.
/// Representation remains decimal text so large revisions are never truncated.
pub(crate) fn integer(text: &str) -> Option<String> {
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

pub(crate) fn general_number(value: f64) -> String {
    if value == 0.0 {
        return if value.is_sign_negative() { "-0" } else { "0" }.into();
    }
    let scientific = format!("{value:.5e}");
    let (mantissa, exponent) = scientific
        .split_once('e')
        .expect("Rust scientific format has an exponent");
    let exponent: i32 = exponent.parse().expect("Rust exponent is numeric");
    if !(-4..6).contains(&exponent) {
        format!(
            "{}e{:+03}",
            mantissa.trim_end_matches('0').trim_end_matches('.'),
            exponent
        )
    } else {
        let places = (5 - exponent).max(0) as usize;
        let fixed = format!("{value:.places$}");
        if fixed.contains('.') {
            fixed.trim_end_matches('0').trim_end_matches('.').into()
        } else {
            fixed
        }
    }
}
