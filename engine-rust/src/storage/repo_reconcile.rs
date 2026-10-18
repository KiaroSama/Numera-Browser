//! Source-compatible read repair; explicit launch coherence remains a separate gate.
use super::repo::ProfileRepo;
use crate::conversion::{to_engine, truth};
use serde_json::{Map, Value};

fn numeric(value: &Value) -> Option<(String, bool)> {
    match value {
        Value::Bool(value) => Some((u8::from(*value).to_string(), true)),
        Value::Number(value) => {
            let text = value.to_string();
            let integer = !text.contains(['.', 'e', 'E']);
            Some((text, integer))
        }
        _ => None,
    }
}

fn number_equal(left: (String, bool), right: (String, bool)) -> bool {
    match (left.1, right.1) {
        (true, true) => {
            left.0 == right.0
                || matches!(
                    (left.0.as_str(), right.0.as_str()),
                    ("-0", "0") | ("0", "-0")
                )
        }
        (false, false) => match (left.0.parse::<f64>(), right.0.parse::<f64>()) {
            (Ok(left), Ok(right)) => left == right,
            _ => false,
        },
        _ => {
            let (integer, float) = if left.1 {
                (left.0, right.0)
            } else {
                (right.0, left.0)
            };
            float.parse::<f64>().ok().is_some_and(|value| {
                value.is_finite()
                    && value.fract() == 0.0
                    && (integer == format!("{value:.0}")
                        || (value == 0.0 && matches!(integer.as_str(), "0" | "-0")))
            })
        }
    }
}

fn equal(left: &Value, right: &Value) -> bool {
    if let (Some(left), Some(right)) = (numeric(left), numeric(right)) {
        return number_equal(left, right);
    }
    match (left, right) {
        (Value::Array(left), Value::Array(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| equal(left, right))
        }
        (Value::Object(left), Value::Object(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .all(|(key, value)| right.get(key).is_some_and(|other| equal(value, other)))
        }
        _ => left == right,
    }
}

fn without(value: &Value, fields: &[&str]) -> Value {
    Value::Object(
        value
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(key, _)| !fields.contains(&key.as_str()))
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect::<Map<_, _>>(),
    )
}

impl ProfileRepo {
    fn secret_disagrees(&self, stored: Option<&str>, fresh: Option<&str>) -> bool {
        if self.same_secret(stored, fresh) {
            return false;
        }
        if stored.is_none_or(str::is_empty) {
            return fresh.is_some_and(|value| !value.is_empty());
        }
        if fresh.is_none_or(str::is_empty) {
            return false;
        }
        self.readable_secret(stored).is_some() && self.readable_secret(fresh).is_some()
    }

    pub(crate) fn read_disagrees(&self, engine: &Value, dashboard: &Value, now: f64) -> bool {
        if !truth(engine) || !truth(dashboard) {
            return false;
        }
        let Some(engine) = engine.as_object() else {
            return false;
        };
        let Ok(profile) = to_engine(dashboard, now) else {
            return false;
        };
        let Ok(Value::Object(fresh)) = serde_json::to_value(profile) else {
            return false;
        };
        engine.keys().chain(fresh.keys()).any(|key| {
            if ["id", "created_at", "updated_at"].contains(&key.as_str()) {
                return false;
            }
            let left = engine.get(key).unwrap_or(&Value::Null);
            let right = fresh.get(key).unwrap_or(&Value::Null);
            match key.as_str() {
                "fingerprint" => !equal(
                    &without(left, &["viewport_width", "viewport_height"]),
                    &without(right, &["viewport_width", "viewport_height"]),
                ),
                "proxy" => {
                    if self.secret_disagrees(left["password"].as_str(), right["password"].as_str())
                    {
                        return true;
                    }
                    let left = without(left, &["password"]);
                    let right = without(right, &["password"]);
                    !equal(
                        if truth(&left) { &left } else { &Value::Null },
                        if truth(&right) { &right } else { &Value::Null },
                    )
                }
                _ => !equal(left, right),
            }
        })
    }
}
