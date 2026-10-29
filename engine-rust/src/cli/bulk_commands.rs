use super::profile_commands::{Outcome, Profiles, parse_proxy};
use crate::{bulk, conversion::value_text, storage::repo::RepoError};
use serde_json::{Map, Value, json};

fn strings(args: &Map<String, Value>, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}
fn entropy() -> Result<i64, RepoError> {
    loop {
        let mut bytes = [0u8; 4];
        getrandom::fill(&mut bytes).map_err(std::io::Error::other)?;
        let value = u32::from_ne_bytes(bytes) & 0x7fff_ffff;
        if value != 0 {
            return Ok(value.into());
        }
    }
}
fn whole(args: &Map<String, Value>, key: &str, default: i64) -> Result<i64, RepoError> {
    match args.get(key).filter(|v| !v.is_null()) {
        None => Ok(default),
        Some(value) => value.as_i64().ok_or_else(|| {
            RepoError::Message(format!("{key} is outside the native supported range"))
        }),
    }
}

impl Profiles<'_> {
    pub fn bulk_create(&self, args: &Map<String, Value>, time: f64) -> Result<Outcome, RepoError> {
        let count = whole(args, "count", 0)?.max(0) as usize;
        let oses = strings(args, "os");
        let locales = strings(args, "locale");
        let versions = strings(args, "chrome");
        let tags = strings(args, "tag");
        let explicit = args
            .get("engine")
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty());
        let engine = match explicit {
            Some(engine) => engine.to_owned(),
            None => value_text(&self.config.default_engine(self.installed)?),
        };
        let seed = match args.get("seed").filter(|v| !v.is_null()) {
            Some(value) => value
                .as_number()
                .cloned()
                .ok_or_else(|| RepoError::Message("Batch seed must be an integer".into()))?,
            None => entropy()?.into(),
        };
        let profiles = bulk::generate(
            bulk::GenerateBatch {
                count,
                seed,
                oses: &oses,
                locales: &locales,
                chrome_versions: &versions,
                engine: &engine,
                prefix: args
                    .get("prefix")
                    .and_then(Value::as_str)
                    .unwrap_or("Profile"),
                start: whole(args, "start", 1)?,
                pad: whole(args, "pad", 4)?.max(0) as usize,
                tags: &tags,
                mobile: args.get("mobile") == Some(&Value::Bool(true)),
            },
            || crate::storage::archive_paths::random_hex(6),
            time,
        )?;
        let changes = explicit
            .map(|v| Map::from_iter([("default_engine".into(), v.into())]))
            .unwrap_or_default();
        let saved = self.repo.save_many(profiles, changes, "skip", time)?;
        Ok(Outcome::success(
            json!({"created":saved.len(),"skipped":count-saved.len(),"engine":engine,"ids":saved.iter().map(|p|&p.id).collect::<Vec<_>>()}),
        ))
    }
    pub fn apply(
        &self,
        args: &Map<String, Value>,
        home: &std::path::Path,
        time: f64,
        confirm: impl FnOnce(usize) -> Result<bool, RepoError>,
    ) -> Result<Outcome, RepoError> {
        self.apply_observed(args, home, time, confirm, |_, _| Ok(()), |_, _| Ok(()))
    }
    pub fn apply_with_output(
        &self,
        args: &Map<String, Value>,
        home: &std::path::Path,
        time: f64,
        confirm: impl FnOnce(usize) -> Result<bool, RepoError>,
        output: &mut super::output::Output<impl std::io::Write, impl std::io::Write>,
    ) -> Result<Outcome, RepoError> {
        let output = std::cell::RefCell::new(output);
        self.apply_observed(
            args,
            home,
            time,
            confirm,
            |text, tone| output.borrow_mut().say(text, tone).map_err(RepoError::from),
            |profiles, touched| {
                for profile in profiles {
                    let fields = touched
                        .get(&profile.id)
                        .and_then(Value::as_array)
                        .ok_or_else(|| RepoError::Message("Applied fields missing".into()))?;
                    let fields = fields.iter().map(value_text).collect::<Vec<_>>().join(", ");
                    let mut output = output.borrow_mut();
                    let name = value_text(&profile.name);
                    let spaces = " ".repeat(24usize.saturating_sub(name.chars().count()));
                    let fields = output.style(&fields, super::output::Tone::Dim);
                    output.say(&format!("  {name}{spaces}{fields}"), None)?;
                }
                Ok(())
            },
        )
    }
    fn apply_observed(
        &self,
        args: &Map<String, Value>,
        home: &std::path::Path,
        time: f64,
        confirm: impl FnOnce(usize) -> Result<bool, RepoError>,
        mut event: impl FnMut(&str, Option<super::output::Tone>) -> Result<(), RepoError>,
        completed: impl FnOnce(&[crate::models::Profile], &Map<String, Value>) -> Result<(), RepoError>,
    ) -> Result<Outcome, RepoError> {
        let targets = bulk::select(
            &self.repo.list(time)?,
            &strings(args, "ref"),
            args.get("tag").and_then(Value::as_str),
            args.get("all") == Some(&Value::Bool(true)),
        );
        if targets.is_empty() {
            event(
                "Nothing matched. Pass profile names/ids, --tag TAG, or --all.",
                Some(super::output::Tone::Red),
            )?;
            return Ok(Outcome::failed("no profiles matched"));
        }
        let mut patch = Map::new();
        for key in ["engine", "notes", "locale", "timezone"] {
            if let Some(value) = args.get(key).filter(|v| !v.is_null()) {
                patch.insert(key.into(), value.clone());
            }
        }
        let startup = strings(args, "startup_url");
        if !startup.is_empty() {
            patch.insert("startup_urls".into(), json!(startup));
        }
        if args.get("extension").is_some_and(Value::is_array) {
            let paths = strings(args, "extension")
                .into_iter()
                .filter(|v| !v.is_empty())
                .map(|v| super::settings_commands::expand(&v, home))
                .collect::<Result<Vec<_>, _>>()?;
            patch.insert("extensions".into(), json!(paths));
        }
        for (src, dst) in [("add_tag", "add_tags"), ("remove_tag", "remove_tags")] {
            let values = strings(args, src);
            if !values.is_empty() {
                patch.insert(dst.into(), json!(values));
            }
        }
        let regen = args.get("regen") == Some(&Value::Bool(true));
        if regen {
            patch.insert("regen".into(), true.into());
        }
        if let Some(proxy) = args.get("proxy").and_then(Value::as_str) {
            patch.insert(
                "proxy".into(),
                serde_json::to_value(parse_proxy(
                    Some(proxy),
                    args.get("country").and_then(Value::as_str),
                )?)
                .map_err(std::io::Error::other)?,
            );
        }
        if patch.is_empty() {
            event(
                "No changes given. Try --engine, --proxy, --locale, --add-tag, --regen…",
                Some(super::output::Tone::Red),
            )?;
            return Ok(Outcome::failed("no changes given"));
        }
        if args.get("yes") != Some(&Value::Bool(true)) {
            event(
                &format!("About to change {} profile(s):", targets.len()),
                Some(super::output::Tone::Bold),
            )?;
            for profile in targets.iter().take(10) {
                event(&format!("  {}", value_text(&profile.name)), None)?;
            }
            if targets.len() > 10 {
                event(
                    &format!("  … and {} more", targets.len() - 10),
                    Some(super::output::Tone::Dim),
                )?;
            }
            if !confirm(targets.len())? {
                event("Aborted.", None)?;
                return Ok(Outcome::success(
                    json!({"ok":false,"aborted":true,"matched":targets.len()}),
                ));
            }
        }
        let changes = patch
            .get("engine")
            .filter(|v| crate::conversion::truth(v))
            .map(|v| Map::from_iter([("default_engine".into(), v.clone())]))
            .unwrap_or_default();
        let patch = Value::Object(patch);
        let mut touched = Map::new();
        let saved = self.repo.patch_many(
            &targets.iter().map(|p| p.id.clone()).collect::<Vec<_>>(),
            |profile| {
                let fields =
                    bulk::apply(profile, &patch, if regen { Some(entropy()?) } else { None })?;
                if fields.is_empty() {
                    return Ok(false);
                }
                touched.insert(profile.id.clone(), json!(fields));
                Ok(true)
            },
            changes,
            "reject",
            time,
        )?;
        completed(&saved, &touched)?;
        event(
            &format!("Updated {} of {} profile(s).", saved.len(), targets.len()),
            Some(super::output::Tone::Green),
        )?;
        Ok(Outcome::success(
            json!({"ok":true,"matched":targets.len(),"updated":saved.len(),"changes":touched}),
        ))
    }
    pub fn regen(
        &self,
        reference: &str,
        os: Option<&str>,
        locale: Option<&str>,
        time: f64,
    ) -> Result<Outcome, RepoError> {
        let mut profiles = self.repo.patch_many(
            &[reference.into()],
            |profile| {
                profile.fingerprint =
                    crate::fingerprint::generate(crate::fingerprint::GenerateOptions {
                        seed: entropy()?,
                        os,
                        locale,
                        country: profile.proxy.as_ref().and_then(|p| p.country.as_deref()),
                        mobile: false,
                    })
                    .map_err(|e| RepoError::Message(e.to_string()))?;
                Ok(true)
            },
            Map::new(),
            "reject",
            time,
        )?;
        Ok(match profiles.pop() {
            Some(profile) => Outcome::success(json!({"ok":true,"profile":profile})),
            None => Outcome::failed(format!("no profile matching '{reference}'")),
        })
    }
    pub fn restore(
        &self,
        file: &std::path::Path,
        new_id: bool,
        time: f64,
    ) -> Result<Outcome, RepoError> {
        Ok(Outcome::success(
            json!({"ok":true,"profile":self.repo.restore(file,new_id,self.host,time)?}),
        ))
    }
}
