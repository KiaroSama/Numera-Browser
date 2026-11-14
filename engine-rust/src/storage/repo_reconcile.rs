//! Source-compatible read repair; explicit launch coherence remains a separate gate.
use super::repo::{ProfileRepo, RepoError};
use crate::conversion::{to_engine, truth};
use serde_json::Value;
use std::collections::BTreeSet;

fn numeric(value: &Value) -> Option<(String, bool)> {
    match value {
        Value::Bool(value) => Some((u8::from(*value).to_string(), true)),
        Value::Number(value) => {
            let text = value.to_string();
            let integer = !text.contains(['.', 'e', 'E'])
                && !matches!(text.as_str(), "NaN" | "Infinity" | "-Infinity");
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

pub(super) fn equal(left: &Value, right: &Value) -> bool {
    // The usual stored numeric values already have identical representations.
    // Keep nonfinite and mixed representations on the compatibility path.
    if let (Value::Number(left), Value::Number(right)) = (left, right)
        && left == right
        && left.as_f64().is_some_and(f64::is_finite)
    {
        return true;
    }
    if let (Some(left), Some(right)) = (numeric(left), numeric(right)) {
        return number_equal(left, right);
    }
    match (left, right) {
        (Value::Array(left), Value::Array(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(left, right)| container_equal(left, right))
        }
        (Value::Object(left), Value::Object(right)) => {
            left.len() == right.len()
                && left.iter().all(|(key, value)| {
                    right
                        .get(key)
                        .is_some_and(|other| container_equal(value, other))
                })
        }
        _ => left == right,
    }
}

fn container_equal(left: &Value, right: &Value) -> bool {
    // CPython's ordinary JSON decoder shares its NaN constant across loads.
    // Container equality checks identity before equality for each member, so
    // that shared NaN compares equal inside containers while scalar NaN does not.
    matches!((left, right), (Value::Number(left), Value::Number(right))
        if left.as_str() == "NaN" && right.as_str() == "NaN")
        || equal(left, right)
}

fn equal_without(left: &Value, right: &Value, fields: &[&str]) -> bool {
    let members = |value: &Value| {
        value.as_object().map_or(0, |object| {
            object
                .keys()
                .filter(|key| !fields.contains(&key.as_str()))
                .count()
        })
    };
    members(left) == members(right)
        && left
            .as_object()
            .into_iter()
            .flatten()
            .filter(|(key, _)| !fields.contains(&key.as_str()))
            .all(|(key, value)| {
                right
                    .get(key)
                    .is_some_and(|other| container_equal(value, other))
            })
}

impl ProfileRepo {
    fn secret_disagrees(
        &self,
        stored: Option<&str>,
        fresh: Option<&str>,
    ) -> Result<bool, RepoError> {
        if self.same_secret(stored, fresh)? {
            return Ok(false);
        }
        if stored.is_none_or(str::is_empty) {
            return Ok(fresh.is_some_and(|value| !value.is_empty()));
        }
        if fresh.is_none_or(str::is_empty) {
            return Ok(false);
        }
        Ok(self.readable_secret(stored)?.is_some() && self.readable_secret(fresh)?.is_some())
    }

    pub(crate) fn read_disagrees(
        &self,
        engine: &Value,
        dashboard: &Value,
        now: f64,
    ) -> Result<bool, RepoError> {
        Ok(!self.disagreements(engine, dashboard, now, 1)?.is_empty())
    }

    pub(crate) fn disagreements(
        &self,
        engine: &Value,
        dashboard: &Value,
        now: f64,
        limit: usize,
    ) -> Result<Vec<String>, RepoError> {
        let mut out = Vec::new();
        if !truth(engine) || !truth(dashboard) {
            return Ok(out);
        }
        let Some(engine) = engine.as_object() else {
            return Ok(out);
        };
        let Ok(profile) = to_engine(dashboard, now) else {
            return Ok(out);
        };
        let fresh = serde_json::to_value(&profile).or_else(|_| {
            // Value's serializer rejects nonfinite numbers even though the
            // source accepts them in stored JSON. Preserve those rare records
            // through the same bounded-syntax decoder as repository reads.
            serde_json::to_string(&profile)
                .and_then(|text| crate::http::json_extended::parse(&text))
        });
        let Ok(Value::Object(fresh)) = fresh else {
            return Ok(out);
        };
        let keys: BTreeSet<_> = engine.keys().chain(fresh.keys()).collect();
        for key in keys {
            if ["id", "created_at", "updated_at"].contains(&key.as_str()) {
                continue;
            }
            let left = engine.get(key).unwrap_or(&Value::Null);
            let right = fresh.get(key).unwrap_or(&Value::Null);
            let changed = match key.as_str() {
                "fingerprint" => {
                    !equal_without(left, right, &["viewport_width", "viewport_height"])
                }
                "proxy" => {
                    if self
                        .secret_disagrees(left["password"].as_str(), right["password"].as_str())?
                    {
                        out.push("proxy.password".into());
                        if out.len() == limit {
                            return Ok(out);
                        }
                    }
                    // Both missing/non-object and password-only proxies were
                    // normalized to null after filtering by the old comparison.
                    !equal_without(left, right, &["password"])
                }
                _ => !equal(left, right),
            };
            if changed {
                out.push(key.clone());
                if out.len() == limit {
                    return Ok(out);
                }
            }
        }
        Ok(out)
    }
}
