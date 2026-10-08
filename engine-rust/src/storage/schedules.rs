use super::{atomic::write_json, locks::DataLock, repo::RepoError};
use crate::conversion::truth;
use serde_json::{Map, Value};
use std::{fs, io, path::PathBuf, time::Duration};

pub const CLAIM_TTL_SECONDS: f64 = 12.0 * 60.0 * 60.0;
const OWNED: &[&str] = &[
    "lastRun",
    "lastOutcome",
    "lastError",
    "lastFailedAt",
    "claimedBy",
    "claimedAt",
];

pub struct ScheduleStore {
    path: PathBuf,
}
impl ScheduleStore {
    pub fn new(root: PathBuf) -> Self {
        Self {
            path: root.join("schedules.json"),
        }
    }
    pub fn list(&self) -> Result<Vec<Value>, RepoError> {
        let bytes = match fs::read(&self.path) {
            Ok(bytes) => bytes,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error.into()),
        };
        let value = match serde_json::from_slice::<Value>(&bytes) {
            Ok(value) => value,
            Err(_) => return Ok(Vec::new()),
        };
        value.as_array().cloned().ok_or_else(|| {
            RepoError::InvalidRecord(
                "Stored schedules are not an array; nothing was written".into(),
            )
        })
    }
    pub fn add(
        &self,
        profile_id: &str,
        request: Value,
        now: Option<f64>,
    ) -> Result<Value, RepoError> {
        let supplied = request
            .as_object()
            .ok_or_else(|| RepoError::InvalidRecord("Schedule request must be an object".into()))?;
        let mut complete = supplied.clone();
        complete.entry("preset").or_insert("general".into());
        complete.entry("enabled").or_insert(true.into());
        let clean = validate_schedule(&Value::Object(complete), false)?;
        let id = super::archive_paths::random_hex(6)?;
        let entry = serde_json::json!({"id":id,"profileId":profile_id,"everyHours":clean["everyHours"],"minutes":clean["minutes"],"preset":clean["preset"],"enabled":clean["enabled"],"lastRun":null,"createdAt":now});
        let lock = DataLock::for_file(&self.path)?;
        let _guard = lock.acquire(Duration::from_secs(30))?;
        let mut rows = self.list()?;
        if rows.iter().any(|row| row["id"] == id) {
            return Err(RepoError::Message(
                "Schedule id collision; nothing was written".into(),
            ));
        }
        rows.push(entry.clone());
        write_json(&self.path, &Value::Array(rows))?;
        Ok(entry)
    }
    pub fn update(&self, id: &str, patch: &Value) -> Result<Option<Value>, RepoError> {
        self.apply(id, validate_schedule(patch, true)?)
    }
    pub fn get(&self, id: &str) -> Result<Option<Value>, RepoError> {
        Ok(self.list()?.into_iter().find(|row| row["id"] == id))
    }
    pub fn claim_due(&self, id: &str, owner: &str, now: f64) -> Result<bool, RepoError> {
        if !now.is_finite() {
            return Err(RepoError::Message(
                "Schedule claim time must be finite".into(),
            ));
        }
        let lock = DataLock::for_file(&self.path)?;
        let _guard = lock.acquire(Duration::from_secs(30))?;
        let mut rows = self.list()?;
        for row in &mut rows {
            if row["id"] != id {
                continue;
            }
            if row.get("enabled").is_some_and(|value| !truth(value)) || next_run_at(row)? > now {
                return Ok(false);
            }
            if truth(&row["claimedAt"]) && now - number(&row["claimedAt"])? < CLAIM_TTL_SECONDS {
                return Ok(false);
            }
            row["claimedBy"] = owner.into();
            row["claimedAt"] = now.into();
            write_json(&self.path, &Value::Array(rows))?;
            return Ok(true);
        }
        Ok(false)
    }
    pub fn release(&self, id: &str) -> Result<(), RepoError> {
        self.apply(
            id,
            serde_json::json!({"claimedBy":null,"claimedAt":null})
                .as_object()
                .cloned()
                .ok_or_else(|| RepoError::Message("Claim release lost object shape".into()))?,
        )?;
        Ok(())
    }
    pub fn record(
        &self,
        id: &str,
        mut outcome: Map<String, Value>,
    ) -> Result<Option<Value>, RepoError> {
        let mut unknown = outcome
            .keys()
            .filter(|key| !OWNED.contains(&key.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        unknown.sort();
        if !unknown.is_empty() {
            return Err(RepoError::Message(format!(
                "not a scheduler-owned field: {}",
                unknown.join(", ")
            )));
        }
        if outcome.contains_key("lastRun") {
            outcome.insert("claimedBy".into(), Value::Null);
            outcome.insert("claimedAt".into(), Value::Null);
        }
        self.apply(id, outcome)
    }
    fn apply(&self, id: &str, patch: Map<String, Value>) -> Result<Option<Value>, RepoError> {
        let lock = DataLock::for_file(&self.path)?;
        let _guard = lock.acquire(Duration::from_secs(30))?;
        let mut rows = self.list()?;
        let mut result = None;
        for row in &mut rows {
            if row["id"] == id {
                row.as_object_mut()
                    .ok_or_else(|| {
                        RepoError::InvalidRecord(
                            "Stored schedule is not an object; nothing was written".into(),
                        )
                    })?
                    .extend(patch.clone());
                result = Some(row.clone());
            }
        }
        if result.is_some() {
            write_json(&self.path, &Value::Array(rows))?;
        }
        Ok(result)
    }
    pub fn delete(&self, id: &str) -> Result<bool, RepoError> {
        let lock = DataLock::for_file(&self.path)?;
        let _guard = lock.acquire(Duration::from_secs(30))?;
        let mut rows = self.list()?;
        let before = rows.len();
        rows.retain(|row| row["id"] != id);
        if rows.len() == before {
            return Ok(false);
        }
        write_json(&self.path, &Value::Array(rows))?;
        Ok(true)
    }
}
pub fn validate_schedule(patch: &Value, partial: bool) -> Result<Map<String, Value>, RepoError> {
    let object = patch
        .as_object()
        .ok_or_else(|| RepoError::InvalidRecord("Schedule patch must be an object".into()))?;
    let mut out = Map::new();
    if object.contains_key("everyHours") || !partial {
        let raw = object.get("everyHours").cloned().unwrap_or(0.25.into());
        let hours = number(&raw).map_err(|_| {
            RepoError::Message(format!(
                "'everyHours' must be a number, got {}",
                crate::validation::repr(&raw)
            ))
        })?;
        if !hours.is_finite() {
            return Err(RepoError::Message(
                "'everyHours' must be a finite number".into(),
            ));
        }
        if hours <= 0.0 {
            return Err(RepoError::Message(format!(
                "'everyHours' must be greater than 0, got {hours:?}"
            )));
        }
        if hours > 720.0 {
            return Err(RepoError::Message(
                "'everyHours' must be at most 720".into(),
            ));
        }
        out.insert("everyHours".into(), hours.max(0.25).into());
    }
    if object.contains_key("minutes") || !partial {
        let raw = object.get("minutes").cloned().unwrap_or(5.into());
        let minutes = number(&raw).map_err(|_| {
            RepoError::Message(format!(
                "'minutes': minutes must be a number, got {}",
                crate::validation::repr(&raw)
            ))
        })?;
        if !minutes.is_finite() {
            return Err(RepoError::Message(
                "'minutes': minutes must be a finite number".into(),
            ));
        }
        if minutes <= 0.0 {
            return Err(RepoError::Message(format!(
                "'minutes': minutes must be greater than zero, got {}",
                general_number(minutes)
            )));
        }
        if minutes > 480.0 {
            return Err(RepoError::Message(format!(
                "'minutes': minutes must be at most 480 (8h), got {}",
                general_number(minutes)
            )));
        }
        out.insert("minutes".into(), minutes.into());
    }
    if let Some(preset) = object.get("preset") {
        if !preset.is_null() && !preset.is_string() {
            return Err(RepoError::Message(format!(
                "'preset' must be a string, got {}",
                crate::validation::kind(preset)
            )));
        }
        if let Some(text) = preset.as_str()
            && !["ads", "crypto", "ecommerce", "general", "news", "social"]
                .contains(&text.to_lowercase().as_str())
        {
            return Err(RepoError::Message(format!(
                "'preset': unknown preset {}. Known: ads, crypto, ecommerce, general, news, social",
                crate::validation::repr(preset)
            )));
        }
        out.insert("preset".into(), preset.clone());
    }
    if let Some(enabled) = object.get("enabled") {
        if !enabled.is_boolean() {
            return Err(RepoError::Message("'enabled' must be true or false".into()));
        }
        out.insert("enabled".into(), enabled.clone());
    }
    if partial {
        for key in OWNED.iter().copied().chain(["id", "profileId"]) {
            if object.contains_key(key) {
                return Err(RepoError::Message(format!(
                    "'{key}' is set by the scheduler and cannot be patched"
                )));
            }
        }
    }
    Ok(out)
}
fn number(value: &Value) -> Result<f64, RepoError> {
    match value {
        Value::Bool(value) => Ok(if *value { 1.0 } else { 0.0 }),
        Value::Number(number) => number.as_f64().ok_or_else(|| {
            RepoError::InvalidRecord("Stored schedule number cannot be represented".into())
        }),
        Value::String(text) => {
            let mut normalized = String::new();
            for character in text.trim().chars() {
                normalized.push_str(
                    &crate::pool::decimal_ascii(&character.to_string())
                        .unwrap_or_else(|| character.to_string()),
                );
            }
            let bytes = normalized.as_bytes();
            for (index, byte) in bytes.iter().enumerate() {
                if *byte == b'_'
                    && (index == 0
                        || index + 1 == bytes.len()
                        || !bytes[index - 1].is_ascii_digit()
                        || !bytes[index + 1].is_ascii_digit())
                {
                    return Err(RepoError::InvalidRecord(
                        "Stored schedule time must be numeric".into(),
                    ));
                }
            }
            normalized.replace('_', "").parse::<f64>().map_err(|_| {
                RepoError::InvalidRecord("Stored schedule time must be numeric".into())
            })
        }
        _ => Err(RepoError::InvalidRecord(
            "Stored schedule time must be numeric".into(),
        )),
    }
}
fn general_number(value: f64) -> String {
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
pub fn next_run_at(schedule: &Value) -> Result<f64, RepoError> {
    let hours = match schedule.get("everyHours") {
        Some(value) => number(value)?,
        None => 1.0,
    };
    let interval = if hours > 0.25 { hours } else { 0.25 } * 3600.0;
    if schedule["lastRun"].is_null() {
        if truth(&schedule["createdAt"]) {
            number(&schedule["createdAt"])
        } else {
            Ok(0.0)
        }
    } else {
        Ok(number(&schedule["lastRun"])? + interval)
    }
}
pub fn due_schedules(schedules: &[Value], now: f64) -> Result<Vec<Value>, RepoError> {
    let mut due = Vec::new();
    for row in schedules {
        if row.get("enabled").is_none_or(truth) && next_run_at(row)? <= now {
            due.push(row.clone());
        }
    }
    Ok(due)
}
