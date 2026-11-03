use super::{atomic::Batch, locks::DataLock};
use serde_json::{Value, json};
use std::{
    collections::{HashMap, HashSet},
    fs, io,
    path::PathBuf,
};

// Python strip also recognizes these four ASCII separators as whitespace.
fn trim_name(name: &str) -> &str {
    name.trim_matches(|c: char| c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}'))
}

pub struct Folders {
    root: PathBuf,
}
impl Folders {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
    fn meta_path(&self) -> PathBuf {
        self.root.join("dashboard-config.json")
    }
    fn metadata(&self) -> io::Result<Value> {
        let value = match fs::read(self.meta_path()) {
            Ok(bytes) => {
                let text = std::str::from_utf8(&bytes).map_err(|_| {
                    io::Error::new(
                        io::ErrorKind::InvalidData,
                        "Stored folder metadata is not UTF-8; nothing was written",
                    )
                })?;
                serde_json::from_str::<Value>(text).unwrap_or_else(|_| json!({}))
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => json!({}),
            Err(error) => return Err(error),
        };
        if !value.is_object() {
            return Err(io::Error::other(
                "Stored folder metadata is not an object; nothing was written",
            ));
        }
        Ok(value)
    }
    fn profiles(&self) -> io::Result<Vec<(PathBuf, Value)>> {
        let directory = self.root.join("dashboard");
        if !directory.is_dir() {
            return Ok(Vec::new());
        }
        let mut paths = fs::read_dir(directory)?
            .map(|entry| entry.map(|entry| entry.path()))
            .collect::<io::Result<Vec<_>>>()?;
        paths.sort();
        let mut found = Vec::new();
        for path in paths {
            if !super::paths::json_filename(&path) {
                continue;
            }
            let bytes = fs::read(&path)?;
            let text = std::str::from_utf8(&bytes).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Stored folder profile is not UTF-8; nothing was written",
                )
            })?;
            if let Ok(value) = serde_json::from_str::<Value>(text) {
                if !value.is_object() {
                    return Err(io::Error::other(
                        "Stored folder profile is not an object; nothing was written",
                    ));
                }
                found.push((path, value));
            }
        }
        Ok(found)
    }
    pub fn listing(&self) -> io::Result<Vec<String>> {
        let metadata = self.metadata()?;
        let mut names = Vec::new();
        if let Some(configured) = metadata.get("folders") {
            for name in configured
                .as_array()
                .ok_or_else(|| io::Error::other("Stored folder list is not an array"))?
            {
                names.push(
                    name.as_str()
                        .ok_or_else(|| io::Error::other("Stored folder name is not text"))?
                        .to_owned(),
                );
            }
        }
        for (_, profile) in self.profiles()? {
            if crate::conversion::truth(&profile["folder"]) {
                names.push(
                    profile["folder"]
                        .as_str()
                        .ok_or_else(|| io::Error::other("Stored profile folder is not text"))?
                        .to_owned(),
                );
            }
        }
        let mut seen = HashSet::new();
        names.retain(|name| seen.insert(name.clone()));
        Ok(names)
    }
    pub fn resolve_name(&self, name: &str) -> io::Result<String> {
        if name.is_empty() {
            return Ok(name.into());
        }
        let meta = self.metadata()?;
        let renamed = Self::renamed(&meta)?;
        let mut name = name.to_owned();
        let mut seen = HashSet::new();
        while seen.insert(name.clone()) {
            let Some(next) = renamed.get(&name) else {
                break;
            };
            name = next.clone();
        }
        Ok(name)
    }
    fn renamed(meta: &Value) -> io::Result<HashMap<String, String>> {
        if !crate::conversion::truth(&meta["renamed"]) {
            return Ok(HashMap::new());
        }
        meta["renamed"]
            .as_object()
            .ok_or_else(|| io::Error::other("Stored folder rename map is not an object"))?
            .iter()
            .map(|(name, value)| {
                Ok((
                    name.clone(),
                    value
                        .as_str()
                        .ok_or_else(|| io::Error::other("Stored folder rename target is not text"))?
                        .to_owned(),
                ))
            })
            .collect()
    }
    fn persist(
        &self,
        batch: &mut Batch,
        names: Vec<String>,
        retire: Option<(&str, &str)>,
    ) -> io::Result<Vec<String>> {
        let mut meta = self.metadata()?;
        let mut seen = HashSet::new();
        let names: Vec<_> = names
            .into_iter()
            .filter(|name| !name.is_empty() && seen.insert(name.clone()))
            .collect();
        let mut renamed = Self::renamed(&meta)?;
        if let Some((old, new)) = retire {
            for target in renamed.values_mut() {
                if target == old {
                    *target = new.into();
                }
            }
            renamed.insert(old.into(), new.into());
        }
        for name in &names {
            renamed.remove(name);
        }
        renamed.retain(|old, new| old != new);
        meta["folders"] = serde_json::to_value(&names).map_err(io::Error::other)?;
        if renamed.is_empty() {
            meta.as_object_mut()
                .ok_or_else(|| io::Error::other("Folder metadata lost its object shape"))?
                .remove("renamed");
        } else {
            meta["renamed"] = serde_json::to_value(renamed).map_err(io::Error::other)?;
        }
        batch.write(&self.meta_path(), &meta)?;
        Ok(names)
    }
    fn change(
        &self,
        update: impl FnOnce(&mut Batch) -> io::Result<Vec<String>>,
    ) -> io::Result<Vec<String>> {
        // Match source lock order; root covers profiles appearing during enumeration.
        let file = DataLock::for_file(&self.meta_path())?;
        let _file = file.acquire_configured()?;
        let root = DataLock::transaction(&self.root)?;
        let _root = root.acquire_configured()?;
        let mut batch = Batch::default();
        match update(&mut batch) {
            Ok(names) => Ok(names),
            Err(error) => {
                let failures = batch.rollback();
                if failures.is_empty() {
                    Err(error)
                } else {
                    Err(io::Error::other(format!(
                        "Folder transaction failed: {error}; {} file(s) could not be restored",
                        failures.len()
                    )))
                }
            }
        }
    }
    pub fn create(&self, name: &str) -> io::Result<Vec<String>> {
        self.change(|batch| {
            let name = trim_name(name);
            if name.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "folder name required",
                ));
            }
            let mut names = self.listing()?;
            names.push(name.into());
            self.persist(batch, names, None)
        })
    }
    pub fn rename(&self, source: &str, target: &str) -> io::Result<Vec<String>> {
        self.change(|batch| {
            let (source, target) = (trim_name(source), trim_name(target));
            if source.is_empty() || target.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "both a source and a destination name are required",
                ));
            }
            for (path, mut profile) in self.profiles()? {
                if profile["folder"] == source {
                    profile["folder"] = target.into();
                    batch.write(&path, &profile)?;
                }
            }
            let names = self
                .listing()?
                .into_iter()
                .map(|name| if name == source { target.into() } else { name })
                .collect();
            self.persist(batch, names, Some((source, target)))
        })
    }
    pub fn delete(&self, name: &str, target: Option<&str>) -> io::Result<Vec<String>> {
        self.change(|batch| {
            let name = trim_name(name);
            if name.is_empty() {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "folder name required",
                ));
            }
            let target = trim_name(target.unwrap_or("Unfiled"));
            let target = if target.is_empty() { "Unfiled" } else { target };
            for (path, mut profile) in self.profiles()? {
                if profile["folder"] == name {
                    profile["folder"] = target.into();
                    batch.write(&path, &profile)?;
                }
            }
            self.persist(
                batch,
                self.listing()?
                    .into_iter()
                    .filter(|item| item != name)
                    .collect(),
                None,
            )
        })
    }
}
