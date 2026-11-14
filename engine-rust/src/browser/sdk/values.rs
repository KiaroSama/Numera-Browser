// SPDX-License-Identifier: Apache-2.0
// Protocol semantics derived from Microsoft Playwright's Apache-2.0 serializers.
//! Preserve the SDK's JSON-visible evaluation values. The general binding
//! intentionally coerces BigInt to text and non-finite numbers to null; this
//! application must retain integers and refuse unrepresentable results.
use super::super::adapter::MAX_FRAME;
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Map, Value, json};
use std::{
    collections::{HashMap, HashSet},
    io::{self, Write},
    str::FromStr,
};

fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "Evaluation result cannot be represented by the SDK JSON protocol",
    )
}

struct Ascii;
impl serde_json::ser::Formatter for Ascii {
    fn write_number_str<W: Write + ?Sized>(
        &mut self,
        writer: &mut W,
        text: &str,
    ) -> io::Result<()> {
        let number = serde_json::Number::from_str(text).map_err(|_| invalid())?;
        writer.write_all(crate::validation::repr(&Value::Number(number)).as_bytes())
    }
    fn write_string_fragment<W: Write + ?Sized>(
        &mut self,
        writer: &mut W,
        text: &str,
    ) -> io::Result<()> {
        for character in text.chars() {
            if character < '\u{7f}' {
                writer.write_all(&[character as u8])?;
            } else {
                for unit in character.encode_utf16(&mut [0; 2]) {
                    write!(writer, "\\u{unit:04x}")?;
                }
            }
        }
        Ok(())
    }
    fn begin_array_value<W: Write + ?Sized>(
        &mut self,
        writer: &mut W,
        first: bool,
    ) -> io::Result<()> {
        if first {
            Ok(())
        } else {
            writer.write_all(b", ")
        }
    }
    fn begin_object_key<W: Write + ?Sized>(
        &mut self,
        writer: &mut W,
        first: bool,
    ) -> io::Result<()> {
        if first {
            Ok(())
        } else {
            writer.write_all(b", ")
        }
    }
    fn begin_object_value<W: Write + ?Sized>(&mut self, writer: &mut W) -> io::Result<()> {
        writer.write_all(b": ")
    }
}
struct Buffer(Vec<u8>);
impl Write for Buffer {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self
            .0
            .len()
            .checked_add(bytes.len())
            .is_none_or(|length| length >= MAX_FRAME)
        {
            return Err(invalid());
        }
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
pub fn encode<T: serde::Serialize + ?Sized>(value: &T) -> io::Result<Vec<u8>> {
    let mut output = Buffer(Vec::new());
    value
        .serialize(&mut serde_json::Serializer::with_formatter(
            &mut output,
            Ascii,
        ))
        .map_err(|_| invalid())?;
    Ok(output.0)
}

struct Item {
    value: Value,
    cost: usize,
}
struct Decoder {
    remaining: usize,
    cache_remaining: usize,
    wanted: HashSet<u64>,
    active: HashSet<u64>,
    references: HashMap<u64, Item>,
}
impl Decoder {
    fn spend(&mut self, count: usize) -> io::Result<()> {
        self.remaining = self.remaining.checked_sub(count).ok_or_else(invalid)?;
        Ok(())
    }
    fn literal(&mut self, value: Value) -> io::Result<Item> {
        let value = match value {
            Value::Number(number) => Value::Number(
                serde_json::Number::from_str(&crate::validation::repr(&Value::Number(number)))
                    .map_err(|_| invalid())?,
            ),
            other => other,
        };
        let cost = encode(&value)?.len();
        self.spend(cost)?;
        Ok(Item { value, cost })
    }
    fn decode(&mut self, raw: &Value, depth: usize) -> io::Result<Item> {
        if depth > 64 || !raw.is_object() {
            return Err(invalid());
        }
        if let Some(id) = raw.get("ref").and_then(Value::as_u64) {
            if self.active.contains(&id) {
                return Err(invalid());
            }
            if let Some(item) = self.references.get(&id) {
                let cost = item.cost;
                if cost > self.remaining {
                    return Err(invalid());
                }
                let value = item.value.clone();
                self.spend(cost)?;
                return Ok(Item { value, cost });
            }
            return self.literal(Value::Null);
        }
        if let Some(value) = raw.get("n").filter(|value| value.is_number()) {
            if !value.as_f64().is_some_and(f64::is_finite) {
                return Err(invalid());
            }
            return self.literal(value.clone());
        }
        if let Some(value) = raw.get("s").and_then(Value::as_str) {
            if value.len() > self.remaining {
                return Err(invalid());
            }
            return self.literal(value.into());
        }
        if let Some(value) = raw.get("b").and_then(Value::as_bool) {
            return self.literal(value.into());
        }
        if let Some(value) = raw.get("v").and_then(Value::as_str) {
            return match value {
                "undefined" | "null" => self.literal(Value::Null),
                "-0" => self.literal(json!(-0.0)),
                _ => Err(invalid()),
            };
        }
        if let Some(value) = raw.get("bi").and_then(Value::as_str) {
            let digits = value.strip_prefix('-').unwrap_or(value);
            if digits.is_empty()
                || digits.len() > 4300
                || !digits.bytes().all(|byte| byte.is_ascii_digit())
            {
                return Err(invalid());
            }
            return self.literal(Value::Number(
                serde_json::Number::from_str(value).map_err(|_| invalid())?,
            ));
        }
        if let Some(value) = raw.get("u").and_then(Value::as_str) {
            return self.literal(url_tuple(value)?);
        }
        if raw.get("d").is_some()
            || raw.get("e").is_some()
            || raw.get("fn").is_some()
            || raw.get("h").is_some()
        {
            return Err(invalid());
        }
        if let Some(regex) = raw.get("r") {
            // The Python SDK retains a serialized JS-only regexp when its
            // pattern cannot be compiled by Python; ordinary patterns become
            // non-JSON re.Pattern objects and fail at the protocol boundary.
            let pattern = regex["p"].as_str().ok_or_else(invalid)?;
            let named = pattern
                .match_indices("(?<")
                .any(|(index, _)| !matches!(pattern.as_bytes().get(index + 3), Some(b'=' | b'!')));
            if named
                || pattern.contains("\\p{")
                || pattern.contains("\\P{")
                || pattern.contains("\\k<")
            {
                return self.literal(raw.clone());
            }
            return Err(invalid());
        }
        if let Some(typed) = raw.get("ta") {
            return self.typed_array(typed);
        }
        let start = self.remaining;
        let id = raw.get("id").and_then(Value::as_u64);
        if let Some(id) = id {
            self.active.insert(id);
        }
        let value = if let Some(array) = raw.get("a").and_then(Value::as_array) {
            self.spend(2)?;
            let mut result = Vec::new();
            for (index, value) in array.iter().enumerate() {
                if index > 0 {
                    self.spend(2)?;
                }
                result.push(self.decode(value, depth + 1)?.value);
            }
            Value::Array(result)
        } else if let Some(object) = raw.get("o").and_then(Value::as_array) {
            self.spend(2)?;
            let mut result = Map::new();
            for (index, row) in object.iter().enumerate() {
                let key = row["k"].as_str().ok_or_else(invalid)?;
                if key.len() > self.remaining {
                    return Err(invalid());
                }
                self.spend(encode(key)?.len() + 2 + 2 * usize::from(index > 0))?;
                result.insert(key.into(), self.decode(&row["v"], depth + 1)?.value);
            }
            Value::Object(result)
        } else {
            return Err(invalid());
        };
        let cost = start - self.remaining;
        if let Some(id) = id {
            self.active.remove(&id);
            if self.wanted.contains(&id) {
                self.cache_remaining =
                    self.cache_remaining.checked_sub(cost).ok_or_else(invalid)?;
                self.references.insert(
                    id,
                    Item {
                        value: value.clone(),
                        cost,
                    },
                );
            }
        }
        Ok(Item { value, cost })
    }
    fn typed_array(&mut self, typed: &Value) -> io::Result<Item> {
        let encoded = typed["b"].as_str().ok_or_else(invalid)?;
        if encoded.len() > MAX_FRAME * 2 {
            return Err(invalid());
        }
        let kind = typed["k"].as_str().ok_or_else(invalid)?;
        let bytes = STANDARD.decode(encoded).map_err(|_| invalid())?;
        let size = match kind {
            "i8" | "ui8" | "ui8c" => 1,
            "i16" | "ui16" => 2,
            "i32" | "ui32" | "f32" => 4,
            "bi64" | "bui64" | "f64" => 8,
            _ => return Err(invalid()),
        };
        if !bytes.len().is_multiple_of(size) {
            return Err(invalid());
        }
        let start = self.remaining;
        self.spend(2)?;
        let mut result = Vec::new();
        for (index, bytes) in bytes.chunks_exact(size).enumerate() {
            if index > 0 {
                self.spend(2)?;
            }
            let value = match kind {
                "i8" => Value::from(bytes[0] as i8),
                "ui8" | "ui8c" => Value::from(bytes[0]),
                "i16" => Value::from(i16::from_le_bytes(bytes.try_into().map_err(|_| invalid())?)),
                "ui16" => Value::from(u16::from_le_bytes(bytes.try_into().map_err(|_| invalid())?)),
                "i32" => Value::from(i32::from_le_bytes(bytes.try_into().map_err(|_| invalid())?)),
                "ui32" => Value::from(u32::from_le_bytes(bytes.try_into().map_err(|_| invalid())?)),
                "bi64" => Value::from(i64::from_le_bytes(bytes.try_into().map_err(|_| invalid())?)),
                "bui64" => {
                    Value::from(u64::from_le_bytes(bytes.try_into().map_err(|_| invalid())?))
                }
                "f32" => {
                    finite(f32::from_le_bytes(bytes.try_into().map_err(|_| invalid())?) as f64)?
                }
                "f64" => finite(f64::from_le_bytes(bytes.try_into().map_err(|_| invalid())?))?,
                _ => return Err(invalid()),
            };
            result.push(self.literal(value)?.value);
        }
        Ok(Item {
            value: Value::Array(result),
            cost: start - self.remaining,
        })
    }
}
fn finite(value: f64) -> io::Result<Value> {
    serde_json::Number::from_f64(value)
        .map(Value::Number)
        .ok_or_else(invalid)
}
fn url_tuple(value: &str) -> io::Result<Value> {
    let url = reqwest::Url::parse(value).map_err(|_| invalid())?;
    let tail = value.split_once(':').ok_or_else(invalid)?.1;
    let netloc = tail
        .strip_prefix("//")
        .map(|tail| tail.split(['/', '?', '#']).next().unwrap_or(""))
        .unwrap_or("");
    let mut path = url.path();
    let mut parameters = "";
    if matches!(
        url.scheme(),
        "ftp"
            | "http"
            | "https"
            | "hdl"
            | "prospero"
            | "rtsp"
            | "rtsps"
            | "rtspu"
            | "sip"
            | "sips"
            | "sftp"
            | "tel"
    ) {
        let segment = path.rfind('/').map_or(0, |index| index + 1);
        if let Some(index) = path[segment..].find(';').map(|index| index + segment) {
            parameters = &path[index + 1..];
            path = &path[..index];
        }
    }
    Ok(json!([
        url.scheme(),
        netloc,
        path,
        parameters,
        url.query().unwrap_or(""),
        url.fragment().unwrap_or("")
    ]))
}

pub fn decode(raw: &Value) -> io::Result<Value> {
    let mut wanted = HashSet::new();
    let mut pending = vec![raw];
    while let Some(raw) = pending.pop() {
        if let Some(id) = raw.get("ref").and_then(Value::as_u64) {
            wanted.insert(id);
        }
        if let Some(array) = raw.get("a").and_then(Value::as_array) {
            pending.extend(array);
        }
        if let Some(object) = raw.get("o").and_then(Value::as_array) {
            pending.extend(object.iter().map(|row| &row["v"]));
        }
    }
    Decoder {
        remaining: MAX_FRAME,
        cache_remaining: MAX_FRAME * 2,
        wanted,
        active: HashSet::new(),
        references: HashMap::new(),
    }
    .decode(raw, 0)
    .map(|item| item.value)
}
