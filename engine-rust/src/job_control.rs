use crate::{
    conversion::truth,
    jobs::JobStore,
    ownership::{claims, recorded_tree},
    storage::{atomic::write_json, locks::DataLock},
};
use serde_json::{Value, json};
use std::{fs, io, path::PathBuf, time::Duration};

impl JobStore {
    fn log_path(&self, record: &Value) -> io::Result<Option<PathBuf>> {
        let Some(log) = record["log"].as_str().filter(|s| !s.is_empty()) else {
            return Ok(None);
        };
        let id = record["id"]
            .as_str()
            .ok_or_else(|| io::Error::other("Stored job ID missing"))?;
        let expected = self.path(id)?.with_extension("log");
        let path = PathBuf::from(log);
        if path.file_name() != expected.file_name() {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Stored log does not belong to this job",
            ));
        }
        let resolved = match path.canonicalize() {
            Ok(path) => path,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let directory = self.root.join("jobs").canonicalize()?;
        if resolved
            != directory.join(
                expected
                    .file_name()
                    .ok_or_else(|| io::Error::other("Job log basename missing"))?,
            )
        {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Stored job log is outside its owning directory",
            ));
        }
        Ok(Some(resolved))
    }
    pub fn tail(&self, record: &Value, lines: i64) -> io::Result<Vec<String>> {
        match self.log_path(record)? {
            Some(path) => crate::job_output::tail(&path, lines),
            None => Ok(Vec::new()),
        }
    }
    pub fn stop(&self, id: &str, host: &str, now: f64) -> io::Result<Option<Value>> {
        if !now.is_finite() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Job stop time must be finite",
            ));
        }
        let path = self.path(id)?;
        let lock = DataLock::for_file(&path)?;
        let _guard = lock.acquire(Duration::from_secs(10))?;
        let Some(record) = self.get(id, now)? else {
            return Ok(None);
        };
        if truth(&record["running"]) && truth(&record["pid"]) {
            if !recorded_tree::verified(&record) {
                return Err(io::Error::other(format!(
                    "job {id} cannot be stopped safely: Numera never captured a verified identity for pid {}, so that number may belong to an unrelated process by now. End it by hand, then run `numera jobs prune`.",
                    record["pid"]
                )));
            }
            if !recorded_tree::settle(&record, Duration::from_secs(10))? {
                return Err(io::Error::other(format!(
                    "job {id} (pid {}) did not stop: something it started is still running and may still hold its profile's browser directory. Stop it by hand and re-run this once it is gone.",
                    record["pid"]
                )));
            }
            let mut outcome = self
                .record(id)?
                .ok_or_else(|| io::Error::other("Job record disappeared during stop"))?;
            // Write only the outcome keys: the spawning process owns identity.
            outcome.retain(|key, _| matches!(key.as_str(), "state" | "exit" | "finished"));
            outcome.extend(
                json!({"state":"stopped","finished":now})
                    .as_object()
                    .cloned()
                    .ok_or_else(|| io::Error::other("Job stop outcome missing"))?,
            );
            write_json(&path.with_extension("exit"), &Value::Object(outcome))?;
            if let Some(profile) = record["profile"].as_str().filter(|s| !s.is_empty()) {
                claims::clear(&self.root, profile, host)?;
            }
        }
        self.get(id, now)
    }
    pub fn stop_profile(&self, id: &str, host: &str, now: f64) -> io::Result<Vec<Value>> {
        let mut stopped = Vec::new();
        for job in self.listing(now)? {
            if job["profile"] == id && truth(&job["running"]) {
                let jid = job["id"]
                    .as_str()
                    .ok_or_else(|| io::Error::other("Stored job ID missing"))?;
                if let Some(record) = self.stop(jid, host, now)? {
                    stopped.push(record);
                }
            }
        }
        Ok(stopped)
    }
    pub fn prune(&self, now: f64) -> io::Result<usize> {
        let mut count = 0;
        for job in self.listing(now)? {
            let id = job["id"]
                .as_str()
                .ok_or_else(|| io::Error::other("Stored job ID missing"))?;
            let path = self.path(id)?;
            let lock = DataLock::for_file(&path)?;
            let _guard = lock.acquire(Duration::from_secs(10))?;
            let Some(current) = self.get(id, now)? else {
                continue;
            };
            if truth(&current["running"]) {
                continue;
            }
            // Validate every path before deleting any part of this job.
            let log = self.log_path(&current)?;
            remove(&path)?;
            remove(&path.with_extension("exit"))?;
            if let Some(log) = log {
                remove(&log)?;
            }
            count += 1;
        }
        Ok(count)
    }
}
fn remove(path: &std::path::Path) -> io::Result<()> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}
