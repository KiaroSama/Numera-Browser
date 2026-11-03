use super::profile_commands::{Outcome, Profiles};
use crate::{
    bulk,
    conversion::truth,
    devices, pool,
    proxy::Proxy,
    storage::{atomic::write_bytes, pool::ProxyPool, repo::RepoError},
};
use serde_json::{Map, Value, json};
use std::path::Path;

pub struct Pool<'a> {
    pub store: &'a ProxyPool,
    pub profiles: &'a Profiles<'a>,
}
fn strings(args: &Map<String, Value>, key: &str) -> Vec<String> {
    args.get(key)
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}
impl Pool<'_> {
    fn locked(&self, rows: &[Value]) -> Result<Option<Outcome>, RepoError> {
        let mut count = 0;
        for row in rows {
            if self
                .store
                .secret_withheld(row["id"].as_str().unwrap_or(""))?
            {
                count += 1;
            }
        }
        Ok((count > 0).then(|| {
            Outcome::failed(format!(
                "{count} of these proxies have a password that cannot be read: {}",
                crate::vault::policy::LOCKED_MESSAGE
            ))
        }))
    }
    pub fn list(&self) -> Result<Outcome, RepoError> {
        let rows = self.store.list()?;
        Ok(Outcome::success(json!({"total":rows.len(),"proxies":rows})))
    }
    pub fn test(
        &self,
        ids: &[String],
        everything: bool,
        check: impl FnMut(&Value) -> std::io::Result<Value>,
        now: impl FnMut() -> f64,
    ) -> Result<Outcome, RepoError> {
        self.test_observed(ids, everything, check, now, |_, _| Ok(()))
    }
    pub fn test_with_output(
        &self,
        ids: &[String],
        everything: bool,
        check: impl FnMut(&Value) -> std::io::Result<Value>,
        now: impl FnMut() -> f64,
        output: &mut super::output::Output<impl std::io::Write, impl std::io::Write>,
    ) -> Result<Outcome, RepoError> {
        self.test_observed(ids, everything, check, now, |proxy, result| {
            super::pool_output::checked(output, proxy, result)
        })
    }
    fn test_observed(
        &self,
        ids: &[String],
        everything: bool,
        mut check: impl FnMut(&Value) -> std::io::Result<Value>,
        mut now: impl FnMut() -> f64,
        mut event: impl FnMut(&Value, &Value) -> Result<(), RepoError>,
    ) -> Result<Outcome, RepoError> {
        let targets = if everything {
            self.store.list()?
        } else {
            ids.iter()
                .map(|id| self.store.get(id))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .flatten()
                .collect()
        };
        if targets.is_empty() {
            return Ok(Outcome::failed("nothing to test"));
        }
        if let Some(refusal) = self.locked(&targets)? {
            return Ok(refusal);
        }
        let mut results = Vec::new();
        let mut failed = 0;
        for proxy in targets {
            let mut result = check(&proxy)?;
            let status = if truth(&result["ok"]) {
                "ok"
            } else if result["ok"].is_null() {
                "skipped"
            } else {
                "failed"
            };
            failed += usize::from(status == "failed");
            self.store.update(
                proxy["id"]
                    .as_str()
                    .ok_or_else(|| RepoError::InvalidRecord("Proxy ID is not text".into()))?,
                Map::from_iter([
                    ("status".into(), status.into()),
                    ("lastCheck".into(), result.clone()),
                    ("checkedAt".into(), now().into()),
                ]),
            )?;
            result["id"] = proxy["id"].clone();
            result["status"] = status.into();
            event(&proxy, &result)?;
            results.push(result);
        }
        Ok(Outcome {
            exit: i32::from(failed > 0),
            payload: json!({"tested":results.len(),"failed":failed,"results":results}),
        })
    }
    pub fn rotate(
        &self,
        ids: &[String],
        rotate: impl FnMut(&Value) -> std::io::Result<Value>,
        now: impl FnMut() -> f64,
    ) -> Result<Outcome, RepoError> {
        self.rotate_observed(ids, rotate, now, |_, _| Ok(()))
    }
    pub fn rotate_with_output(
        &self,
        ids: &[String],
        rotate: impl FnMut(&Value) -> std::io::Result<Value>,
        now: impl FnMut() -> f64,
        output: &mut super::output::Output<impl std::io::Write, impl std::io::Write>,
    ) -> Result<Outcome, RepoError> {
        self.rotate_observed(ids, rotate, now, |proxy, result| {
            super::pool_output::rotated(output, proxy, result)
        })
    }
    fn rotate_observed(
        &self,
        ids: &[String],
        mut rotate: impl FnMut(&Value) -> std::io::Result<Value>,
        mut now: impl FnMut() -> f64,
        mut event: impl FnMut(&Value, &Value) -> Result<(), RepoError>,
    ) -> Result<Outcome, RepoError> {
        let targets = ids
            .iter()
            .map(|id| self.store.get(id))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        if targets.is_empty() {
            return Ok(Outcome::failed("no proxy with that id"));
        }
        if let Some(refusal) = self.locked(&targets)? {
            return Ok(refusal);
        }
        let mut results = Vec::new();
        let mut moved = 0;
        for proxy in targets {
            let mut result = rotate(&proxy)?;
            let mut patch = Map::new();
            if truth(&result["rotated"]) {
                patch.insert("rotatedAt".into(), now().into());
                moved += 1;
            }
            // The original CLI records movement/health, not HTTP rotateTriedAt.
            if !result["ok"].is_null() {
                patch.insert(
                    "status".into(),
                    if truth(&result["ok"]) { "ok" } else { "failed" }.into(),
                );
                patch.insert("lastCheck".into(), result.clone());
                patch.insert("checkedAt".into(), now().into());
            }
            if !patch.is_empty() {
                self.store.update(
                    proxy["id"]
                        .as_str()
                        .ok_or_else(|| RepoError::InvalidRecord("Proxy ID is not text".into()))?,
                    patch,
                )?;
            }
            result["id"] = proxy["id"].clone();
            event(&proxy, &result)?;
            results.push(result);
        }
        Ok(Outcome {
            exit: i32::from(moved == 0),
            payload: json!({"rotated":moved,"tried":results.len(),"results":results}),
        })
    }
    pub fn add(&self, text: &str) -> Result<Outcome, RepoError> {
        let parsed = match pool::normalize_many(&json!(pool::parse_many(text))) {
            Ok(rows) => rows,
            Err(error) => return Ok(Outcome::failed(error.to_string())),
        };
        if parsed.is_empty() {
            return Ok(Outcome::failed(
                "no proxies could be parsed from that input",
            ));
        }
        let count = parsed.len();
        let added = self.store.add_many(parsed)?;
        Ok(Outcome::success(
            json!({"added":added.len(),"parsed":count,"total":self.store.list()?.len(),"proxies":added}),
        ))
    }
    pub fn remove(&self, ids: &[String]) -> Result<Outcome, RepoError> {
        let mut removed = Vec::new();
        for id in ids {
            if self.store.delete(id)? {
                removed.push(id.clone());
            }
        }
        let missing = ids
            .iter()
            .filter(|id| !removed.contains(id))
            .collect::<Vec<_>>();
        Ok(Outcome {
            exit: i32::from(removed.is_empty()),
            payload: json!({"removed":removed.len(),"ids":removed,"missing":missing,"total":self.store.list()?.len()}),
        })
    }
    pub fn archive(&self, ids: &[String], restore: bool) -> Result<Outcome, RepoError> {
        let mut changed = Vec::new();
        for id in ids {
            if self.store.get(id)?.is_some()
                && let Some(row) = self
                    .store
                    .update(id, Map::from_iter([("archived".into(), (!restore).into())]))?
            {
                changed.push(row["id"].clone());
            }
        }
        let mut payload = Map::new();
        payload.insert(
            if restore { "restored" } else { "archived" }.into(),
            changed.len().into(),
        );
        payload.insert("ids".into(), json!(changed));
        Ok(Outcome {
            exit: i32::from(changed.is_empty()),
            payload: Value::Object(payload),
        })
    }
    pub fn edit(&self, id: &str, args: &Map<String, Value>) -> Result<Outcome, RepoError> {
        let Some(mut current) = self.store.get(id)? else {
            return Ok(Outcome::failed("no proxy with that id"));
        };
        let mut patch = Map::new();
        for (arg, field) in [
            ("label", "label"),
            ("type", "type"),
            ("host", "host"),
            ("port", "port"),
            ("user", "user"),
            ("password", "pass"),
            ("country", "country"),
            ("rotate_url", "rotateUrl"),
        ] {
            if let Some(value) = args.get(arg).filter(|v| !v.is_null()) {
                patch.insert(field.into(), value.clone());
            }
        }
        if patch.is_empty() {
            return Ok(Outcome::failed("nothing to change"));
        }
        current
            .as_object_mut()
            .ok_or_else(|| RepoError::InvalidRecord("Stored pool row is not an object".into()))?
            .extend(patch.clone());
        let clean = match pool::normalize(&current) {
            Ok(row) => row,
            Err(error) => return Ok(Outcome::failed(error.to_string())),
        };
        for field in ["type", "host", "port", "user", "country"] {
            if patch.contains_key(field) {
                patch.insert(field.into(), clean[field].clone());
            }
        }
        if ["host", "port", "user", "pass", "type"]
            .iter()
            .any(|field| patch.contains_key(*field))
        {
            patch.extend(Map::from_iter([
                ("status".into(), "untested".into()),
                ("lastCheck".into(), Value::Null),
                ("checkedAt".into(), Value::Null),
            ]));
        }
        Ok(Outcome::success(
            json!({"ok":true,"proxy":self.store.update(id,patch)?}),
        ))
    }
    pub fn export(&self, ids: &[String], file: Option<&Path>) -> Result<Outcome, RepoError> {
        let rows = self
            .store
            .list()?
            .into_iter()
            .filter(|row| ids.is_empty() || ids.iter().any(|id| row["id"] == *id))
            .collect::<Vec<_>>();
        if let Some(refusal) = self.locked(&rows)? {
            return Ok(refusal);
        }
        let lines = rows
            .iter()
            .map(pool::to_line)
            .collect::<Result<Vec<_>, _>>()?;
        let text = if lines.is_empty() {
            String::new()
        } else {
            format!("{}\n", lines.join("\n"))
        };
        if let Some(file) = file {
            write_bytes(file, text.as_bytes())?;
        }
        Ok(Outcome::success(
            json!({"exported":lines.len(),"file":file,"text":text}),
        ))
    }
    pub fn assign(&self, args: &Map<String, Value>, time: f64) -> Result<Outcome, RepoError> {
        self.assign_observed(args, time, |_, _| Ok(()))
    }
    pub fn assign_with_output(
        &self,
        args: &Map<String, Value>,
        time: f64,
        output: &mut super::output::Output<impl std::io::Write, impl std::io::Write>,
    ) -> Result<Outcome, RepoError> {
        self.assign_observed(args, time, |row, locale| {
            super::pool_output::assigned(output, row, locale)
        })
    }
    fn assign_observed(
        &self,
        args: &Map<String, Value>,
        time: f64,
        mut event: impl FnMut(&Value, Option<&str>) -> Result<(), RepoError>,
    ) -> Result<Outcome, RepoError> {
        let ids = strings(args, "proxy_id");
        let rows = if ids.is_empty() {
            self.store.list()?
        } else {
            ids.iter()
                .map(|id| self.store.get(id))
                .collect::<Result<Vec<_>, _>>()?
                .into_iter()
                .flatten()
                .collect()
        };
        if rows.is_empty() {
            return Ok(Outcome::failed("the proxy pool is empty"));
        }
        if let Some(refusal) = self.locked(&rows)? {
            return Ok(refusal);
        }
        let targets = bulk::select(
            &self.profiles.repo.list(time)?,
            &strings(args, "ref"),
            args.get("tag").and_then(Value::as_str),
            args.get("all") == Some(&Value::Bool(true)),
        );
        if targets.is_empty() {
            return Ok(Outcome::failed("no profiles matched"));
        }
        let references = targets.iter().map(|p| p.id.clone()).collect::<Vec<_>>();
        let catalog = &devices::catalog().map_err(std::io::Error::other)?["catalog"];
        let mut assigned = Vec::new();
        let mut locale_notes = Vec::new();
        self.profiles.repo.patch_many(
            &references,
            |profile| {
                let index = references.iter().position(|id| id == &profile.id)
                    .ok_or_else(|| RepoError::Message("Pool assignment target changed".into()))?;
                let row = &rows[index % rows.len()];
                let kind = row["type"].as_str().unwrap_or("HTTP");
                let host = row["host"].as_str()
                    .ok_or_else(|| RepoError::InvalidRecord("Pool host is not text".into()))?;
                let port = crate::conversion::value_text(&row["port"]);
                let cc = row["country"].as_str().unwrap_or("").to_uppercase();
                let optional = |key: &str| if truth(&row[key]) { row[key].clone() } else { Value::Null };
                let proxy = json!({
                    "server": pool::server_url(kind, host, &port)?,
                    "username": optional("user"), "password": optional("pass"),
                    "country": if cc.is_empty() { Value::Null } else { cc.clone().into() }
                });
                profile.proxy = Some(Proxy::from_record(&proxy, false).map_err(std::io::Error::other)?);
                if let Some(locale) = catalog["COUNTRY_TO_LOCALE"][&cc].as_str() {
                    profile.fingerprint.locale = locale.into();
                    profile.fingerprint.timezone = catalog["LOCALES"][locale][0].as_str()
                        .ok_or_else(|| RepoError::InvalidRecord("Locale timezone missing".into()))?.into();
                    profile.fingerprint.languages = serde_json::from_value(catalog["LOCALES"][locale][1].clone())
                        .map_err(std::io::Error::other)?;
                }
                locale_notes.push(catalog["COUNTRY_TO_LOCALE"][&cc].as_str().map(str::to_owned));
                assigned.push(json!({"id":profile.id,"name":profile.name,
                    "proxy":profile.proxy.as_ref().map(|p| &p.server),"locale":profile.fingerprint.locale}));
                Ok(true)
            },
            Map::new(), "reject", time,
        )?;
        for (row, locale) in assigned.iter().zip(&locale_notes) {
            event(row, locale.as_deref())?;
        }
        Ok(Outcome::success(
            json!({"assigned":assigned.len(),"pool":rows.len(),"profiles":assigned}),
        ))
    }
}
