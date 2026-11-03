//! Durable job identity/outcome reads. Spawning and cross-process stop are separate.
use crate::{
    conversion::truth,
    storage::{atomic::write_json, paths::safe_id},
};
use serde_json::{Map, Value, json};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub fn child_argv(argv: &[String], root: &Path) -> Vec<String> {
    let mut rest = Vec::new();
    let mut skip = false;
    for arg in argv {
        if skip {
            skip = false;
            continue;
        }
        if arg == "--detach" || arg.starts_with("--password=") {
            continue;
        }
        if arg == "--password" {
            skip = true;
            continue;
        }
        rest.push(arg.clone());
    }
    if !rest
        .iter()
        .any(|arg| arg == "--data-dir" || arg.starts_with("--data-dir="))
    {
        rest.splice(
            0..0,
            ["--data-dir".into(), root.to_string_lossy().into_owned()],
        );
    }
    rest
}
pub fn decorate(
    mut record: Map<String, Value>,
    leader: bool,
    survivors: bool,
    now: f64,
) -> io::Result<Value> {
    if !now.is_finite() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Job observation time must be finite",
        ));
    }
    if !record.get("state").is_some_and(truth) {
        let state = if leader || survivors {
            "running"
        } else if record.get("pid").is_none_or(Value::is_null)
            && now - record.get("started").and_then(Value::as_f64).unwrap_or(0.0) < 30.0
        {
            "starting"
        } else {
            "lost"
        };
        record.insert("state".into(), state.into());
    }
    if survivors {
        record.insert("orphaned".into(), true.into());
    }
    let running = matches!(
        record.get("state").and_then(Value::as_str),
        Some("running" | "starting")
    ) || survivors;
    record.insert("running".into(), running.into());
    Ok(Value::Object(record))
}
pub fn observed(
    record: Map<String, Value>,
    leader: io::Result<bool>,
    survivors: io::Result<bool>,
    now: f64,
) -> io::Result<Value> {
    let uncertain = leader.is_err();
    decorate(
        record,
        leader.unwrap_or(true),
        uncertain || survivors.unwrap_or(true),
        now,
    )
}
fn read(path: &Path) -> io::Result<Map<String, Value>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Map::new()),
        Err(error) => return Err(error),
    };
    let text = std::str::from_utf8(&bytes).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Stored job record is not UTF-8; nothing was written",
        )
    })?;
    Ok(serde_json::from_str::<Value>(text)
        .ok()
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default())
}
pub struct JobStore {
    pub(crate) root: PathBuf,
}
impl JobStore {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
    fn directory(&self) -> io::Result<PathBuf> {
        let directory = self.root.join("jobs");
        fs::create_dir_all(&directory)?;
        Ok(directory)
    }
    pub(crate) fn path(&self, id: &str) -> io::Result<PathBuf> {
        safe_id(id).map_err(io::Error::other)?;
        Ok(self.directory()?.join(format!("{id}.json")))
    }
    pub fn record(&self, id: &str) -> io::Result<Option<Map<String, Value>>> {
        let path = self.path(id)?;
        let mut record = read(&path)?;
        if record.is_empty() {
            return Ok(None);
        }
        record.extend(read(&path.with_extension("exit"))?);
        if record
            .get("id")
            .is_some_and(|value| value.as_str() != Some(id))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Stored job ID does not match its record path",
            ));
        }
        Ok(Some(record))
    }
    pub fn get(&self, id: &str, now: f64) -> io::Result<Option<Value>> {
        let Some(record) = self.record(id)? else {
            return Ok(None);
        };
        let pid = record
            .get("pid")
            .and_then(Value::as_u64)
            .and_then(|v| u32::try_from(v).ok())
            .unwrap_or(0);
        let token = record.get("pidToken").and_then(Value::as_u64);
        // Unknown observations retain ownership even with a stored done outcome.
        let observation = crate::ownership::identity::alive(pid, token);
        let leader = observation.as_ref().copied().unwrap_or(true);
        let value = Value::Object(record.clone());
        let report_survivors = match crate::ownership::unit_report::for_job(&value, &self.root, id)
        {
            Ok(Some(path)) => !crate::ownership::unit_report::empty(&path).unwrap_or(false),
            Ok(None) => false,
            Err(_) => true,
        };
        let supervisor_active = record.get("state").is_none()
            && record
                .get("supervisorPid")
                .and_then(Value::as_u64)
                .and_then(|pid| u32::try_from(pid).ok())
                .is_some_and(|pid| {
                    crate::ownership::identity::alive(
                        pid,
                        record.get("supervisorToken").and_then(Value::as_u64),
                    )
                    .unwrap_or(true)
                });
        let survivors = if report_survivors || supervisor_active || observation.is_err() {
            true
        } else if !leader && crate::ownership::recorded_tree::verified(&value) {
            crate::ownership::recorded_tree::owned(&value).unwrap_or(true)
                && !crate::ownership::recorded_tree::empty(&value).unwrap_or(false)
        } else {
            false
        };
        observed(record, observation, Ok(survivors), now).map(Some)
    }
    pub fn listing(&self, now: f64) -> io::Result<Vec<Value>> {
        let mut paths = fs::read_dir(self.directory()?)?
            .map(|e| e.map(|e| e.path()))
            .collect::<io::Result<Vec<_>>>()?;
        paths.sort();
        let mut rows = Vec::new();
        for path in paths {
            if path.extension().is_some_and(|v| v == "json")
                && let Some(id) = path.file_stem().and_then(|v| v.to_str())
                && let Some(record) = self.get(id, now)?
            {
                rows.push(record);
            }
        }
        rows.sort_by(|a, b| {
            a["started"]
                .as_f64()
                .unwrap_or(0.0)
                .total_cmp(&b["started"].as_f64().unwrap_or(0.0))
        });
        Ok(rows)
    }
    pub fn record_exit(&self, id: &str, code: i32, now: f64) -> io::Result<()> {
        if !now.is_finite() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Job completion time must be finite",
            ));
        }
        let path = self.path(id)?.with_extension("exit");
        let mut outcome = read(&path)?;
        outcome.extend(
            json!({"state":if code==0{"done"}else{"failed"},"exit":code,"finished":now})
                .as_object()
                .cloned()
                .ok_or_else(|| io::Error::other("Job outcome shape missing"))?,
        );
        write_json(&path, &Value::Object(outcome))
    }
}
