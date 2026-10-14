use chrono::{Datelike, Local, LocalResult, NaiveDate, NaiveDateTime, TimeZone, Weekday};
use serde_json::Value;
use std::io;

fn digits(text: &str) -> Option<u32> {
    if text.is_empty() || !text.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    text.parse().ok()
}

fn date(text: &str) -> Option<(NaiveDate, usize)> {
    let year = digits(text.get(..4)?)? as i32;
    if !(1..=9999).contains(&year) {
        return None;
    }
    let dashed = text.as_bytes().get(4) == Some(&b'-');
    let start = if dashed { 5 } else { 4 };
    if text.as_bytes().get(start) == Some(&b'W') {
        let week = digits(text.get(start + 1..start + 3)?)?;
        let mut end = start + 3;
        if dashed && text.as_bytes().get(end) == Some(&b'-') {
            end += 1;
        }
        let day = if text.as_bytes().get(end).is_some_and(u8::is_ascii_digit) {
            let value = digits(text.get(end..end + 1)?)?;
            end += 1;
            value
        } else {
            1
        };
        let weekday = match day {
            1 => Weekday::Mon,
            2 => Weekday::Tue,
            3 => Weekday::Wed,
            4 => Weekday::Thu,
            5 => Weekday::Fri,
            6 => Weekday::Sat,
            7 => Weekday::Sun,
            _ => return None,
        };
        let value = NaiveDate::from_isoywd_opt(year, week, weekday)?;
        return (1..=9999).contains(&value.year()).then_some((value, end));
    }
    let month = digits(text.get(start..start + 2)?)?;
    let day_start = if dashed {
        if text.as_bytes().get(start + 2) != Some(&b'-') {
            return None;
        }
        start + 3
    } else {
        start + 2
    };
    Some((
        NaiveDate::from_ymd_opt(year, month, digits(text.get(day_start..day_start + 2)?)?)?,
        day_start + 2,
    ))
}

fn clock(text: &str, offset: bool) -> Option<(u32, u32, u32, u32)> {
    if !text.is_ascii() {
        return None;
    }
    let (base, fractional) = text.split_once(['.', ',']).unwrap_or((text, ""));
    let separated = base.contains(':');
    let components: Vec<_> = if separated {
        base.split(':').collect()
    } else {
        if !matches!(base.len(), 2 | 4 | 6) {
            return None;
        }
        (0..base.len())
            .step_by(2)
            .map(|i| &base[i..i + 2])
            .collect()
    };
    if components.is_empty() || components.len() > 3 || components.iter().any(|v| v.len() != 2) {
        return None;
    }
    let hour = digits(components[0])?;
    let minute = components.get(1).map_or(Some(0), |v| digits(v))?;
    let second = components.get(2).map_or(Some(0), |v| digits(v))?;
    let micro = if fractional.is_empty() {
        if base.len() != text.len() {
            return None;
        }
        0
    } else {
        if !fractional.bytes().all(|c| c.is_ascii_digit()) {
            return None;
        }
        let first = &fractional[..fractional.len().min(6)];
        digits(first)? * 10u32.pow(6 - first.len() as u32)
    };
    if !offset && (hour >= 24 || minute >= 60 || second >= 60) {
        return None;
    }
    Some((hour, minute, second, micro))
}

fn iso(text: &str) -> Option<f64> {
    let text = text.replace('Z', "+00:00");
    let (day, consumed) = date(&text)?;
    let remaining = &text[consumed..];
    let (hour, minute, second, micro, offset) = if remaining.is_empty() {
        (0, 0, 0, 0, None)
    } else {
        let separator = remaining.chars().next()?;
        let time = &remaining[separator.len_utf8()..];
        let split = time.find(['+', '-']);
        let (time, offset) = split.map_or((time, None), |i| (&time[..i], Some(&time[i..])));
        let (hour, minute, second, micro) = clock(time, false)?;
        let offset = if let Some(raw) = offset {
            let (h, m, s, fraction) = clock(&raw[1..], true)?;
            // Python normalizes offset fields before checking the total range;
            // +01:60 is valid even though 60 is not a time-of-day minute.
            let seconds = f64::from(h * 3600 + m * 60 + s) + f64::from(fraction) / 1_000_000.0;
            if seconds >= 86_400.0 {
                return None;
            }
            Some(if raw.starts_with('-') {
                -seconds
            } else {
                seconds
            })
        } else {
            None
        };
        (hour, minute, second, micro, offset)
    };
    let value = day.and_hms_micro_opt(hour, minute, second, micro)?;
    match offset {
        Some(offset) => Some(epoch(value) - offset),
        None => match Local.from_local_datetime(&value) {
            LocalResult::Single(value) => {
                Some(value.timestamp() as f64 + f64::from(micro) / 1_000_000.0)
            }
            LocalResult::Ambiguous(first, _) => {
                Some(first.timestamp() as f64 + f64::from(micro) / 1_000_000.0)
            }
            LocalResult::None => None,
        },
    }
}
fn epoch(value: NaiveDateTime) -> f64 {
    value.and_utc().timestamp() as f64
        + f64::from(value.and_utc().timestamp_subsec_micros()) / 1_000_000.0
}

pub(crate) fn numeric_text(text: &str) -> Option<f64> {
    let mut ascii = String::new();
    let chars: Vec<_> = text.trim().chars().collect();
    for (index, &c) in chars.iter().enumerate() {
        if c == '_' {
            let decimal = |c: char| crate::pool::decimal_ascii(&c.to_string()).is_some();
            if index == 0
                || index + 1 == chars.len()
                || !decimal(chars[index - 1])
                || !decimal(chars[index + 1])
            {
                return None;
            }
        } else if c.is_ascii() {
            ascii.push(c);
        } else {
            ascii.push_str(&crate::pool::decimal_ascii(&c.to_string())?);
        }
    }
    ascii.parse::<f64>().ok()
}

/// Preserve tolerant expiry fallback, but refuse non-finite values at the native boundary.
pub(super) fn as_epoch(value: &Value) -> io::Result<f64> {
    let number = match value {
        Value::Bool(value) => Some(f64::from(u8::from(*value))),
        Value::Number(value) => value.as_f64(),
        Value::String(value) => numeric_text(value),
        _ => None,
    }
    .or_else(|| value.as_str().and_then(iso))
    .unwrap_or(-1.0);
    if number.is_finite() {
        Ok(number)
    } else {
        Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Cookie expiry is not finite",
        ))
    }
}
