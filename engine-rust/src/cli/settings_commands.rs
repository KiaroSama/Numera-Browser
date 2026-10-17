use super::profile_commands::Outcome;
use crate::{
    conversion::truth,
    http::tokens::TokenStore,
    storage::{
        config::ConfigStore,
        folders::Folders,
        repo::{ProfileRepo, RepoError},
        schedules::{ScheduleStore, next_run_at},
    },
};
use serde_json::{Map, Value, json};
use std::{collections::BTreeMap, path::PathBuf};

pub struct Settings<'a> {
    pub root: &'a std::path::Path,
    pub repo: &'a ProfileRepo,
    pub installed: &'a BTreeMap<String, bool>,
    pub environment_token: bool,
}
impl Settings<'_> {
    pub fn engines(&self) -> Outcome {
        let config = ConfigStore::new(self.root.into());
        Outcome::success(
            json!({"engines":self.installed,"default":config.default_engine(self.installed)}),
        )
    }
    pub fn default_engine(&self, name: Option<&str>) -> Result<Outcome, RepoError> {
        let config = ConfigStore::new(self.root.into());
        let Some(name) = name.filter(|v| !v.is_empty()) else {
            return Ok(Outcome::success(
                json!({"default_engine":config.default_engine(self.installed)}),
            ));
        };
        if !["playwright", "patchright", "camoufox", "cloak"].contains(&name) {
            return Ok(Outcome {
                exit: 1,
                payload: json!({"ok":false,"error":format!("unknown engine '{name}'"),"known":["camoufox","cloak","patchright","playwright"]}),
            });
        }
        config
            .set_many(Map::from_iter([("default_engine".into(), name.into())]))
            .map_err(|e| RepoError::Message(e.to_string()))?;
        Ok(Outcome::success(
            json!({"default_engine":name,"installed":self.installed.get(name)==Some(&true)}),
        ))
    }
    pub fn config(
        &self,
        args: &Map<String, Value>,
        home: &std::path::Path,
    ) -> Result<Outcome, RepoError> {
        let config = ConfigStore::new(self.root.into());
        let mut patch = Map::new();
        if let Some(engine) = args.get("default_engine").and_then(Value::as_str) {
            if !["playwright", "patchright", "camoufox", "cloak"].contains(&engine) {
                return Ok(Outcome {
                    exit: 1,
                    payload: json!({"ok":false,"error":format!("unknown engine '{engine}'"),"known":["camoufox","cloak","patchright","playwright"]}),
                });
            }
            patch.insert("default_engine".into(), engine.into());
        }
        if let Some(headless) = args.get("headless_launch").and_then(Value::as_str) {
            patch.insert("headless_launch".into(), (headless == "on").into());
        }
        if let Some(entries) = args.get("default_extension").and_then(Value::as_array) {
            let wanted = entries
                .iter()
                .filter_map(Value::as_str)
                .filter(|v| !v.is_empty())
                .map(|v| expand(v, home))
                .collect::<Result<Vec<_>, _>>()?;
            let current = config
                .default_extensions()
                .map_err(|e| RepoError::Message(e.to_string()))?;
            let fresh = wanted
                .iter()
                .filter(|v| !current.iter().any(|old| old == v.as_str()))
                .cloned()
                .collect::<Vec<_>>();
            let problems = crate::storage::extensions::unloadable(&fresh);
            if !problems.is_empty() {
                return Ok(Outcome::failed(problems.join("; ")));
            }
            patch.insert("default_extensions".into(), json!(wanted));
        }
        config
            .set_many(patch)
            .map_err(|e| RepoError::Message(e.to_string()))?;
        let values = config.read();
        Ok(Outcome::success(
            json!({"default_engine":config.default_engine(self.installed),"default_extensions":config.default_extensions().map_err(|e|RepoError::Message(e.to_string()))?,
            "headless_launch":truth(&values.get("headless_launch").cloned().unwrap_or(Value::Null)),"root":self.root,"version":env!("CARGO_PKG_VERSION")}),
        ))
    }
    pub fn folder(&self, verb: &str, args: &Map<String, Value>) -> Result<Outcome, RepoError> {
        let folders = Folders::new(self.root.into());
        let text = |key: &str| args.get(key).and_then(Value::as_str).unwrap_or("");
        let result = match verb {
            "list" => folders.listing().map(|names| json!({"folders":names})),
            "create" => folders
                .create(text("name"))
                .map(|names| json!({"folders":names,"created":text("name")})),
            "rename" => folders
                .rename(text("old"), text("new"))
                .map(|names| json!({"folders":names,"from":text("old"),"to":text("new")})),
            "delete" => {
                let target = text("reassign_to");
                let target = if target.is_empty() { "Unfiled" } else { target };
                folders.delete(text("name"), Some(target)).map(
                    |names| json!({"folders":names,"deleted":text("name"),"reassignedTo":target}),
                )
            }
            _ => return Err(RepoError::Message("Unknown folder command".into())),
        };
        match result {
            Ok(payload) => Ok(Outcome::success(payload)),
            Err(error) if error.kind() == std::io::ErrorKind::InvalidInput => Ok(Outcome {
                exit: 1,
                payload: json!({"error":error.to_string()}),
            }),
            Err(error) => Err(error.into()),
        }
    }
    pub fn tokens(
        &self,
        verb: &str,
        args: &Map<String, Value>,
        time: f64,
    ) -> Result<Outcome, RepoError> {
        let tokens = TokenStore::new(self.root.into());
        match verb {
            "list" => {
                let rows = tokens
                    .list()
                    .into_iter()
                    .map(|mut row| {
                        if let Some(row) = row.as_object_mut() {
                            row.remove("hash");
                        }
                        row
                    })
                    .collect::<Vec<_>>();
                Ok(Outcome::success(
                    json!({"tokens":rows,"envToken":self.environment_token}),
                ))
            }
            "create" => {
                match tokens.create(args.get("name").and_then(Value::as_str).unwrap_or(""), time) {
                    Ok(token) => Ok(Outcome::success(token)),
                    Err(error @ crate::http::token_errors::TokenError::DuplicateName(_)) => {
                        Ok(Outcome::failed(error.to_string()))
                    }
                    Err(error) => Err(RepoError::Message(error.to_string())),
                }
            }
            "revoke" => {
                let id = args.get("id").and_then(Value::as_str).unwrap_or("");
                if !tokens
                    .revoke(id, false)
                    .map_err(|e| RepoError::Message(e.to_string()))?
                {
                    return Ok(Outcome::failed("no token with that id"));
                }
                Ok(Outcome::success(
                    json!({"ok":true,"remaining":tokens.list().len()}),
                ))
            }
            _ => Err(RepoError::Message("Unknown token command".into())),
        }
    }
    pub fn schedule(
        &self,
        verb: &str,
        args: &Map<String, Value>,
        time: f64,
    ) -> Result<Outcome, RepoError> {
        let schedules = ScheduleStore::new(self.root.into());
        let text = |key: &str| args.get(key).and_then(Value::as_str).unwrap_or("");
        match verb {
            "list" => {
                let profiles = self.repo.list(time)?;
                let mut rows = schedules.list()?;
                for row in &mut rows {
                    let next = next_run_at(row)?;
                    let name = profiles
                        .iter()
                        .find(|p| row["profileId"] == p.id)
                        .map(|p| p.name.clone())
                        .unwrap_or_else(|| row["profileId"].clone());
                    row["profileName"] = name;
                    row["nextRun"] = next.into();
                    row["due"] = (next <= time).into();
                }
                Ok(Outcome::success(json!({"schedules":rows})))
            }
            "add" => {
                let reference = text("ref");
                let Some(profile) = self.repo.resolve(reference, time)? else {
                    return Ok(Outcome::failed(format!(
                        "no profile matching '{reference}'"
                    )));
                };
                let preset = args
                    .get("preset")
                    .filter(|v| truth(v))
                    .cloned()
                    .unwrap_or_else(|| "general".into());
                Ok(Outcome::success(schedules.add(&profile.id,json!({"everyHours":args.get("every"),"minutes":args.get("minutes"),"preset":preset}),Some(time))?))
            }
            "update" => {
                let mut patch = Map::new();
                for (src, dst) in [
                    ("every", "everyHours"),
                    ("minutes", "minutes"),
                    ("preset", "preset"),
                ] {
                    if let Some(value) = args.get(src).filter(|v| !v.is_null()) {
                        patch.insert(dst.into(), value.clone());
                    }
                }
                if let Some(value) = args.get("enabled").and_then(Value::as_str) {
                    patch.insert("enabled".into(), (value == "on").into());
                }
                if patch.is_empty() {
                    return Ok(Outcome::failed("no changes given"));
                }
                Ok(match schedules.update(text("id"), &Value::Object(patch))? {
                    Some(row) => Outcome::success(row),
                    None => Outcome::failed(format!("no schedule with id '{}'", text("id"))),
                })
            }
            "remove" => {
                let removed = schedules.delete(text("id"))?;
                Ok(Outcome {
                    exit: i32::from(!removed),
                    payload: json!({"ok":removed,"id":text("id")}),
                })
            }
            _ => Err(RepoError::Message(
                "Schedule execution requires the native warm-up service".into(),
            )),
        }
    }
}
pub(crate) fn expand(text: &str, home: &std::path::Path) -> Result<String, RepoError> {
    if text.starts_with('~') && text != "~" && !text.starts_with("~/") && !text.starts_with("~\\") {
        return Err(RepoError::Message(
            "Named-user home resolution is not configured".into(),
        ));
    }
    let path = crate::storage::data_root::expand_home(&PathBuf::from(text), home);
    Ok(path
        .to_str()
        .ok_or_else(|| RepoError::Message("Extension path is not Unicode".into()))?
        .into())
}
