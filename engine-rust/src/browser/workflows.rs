//! Store repeatable QA steps locally and run them in an owned browser session.
use super::{
    automation::{Automation, PageAction, invalid},
    runtime::Runtime,
};
use crate::{
    conversion::value_text,
    ownership::registry::Registry,
    storage::{
        atomic,
        locks::DataLock,
        paths::safe_id,
        repo::{ProfileRepo, RepoError},
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::{
    collections::BTreeSet,
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

const MAX_PROFILES: usize = 4;
const MAX_STEPS: usize = 16;
const MAX_DOCUMENT: u64 = 128 * 1024;
const BUDGET: Duration = Duration::from_secs(50);

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LocalWorkflow {
    pub id: String,
    pub name: String,
    pub steps: Vec<WorkflowStep>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WorkflowStep {
    pub action: String,
    #[serde(default)]
    pub target: Option<String>,
    #[serde(default)]
    pub value: Option<Value>,
}

impl WorkflowStep {
    fn page_action(&self) -> io::Result<PageAction> {
        let mut fields = Map::new();
        let name = match self.action.as_str() {
            "navigate" => {
                fields.insert("url".into(), json!(self.target));
                "navigate"
            }
            "click" | "fill" => {
                fields.insert("selector".into(), json!(self.target));
                if self.action == "fill" {
                    fields.insert("value".into(), json!(self.value));
                }
                self.action.as_str()
            }
            "wait_ms" | "wait-ms" => {
                let ms = self
                    .value
                    .as_ref()
                    .and_then(|value| value.as_u64().or_else(|| value.as_str()?.parse().ok()));
                fields.insert("ms".into(), json!(ms));
                "wait"
            }
            "wait_selector" | "wait-selector" => {
                fields.insert("selector".into(), json!(self.target));
                "wait"
            }
            "snapshot" | "screenshot" => self.action.as_str(),
            _ => {
                return Err(invalid(
                    "Unsupported workflow action; run eval and cookie export as explicit profile commands",
                ));
            }
        };
        PageAction::parse(name, &fields)
    }
}

impl LocalWorkflow {
    fn actions(&self) -> io::Result<Vec<PageAction>> {
        workflow_id(&self.id)?;
        if self.name.trim().is_empty()
            || self.name.len() > 128
            || self.steps.is_empty()
            || self.steps.len() > MAX_STEPS
        {
            return Err(invalid(
                "Provide a workflow name (up to 128 bytes) and 1-16 steps",
            ));
        }
        if self
            .steps
            .first()
            .is_none_or(|step| step.action != "navigate")
        {
            return Err(invalid(
                "Start a workflow with a navigation to its loopback fixture",
            ));
        }
        self.steps.iter().map(WorkflowStep::page_action).collect()
    }
}

#[derive(Serialize)]
pub struct WorkflowRunReport {
    pub workflow_id: String,
    pub results: Vec<WorkflowRunResult>,
}

#[derive(Serialize)]
pub struct WorkflowRunResult {
    pub profile_id: String,
    pub ok: bool,
    pub steps_completed: usize,
    pub duration_ms: u64,
    pub error: Option<String>,
    pub artifacts: Vec<Value>,
}

pub(crate) struct Workflows<'a>(pub &'a Path);
impl Workflows<'_> {
    fn directory(&self) -> PathBuf {
        self.0.join("workflows")
    }
    fn path(&self, id: &str) -> io::Result<PathBuf> {
        Ok(self.directory().join(format!("{}.json", workflow_id(id)?)))
    }
    pub(crate) fn list(&self) -> io::Result<Vec<LocalWorkflow>> {
        let entries = match fs::read_dir(self.directory()) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => return Err(error),
        };
        let mut workflows = Vec::new();
        for entry in entries {
            let entry = entry?;
            if entry.file_type()?.is_file()
                && entry.path().extension().is_some_and(|ext| ext == "json")
            {
                let path = entry.path();
                let id = path
                    .file_stem()
                    .and_then(|id| id.to_str())
                    .ok_or_else(|| invalid("Invalid workflow filename"))?;
                workflows.push(self.load(id)?);
            }
        }
        workflows.sort_unstable_by(|a, b| a.id.cmp(&b.id));
        Ok(workflows)
    }
    pub(crate) fn save(&self, workflow: &LocalWorkflow) -> io::Result<()> {
        workflow.actions()?;
        let bytes = serde_json::to_vec_pretty(workflow).map_err(io::Error::other)?;
        if bytes.len() as u64 > MAX_DOCUMENT {
            return Err(invalid("Workflow document exceeds 128 KiB"));
        }
        let path = self.path(&workflow.id)?;
        let lock = DataLock::transaction(self.0)?;
        let _guard = lock.acquire_configured()?;
        atomic::write_bytes(&path, &bytes)
    }
    pub(crate) fn delete(&self, id: &str) -> io::Result<()> {
        let path = self.path(id)?;
        let lock = DataLock::transaction(self.0)?;
        let _guard = lock.acquire_configured()?;
        fs::remove_file(path)
    }
    pub(crate) fn load(&self, id: &str) -> io::Result<LocalWorkflow> {
        let path = self.path(id)?;
        if !path.symlink_metadata()?.file_type().is_file() {
            return Err(invalid("Workflow must be a regular file"));
        }
        let mut bytes = Vec::new();
        fs::File::open(path)?
            .take(MAX_DOCUMENT + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_DOCUMENT {
            return Err(invalid("Workflow document exceeds 128 KiB"));
        }
        let workflow: LocalWorkflow =
            serde_json::from_slice(&bytes).map_err(|_| invalid("Invalid stored workflow JSON"))?;
        if workflow.id != id {
            return Err(invalid("Stored workflow ID does not match its filename"));
        }
        Ok(workflow)
    }
}

fn workflow_id(id: &str) -> io::Result<&str> {
    safe_id(id)
        .map_err(|_| invalid("Workflow ID must be a portable slug"))
        .and_then(|id| {
            if id.bytes().any(|byte| byte.is_ascii_uppercase()) {
                Err(invalid("Workflow ID must be lowercase"))
            } else {
                Ok(id)
            }
        })
}

fn profile_ids(ids: &[String]) -> io::Result<&[String]> {
    let distinct = ids.iter().collect::<BTreeSet<_>>();
    if ids.is_empty()
        || ids.len() > MAX_PROFILES
        || distinct.len() != ids.len()
        || ids.iter().any(|id| safe_id(id).is_err())
    {
        return Err(invalid("Provide 1-4 distinct profile IDs"));
    }
    Ok(ids)
}

pub(crate) fn run(
    automation: &Automation<'_>,
    workflow: &LocalWorkflow,
    ids: &[String],
    now: f64,
) -> Result<WorkflowRunReport, RepoError> {
    let ids = profile_ids(ids)?;
    let actions = workflow.actions()?;
    let deadline = Instant::now() + BUDGET;
    let mut results = Vec::new();
    for id in ids {
        let started = Instant::now();
        let mut steps_completed = 0;
        let mut artifacts = Vec::new();
        let mut artifact_bytes = 0usize;
        let result = if deadline <= started {
            Err(io::Error::new(io::ErrorKind::TimedOut, "Workflow deadline expired").into())
        } else {
            automation.with_page(id, now, |adapter| {
                for action in &actions {
                    let result = action
                        .execute(adapter, deadline.saturating_duration_since(Instant::now()))?;
                    steps_completed += 1;
                    if result["snapshot"].is_string() || result["screenshot"].is_string() {
                        let size = serde_json::to_vec(&result).map_err(io::Error::other)?.len();
                        artifact_bytes += size;
                        if artifact_bytes > 64 * 1024 {
                            return Err(invalid(
                                "Workflow artifacts exceeded 64 KiB for this profile",
                            ));
                        }
                        artifacts.push(json!({"step":steps_completed,"result":result}));
                    }
                }
                Ok(())
            })
        };
        results.push(WorkflowRunResult {
            profile_id: id.clone(),
            ok: result.is_ok(),
            steps_completed,
            duration_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            error: result.err().map(|e| e.to_string()),
            artifacts,
        });
    }
    Ok(WorkflowRunReport {
        workflow_id: workflow.id.clone(),
        results,
    })
}

pub(crate) fn fleet(
    repo: &ProfileRepo,
    sessions: &Registry,
    runtime: Option<&Runtime>,
    ids: &[String],
    action: &str,
    url: Option<&str>,
    now: f64,
) -> Result<Value, RepoError> {
    let ids = profile_ids(ids)?;
    let navigation = match (action, url) {
        ("navigate", Some(url)) => Some(PageAction::parse(
            "navigate",
            &Map::from_iter([("url".into(), url.into())]),
        )?),
        ("launch" | "stop", None) => None,
        _ => {
            return Err(invalid(
                "Use launch or stop without a URL, or navigate with a loopback fixture URL",
            )
            .into());
        }
    };
    let mut results = Vec::new();
    let deadline = Instant::now() + BUDGET;
    for id in ids {
        let result = (|| -> Result<(), RepoError> {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(io::Error::new(
                    io::ErrorKind::TimedOut,
                    "Fleet action deadline expired",
                )
                .into());
            }
            let profile = repo
                .materialize(id, now)?
                .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Profile not found"))?;
            match action {
                "launch" => {
                    super::automation::require_qa_profile(&profile)?;
                    let runtime = runtime.ok_or_else(|| {
                        io::Error::other("Native browser runtime is not configured")
                    })?;
                    if !sessions
                        .open_browser(id, &value_text(&profile.name), |session, mut profile| {
                            super::automation::require_qa_profile(&profile).map_err(|error| {
                                crate::ownership::ready::StartupFailed {
                                    kind: "failed".into(),
                                    message: error.to_string(),
                                }
                            })?;
                            profile.startup_urls.clear();
                            runtime.open(session, profile, false, now)
                        })
                        .map_err(io::Error::other)?
                        && !sessions.owns(id)?
                    {
                        return Err(io::Error::new(
                            io::ErrorKind::WouldBlock,
                            "Profile browser belongs to another process",
                        )
                        .into());
                    }
                }
                "stop"
                    if !sessions.stop(id, remaining.min(Duration::from_secs(10)))?
                        && sessions.running(id)? =>
                {
                    return Err(io::Error::new(
                        io::ErrorKind::WouldBlock,
                        "Profile browser belongs to another process",
                    )
                    .into());
                }
                "navigate" => {
                    super::automation::require_qa_profile(&profile)?;
                    let action = navigation
                        .as_ref()
                        .ok_or_else(|| invalid("Missing navigation action"))?;
                    if sessions
                        .with_owned_page(id, |adapter| action.execute(adapter, remaining))?
                        .is_err()
                    {
                        return Err(io::Error::new(
                            io::ErrorKind::NotFound,
                            "Launch this profile in the running service first",
                        )
                        .into());
                    }
                }
                _ => {}
            }
            Ok(())
        })();
        results.push(json!({"profile_id":id,"ok":result.is_ok(),"error":result.err().map(|e| e.to_string())}));
    }
    Ok(json!({"action":action,"results":results}))
}

pub(crate) fn profile_ip_check(repo: &ProfileRepo, id: &str, now: f64) -> Result<Value, RepoError> {
    if repo.materialize(id, now)?.is_none() {
        return Err(io::Error::new(io::ErrorKind::NotFound, "Profile not found").into());
    }
    let profile = repo
        .get(id)?
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Profile not found"))?;
    let proxy = profile
        .proxy
        .as_ref()
        .map(crate::proxy_metadata::profile_entry)
        .transpose()?;
    let observed = match proxy {
        Some(proxy) => crate::network::proxy_check::check_sync(&proxy, Duration::from_secs(5))?,
        None => Value::Null,
    };
    Ok(
        json!({"profile_id":id,"ip":observed["ip"],"country_code":observed["country_code"],
        "country_name":observed["country_name"],"country_flag":observed["country_flag"].as_str().unwrap_or("--"),
        "timezone":profile.fingerprint.timezone,"webrtc_policy":profile.fingerprint.webrtc,
        "mac_address":null,"ready":observed["ok"]==true,
        "hint": if profile.proxy.is_none() { Some("No proxy is configured; no external IP probe was made") } else if observed["ok"] != true { Some("Check the configured proxy's address, credentials, and connectivity") } else { None }}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn workflow_storage_round_trips_and_rejects_nonportable_ids() -> io::Result<()> {
        let mut scratch = crate::storage::archive_paths::Staging::new(
            &std::env::temp_dir(),
            "numera-workflow-test-",
        )?;
        let store = Workflows(scratch.path()?);
        let mut workflow = LocalWorkflow {
            id: "check-form".into(),
            name: "Check formulaire".into(),
            steps: vec![
                WorkflowStep {
                    action: "navigate".into(),
                    target: Some("http://127.0.0.1:1234/form".into()),
                    value: None,
                },
                WorkflowStep {
                    action: "fill".into(),
                    target: Some("#message".into()),
                    value: Some(json!("فارسی")),
                },
            ],
        };
        store.save(&workflow)?;
        let list = store.list()?;
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].name, workflow.name);
        assert_eq!(list[0].steps[1].value, Some(json!("فارسی")));
        store.delete(&workflow.id)?;
        assert!(store.list()?.is_empty());
        workflow.id = "../escape".into();
        assert!(store.save(&workflow).is_err());
        assert!(profile_ids(&["one".into(), "one".into()]).is_err());
        scratch.cleanup()
    }
}
