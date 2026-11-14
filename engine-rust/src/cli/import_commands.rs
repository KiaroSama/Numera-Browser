use super::profile_commands::{Outcome, Profiles};
use crate::{import_format, importers, storage::repo::RepoError};
use serde_json::{Map, Value, json};
use std::{collections::BTreeMap, path::Path};

impl Profiles<'_> {
    pub fn import_from(&self, file: &Path, time: f64) -> Result<Outcome, RepoError> {
        let result = (|| -> Result<Vec<Value>, String> {
            let text = std::fs::read_to_string(file).map_err(|error| error.to_string())?;
            let raw = serde_json::from_str(&text).map_err(|error| error.to_string())?;
            import_format::vendor(&raw)
        })();
        let rows = match result {
            Ok(rows) => rows,
            Err(error) => {
                return Ok(Outcome::failed(format!(
                    "couldn't read {}: {error}",
                    file.display()
                )));
            }
        };
        let imported = self.import_vendor(&rows, time, || {
            loop {
                let mut bytes = [0u8; 4];
                getrandom::fill(&mut bytes).map_err(std::io::Error::other)?;
                let seed = u32::from_ne_bytes(bytes) & 0x7fff_ffff;
                if seed != 0 {
                    return Ok(i64::from(seed));
                }
            }
        });
        match imported {
            Ok(outcome) => Ok(outcome),
            Err(error) => Ok(Outcome::failed(format!(
                "couldn't read {}: {error}",
                file.display()
            ))),
        }
    }
    pub fn import_vendor(
        &self,
        rows: &[Value],
        time: f64,
        mut seed: impl FnMut() -> Result<i64, RepoError>,
    ) -> Result<Outcome, RepoError> {
        let mut pending = Vec::new();
        let mut issues = BTreeMap::new();
        for (index, row) in rows.iter().enumerate() {
            let (profile, problems) = importers::model(row, seed()?, time).map_err(|message| {
                RepoError::Message(if rows.len() > 1 {
                    message.replace("profile: ", &format!("profiles[{index}]: "))
                } else {
                    message
                })
            })?;
            issues.insert(profile.id.clone(), problems);
            pending.push(profile);
        }
        let expected = pending
            .iter()
            .map(|profile| (profile.id.clone(), profile.name.clone()))
            .collect::<Vec<_>>();
        let written = self.repo.save_many(pending, Map::new(), "skip", time)?;
        let skipped = expected
            .into_iter()
            .filter(|(id, _)| !written.iter().any(|p| &p.id == id))
            .map(|(_, name)| name)
            .collect::<Vec<_>>();
        if written.is_empty() && skipped.is_empty() {
            return Ok(Outcome::failed("no profiles found in that file"));
        }
        let mut results = Vec::new();
        let mut clean = 0;
        for profile in written {
            let problems = issues
                .remove(&profile.id)
                .ok_or_else(|| RepoError::Message("Imported profile issues missing".into()))?;
            clean += usize::from(problems.is_empty());
            results.push(json!({"id":profile.id,"name":profile.name,"issues":problems}));
        }
        Ok(Outcome::success(
            json!({"ok":true,"imported":results.len(),"coherent":clean,"skipped":skipped,"results":results}),
        ))
    }
}
