use crate::{
    collision,
    conversion::value_text,
    fingerprint::{self, GenerateOptions},
    identity,
    models::Profile,
    proxy::Proxy,
    storage::{
        config::ConfigStore,
        repo::{ProfileRepo, RepoError},
    },
};
use serde_json::{Map, Value, json};
use std::{collections::BTreeMap, path::Path};

pub struct Outcome {
    pub payload: Value,
    pub exit: i32,
}
impl Outcome {
    pub fn success(payload: Value) -> Self {
        Self { payload, exit: 0 }
    }
    pub fn failed(message: impl Into<String>) -> Self {
        Self {
            payload: json!({"ok":false,"error":message.into()}),
            exit: 1,
        }
    }
}
fn missing(reference: &str) -> Outcome {
    Outcome::failed(format!("no profile matching '{reference}'"))
}

pub struct Profiles<'a> {
    pub repo: &'a ProfileRepo,
    pub config: &'a ConfigStore,
    pub installed: &'a BTreeMap<String, bool>,
    pub host: &'a str,
}
impl Profiles<'_> {
    pub fn list(&self, tag: Option<&str>, time: f64) -> Result<Outcome, RepoError> {
        let rows = self
            .repo
            .list(time)?
            .into_iter()
            .filter(|p| tag.is_none_or(|tag| tag.is_empty() || p.tags.iter().any(|t| t == tag)))
            .map(|p| summary(&p))
            .collect::<Vec<_>>();
        Ok(Outcome::success(
            json!({"total":rows.len(),"profiles":rows}),
        ))
    }
    pub fn show(&self, reference: &str, time: f64) -> Result<Outcome, RepoError> {
        Ok(match self.repo.resolve(reference, time)? {
            Some(profile) => {
                Outcome::success(serde_json::to_value(profile).map_err(std::io::Error::other)?)
            }
            None => missing(reference),
        })
    }
    pub fn check(&self, reference: &str, time: f64) -> Result<Outcome, RepoError> {
        let Some(profile) = self.repo.resolve(reference, time)? else {
            return Ok(missing(reference));
        };
        let issues = identity::validate(&profile.fingerprint).map_err(std::io::Error::other)?;
        Ok(Outcome {
            exit: i32::from(!issues.is_empty()),
            payload: json!({"ok":issues.is_empty(),"id":profile.id,"name":profile.name,"issues":issues}),
        })
    }
    pub fn collisions(&self, time: f64) -> Result<Outcome, RepoError> {
        let profiles = self.repo.list(time)?;
        let groups = collision::find(&profiles);
        Ok(Outcome {
            exit: i32::from(!groups.is_empty()),
            payload: json!({"total":profiles.len(),"groups":groups}),
        })
    }
    pub fn create(&self, args: &Map<String, Value>, time: f64) -> Result<Outcome, RepoError> {
        self.create_started(args, time, |_| Ok(()))
    }
    pub fn create_with_output(
        &self,
        args: &Map<String, Value>,
        time: f64,
        output: &mut super::output::Output<impl std::io::Write, impl std::io::Write>,
    ) -> Result<Outcome, RepoError> {
        let outcome = self.create_started(args, time, |engine| {
            super::profile_output::engine_notice(output, engine).map_err(RepoError::from)
        })?;
        if outcome.payload["ok"] == false {
            output.say(
                &format!(
                    "A profile named '{}' already exists.",
                    value_text(&args["name"])
                ),
                Some(super::output::Tone::Red),
            )?;
        } else {
            super::profile_output::created(output, &outcome.payload)?;
        }
        Ok(outcome)
    }
    fn create_started(
        &self,
        args: &Map<String, Value>,
        time: f64,
        notice: impl FnOnce(&str) -> Result<(), RepoError>,
    ) -> Result<Outcome, RepoError> {
        let text = |key: &str| args.get(key).and_then(Value::as_str);
        let proxy = parse_proxy(text("proxy"), text("country"))?;
        let seed = match args.get("seed").filter(|v| !v.is_null()) {
            Some(value) => value
                .as_number()
                .cloned()
                .ok_or_else(|| RepoError::Message("Fingerprint seed must be an integer".into()))?,
            None => loop {
                let mut bytes = [0u8; 4];
                getrandom::fill(&mut bytes).map_err(std::io::Error::other)?;
                let seed = u32::from_ne_bytes(bytes) & 0x7fff_ffff;
                if seed != 0 {
                    break serde_json::Number::from(seed);
                }
            },
        };
        let fingerprint = fingerprint::generate(GenerateOptions {
            seed,
            os: text("os"),
            locale: text("locale"),
            country: proxy.as_ref().and_then(|p| p.country.as_deref()),
            mobile: args.get("mobile") == Some(&Value::Bool(true)),
        })
        .map_err(|e| RepoError::Message(e.to_string()))?;
        let explicit = text("engine").filter(|v| !v.is_empty());
        let engine = explicit
            .map(str::to_owned)
            .unwrap_or_else(|| value_text(&self.config.default_engine(self.installed)));
        let installed = !["playwright", "patchright", "camoufox", "cloak"]
            .contains(&engine.as_str())
            || self.installed.get(&engine) == Some(&true);
        if !installed {
            notice(&engine)?;
        }
        let id = crate::storage::archive_paths::random_hex(6)?;
        let extensions = self
            .config
            .default_extensions()
            .map_err(|e| RepoError::Message(e.to_string()))?;
        let mut profile = Profile::from_record(&json!({"id":id,"name":text("name").unwrap_or(""),"fingerprint":fingerprint,"proxy":null,"engine":engine,
            "notes":text("notes").unwrap_or(""),"tags":args.get("tag").filter(|v|v.is_array()).cloned().unwrap_or_else(||json!([])),
            "extensions":extensions,"created_at":time,"updated_at":time})).map_err(std::io::Error::other)?;
        // This proxy is a fresh user choice, not a decoded storage view.
        profile.proxy = proxy;
        let changes = explicit
            .map(|engine| Map::from_iter([("default_engine".into(), engine.into())]))
            .unwrap_or_default();
        let written = self.repo.save_many(vec![profile], changes, "reject", time);
        match written {
            Err(RepoError::NameTaken(message)) => Ok(Outcome::failed(message)),
            Err(error) => Err(error),
            Ok(mut profiles) => {
                let profile = profiles.pop().ok_or_else(|| {
                    RepoError::Message("Profile create returned no record".into())
                })?;
                Ok(Outcome::success(
                    json!({"ok":true,"profile":profile,"engineInstalled":installed}),
                ))
            }
        }
    }
    pub fn edit(
        &self,
        reference: &str,
        args: &Map<String, Value>,
        time: f64,
    ) -> Result<Outcome, RepoError> {
        let mut profiles = self.repo.patch_many(
            &[reference.into()],
            |profile| {
                if let Some(notes) = args.get("notes").and_then(Value::as_str) {
                    profile.notes = notes.into();
                }
                if let Some(tags) = args.get("add_tag").and_then(Value::as_array) {
                    for tag in tags.iter().filter_map(Value::as_str) {
                        if !profile.tags.iter().any(|v| v == tag) {
                            profile.tags.push(tag.into());
                        }
                    }
                }
                if let Some(proxy) = args.get("proxy").and_then(Value::as_str) {
                    profile.proxy =
                        parse_proxy(Some(proxy), args.get("country").and_then(Value::as_str))?;
                }
                Ok(true)
            },
            Map::new(),
            "reject",
            time,
        )?;
        Ok(match profiles.pop() {
            Some(profile) => Outcome::success(json!({"ok":true,"profile":profile})),
            None => missing(reference),
        })
    }
    pub fn export(
        &self,
        reference: &str,
        file: &Path,
        backup: bool,
        time: f64,
    ) -> Result<Outcome, RepoError> {
        let Some(profile) = self.repo.resolve(reference, time)? else {
            return Ok(missing(reference));
        };
        let destination = if backup {
            self.repo.backup(&profile.id, file, self.host)?
        } else {
            self.repo.export(&profile.id, file)?
        };
        Ok(Outcome::success(
            json!({"ok":true,"id":profile.id,"name":profile.name,"file":destination.map(|p|p.to_string_lossy().into_owned())}),
        ))
    }
    pub fn import(&self, file: &Path, time: f64) -> Result<Outcome, RepoError> {
        Ok(Outcome::success(
            json!({"ok":true,"profile":self.repo.import_file(file,time)?}),
        ))
    }
}

pub fn summary(profile: &Profile) -> Value {
    let fp = &profile.fingerprint;
    json!({"id":profile.id,"name":profile.name,"engine":profile.engine,"os":fp.os,"locale":fp.locale,"timezone":fp.timezone,"mobile":fp.is_mobile,"tags":profile.tags,
        "proxy":profile.proxy.as_ref().map(|p|&p.server),"proxyCountry":profile.proxy.as_ref().and_then(|p|p.country.as_ref())})
}
pub(crate) fn parse_proxy(
    raw: Option<&str>,
    country: Option<&str>,
) -> Result<Option<Proxy>, RepoError> {
    let Some(raw) = raw.filter(|s| !s.is_empty()) else {
        return Ok(None);
    };
    let mut server = raw.to_owned();
    let mut username = None;
    let mut password = None;
    if raw.contains('@') {
        let (scheme, rest) = raw.split_once("://").unwrap_or(("http", raw));
        let (credentials, host) = rest
            .rsplit_once('@')
            .ok_or_else(|| RepoError::Message("Proxy credentials cannot be parsed".into()))?;
        if let Some((user, pass)) = credentials.split_once(':') {
            username = Some(user);
            password = Some(pass);
        }
        server = format!("{scheme}://{host}");
    }
    Proxy::from_record(
        &json!({"server":server,"username":username,"password":password,"country":country}),
        false,
    )
    .map(Some)
    .map_err(|e| RepoError::Message(e.to_string()))
}
