//! Browser QA operations on an owned profile and an explicitly local fixture.
use crate::{
    browser::{adapter::Adapter, runtime::Runtime},
    conversion::value_text,
    ownership::registry::Registry,
    storage::repo::{ProfileRepo, RepoError},
};
use serde_json::{Map, Value, json};
use std::{io, time::Duration};

pub(crate) const ACTIONS: [&str; 9] = [
    "navigate",
    "click",
    "fill",
    "snapshot",
    "console",
    "network",
    "eval",
    "screenshot",
    "wait",
];
const MAX_TIMEOUT: u64 = 10_000;

#[derive(Clone)]
pub(crate) struct PageAction {
    name: String,
    parameters: Map<String, Value>,
    timeout: u64,
}

impl PageAction {
    pub(crate) fn parse(name: &str, fields: &Map<String, Value>) -> io::Result<Self> {
        if !ACTIONS.contains(&name) {
            return Err(invalid("Unknown page action"));
        }
        let timeout = match fields.get("timeout_ms").filter(|value| !value.is_null()) {
            Some(value) => value
                .as_u64()
                .filter(|ms| (1..=MAX_TIMEOUT).contains(ms))
                .ok_or_else(|| invalid("timeout_ms must be between 1 and 10000"))?,
            None => 3_000,
        };
        let mut parameters = Map::new();
        if let Some(url) = fields.get("url").filter(|value| !value.is_null()) {
            let url = url.as_str().ok_or_else(|| invalid("url must be text"))?;
            parameters.insert(
                "url".into(),
                crate::network::devtools::qa_url(url)?.as_str().into(),
            );
        } else if name == "navigate" {
            return Err(invalid("Provide a loopback fixture URL"));
        }
        let required = |key: &str, allow_empty: bool| {
            fields
                .get(key)
                .and_then(Value::as_str)
                .filter(|value| value.len() <= 16_384 && (allow_empty || !value.trim().is_empty()))
                .ok_or_else(|| invalid(&format!("Provide a valid {key}")))
        };
        match name {
            "click" | "fill" => {
                parameters.insert("selector".into(), required("selector", false)?.into());
                if name == "fill" {
                    parameters.insert("value".into(), required("value", true)?.into());
                }
            }
            "eval" => {
                parameters.insert("expression".into(), required("expression", false)?.into());
            }
            "wait" => {
                if fields.get("selector").is_some_and(|value| !value.is_null()) {
                    if fields.get("ms").is_some_and(|value| !value.is_null()) {
                        return Err(invalid("Use selector or ms, not both"));
                    }
                    parameters.insert("selector".into(), required("selector", false)?.into());
                } else {
                    let ms = fields
                        .get("ms")
                        .and_then(Value::as_u64)
                        .filter(|ms| (1..=timeout).contains(ms))
                        .ok_or_else(|| {
                            invalid("Provide a selector or a wait no longer than timeout_ms")
                        })?;
                    parameters.insert("ms".into(), ms.into());
                }
            }
            _ => {}
        }
        Ok(Self {
            name: name.into(),
            parameters,
            timeout,
        })
    }

    pub(crate) fn execute(&self, adapter: &mut Adapter, budget: Duration) -> io::Result<Value> {
        let mut fields = self.parameters.clone();
        fields.insert("action".into(), self.name.clone().into());
        let timeout = self
            .timeout
            .min(u64::try_from(budget.as_millis()).unwrap_or(u64::MAX));
        if timeout == 0 {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "Page action deadline expired",
            ));
        }
        fields.insert("timeout_ms".into(), timeout.into());
        let result = adapter.call(
            "page-action",
            fields,
            Duration::from_millis(timeout + 1_000),
        )?;
        if let Some(error) = result["error"].as_str() {
            let kind = if result["timed_out"] == true {
                io::ErrorKind::TimedOut
            } else {
                io::ErrorKind::InvalidInput
            };
            return Err(io::Error::new(kind, error.to_owned()));
        }
        Ok(result)
    }
}

pub(crate) struct Automation<'a> {
    pub repo: &'a ProfileRepo,
    pub sessions: &'a Registry,
    pub runtime: Option<&'a Runtime>,
}
impl Automation<'_> {
    pub(crate) fn run(&self, id: &str, action: &PageAction, now: f64) -> Result<Value, RepoError> {
        self.with_page(id, now, |adapter| {
            action.execute(adapter, Duration::from_millis(MAX_TIMEOUT))
        })
        .map(|result| json!({"profile_id":id,"result":result}))
    }

    pub(crate) fn with_page<T>(
        &self,
        id: &str,
        now: f64,
        operation: impl FnOnce(&mut Adapter) -> io::Result<T>,
    ) -> Result<T, RepoError> {
        let profile = self
            .repo
            .materialize(id, now)?
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Profile not found"))?;
        require_qa_profile(&profile)?;
        match self.sessions.with_owned_page(id, operation)? {
            Ok(value) => Ok(value),
            Err(operation) => {
                let runtime = self
                    .runtime
                    .ok_or_else(|| io::Error::other("Native browser runtime is not configured"))?;
                self.sessions
                    .with_finite_browser(
                        id,
                        &value_text(&profile.name),
                        |session, mut profile| {
                            require_qa_profile(&profile).map_err(|error| {
                                crate::ownership::ready::StartupFailed {
                                    kind: "failed".into(),
                                    message: error.to_string(),
                                }
                            })?;
                            profile.startup_urls.clear();
                            runtime.open(session, profile, true, now)
                        },
                        operation,
                    )
                    .map_err(RepoError::from)
            }
        }
    }
}

pub(crate) fn require_qa_profile(profile: &crate::models::Profile) -> io::Result<()> {
    if !matches!(profile.engine.as_str(), "" | "playwright")
        || profile.proxy.is_some()
        || !profile.extensions.is_empty()
        || !profile.launch_args.is_empty()
    {
        return Err(invalid(
            "Use a Playwright QA profile without a proxy, extensions, or custom browser flags",
        ));
    }
    Ok(())
}

pub(crate) fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn actions_reject_nonlocal_urls_and_require_correct_fields() {
        let fields = |value: Value| value.as_object().cloned().unwrap_or_default();
        assert!(
            PageAction::parse("navigate", &fields(json!({"url":"https://example.test"}))).is_err()
        );
        assert!(
            PageAction::parse(
                "navigate",
                &fields(json!({"url":"http://127.0.0.1:1234/qa"}))
            )
            .is_ok()
        );
        assert!(PageAction::parse("fill", &fields(json!({"selector":"#qa"}))).is_err());
        assert!(PageAction::parse("fill", &fields(json!({"selector":"#qa","value":""}))).is_ok());
        assert!(PageAction::parse("wait", &fields(json!({"selector":"#qa","ms":9}))).is_err());
        assert!(PageAction::parse("wait", &fields(json!({"timeout_ms":8,"ms":9}))).is_err());
    }
}
