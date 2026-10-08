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
