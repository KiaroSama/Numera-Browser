use super::{check_password, decrypt, is_envelope, journal::usable_descriptor};
use serde::Serialize;
use serde_json::Value;
use std::{
    collections::BTreeSet,
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Debug, Serialize)]
pub struct Problem {
    pub code: &'static str,
    pub detail: String,
}

pub(crate) struct Site {
    pub path: PathBuf,
    pub node: Option<&'static str>,
    pub field: &'static str,
    pub many: bool,
}

pub(crate) fn sites(root: &Path) -> io::Result<Vec<Site>> {
    let mut found = Vec::new();
    for (folder, field) in [("profiles", "password"), ("dashboard", "pass")] {
        let directory = root.join(folder);
        let entries = match fs::read_dir(&directory) {
            Ok(e) => e,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(e),
        };
        let mut paths = entries
            .map(|entry| entry.map(|e| e.path()))
            .collect::<io::Result<Vec<_>>>()?;
        paths.sort();
        for path in paths {
            if crate::storage::paths::json_filename(&path) && path.is_file() {
                found.push(Site {
                    path,
                    node: Some("proxy"),
                    field,
                    many: false,
                });
            }
        }
    }
    let path = root.join("proxies.json");
    if path.is_file() {
        found.push(Site {
            path,
            node: None,
            field: "pass",
            many: true,
        });
    }
    Ok(found)
}

pub(crate) fn holders<'a>(data: &'a Value, site: &Site) -> Option<Vec<&'a Value>> {
    let records: Vec<&Value> = if site.many {
        data.as_array()?.iter().collect()
    } else {
        vec![data]
    };
    let mut out = Vec::new();
    for record in records {
        record.as_object()?;
        let holder = match site.node {
            Some(node) => &record[node],
            None => record,
        };
        if holder.is_null() {
            continue;
        }
        holder.as_object()?;
        let value = &holder[site.field];
        if !value.is_null() && !value.is_string() {
            return None;
        }
        if value.as_str().is_some_and(|v| !v.is_empty()) {
            out.push(value);
        }
    }
    Some(out)
}

/// Read-only full-generation proof. A verifier alone does not prove the files.
pub fn problems(
    root: &Path,
    descriptor: &Value,
    password: Option<&str>,
) -> io::Result<Vec<Problem>> {
    let mut found = Vec::new();
    if !usable_descriptor(descriptor) {
        return Ok(vec![Problem {
            code: "descriptor_unusable",
            detail: "the descriptor has no usable salt and verifier".into(),
        }]);
    }
    let mut damaged = Vec::new();
    let mut plaintext = 0;
    let mut plaintext_files = BTreeSet::new();
    let mut stale_files = BTreeSet::new();
    for site in sites(root)? {
        let name = site
            .path
            .file_name()
            .ok_or_else(|| io::Error::other("Secret site has no name"))?
            .to_string_lossy()
            .into_owned();
        let data = fs::read(&site.path)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Value>(&bytes).ok());
        let Some(data) = data else {
            damaged.push(name.clone());
            stale_files.insert(site.path.clone());
            continue;
        };
        if holders(&data, &site).is_none() {
            damaged.push(name.clone());
        }
        // Structural damage does not erase independently readable ciphertext.
        let records: Vec<&Value> = if site.many && data.is_array() {
            data.as_array().into_iter().flatten().collect()
        } else {
            vec![&data]
        };
        for record in records {
            let Some(holder) = record.as_object().and_then(|record| match site.node {
                Some(node) => record.get(node)?.as_object(),
                None => Some(record),
            }) else {
                continue;
            };
            let Some(text) = holder
                .get(site.field)
                .and_then(Value::as_str)
                .filter(|v| !v.is_empty())
            else {
                continue;
            };
            if !is_envelope(text) {
                plaintext += 1;
                plaintext_files.insert(name.clone());
            } else if password.is_some_and(|p| {
                decrypt(p, descriptor["salt"].as_str().unwrap_or(""), text).is_err()
            }) {
                stale_files.insert(site.path.clone());
            }
        }
    }
    if !damaged.is_empty() {
        found.push(Problem {
            code: "unreadable_files",
            detail: format!(
                "{} secret-bearing file(s) cannot be read: {}",
                damaged.len(),
                damaged.join(", ")
            ),
        });
    } else if plaintext > 0 {
        found.push(Problem {
            code: "plaintext_left",
            detail: format!(
                "{plaintext} credential(s) are still unencrypted in {}",
                plaintext_files.into_iter().collect::<Vec<_>>().join(", ")
            ),
        });
    }
    let Some(password) = password else {
        found.push(Problem{code:"no_password",detail:"no master password was supplied, so whether the stored secrets open with this descriptor cannot be established".into()});
        return Ok(found);
    };
    if !check_password(
        password,
        descriptor["salt"].as_str().unwrap_or(""),
        descriptor["verifier"].as_str().unwrap_or(""),
    ) {
        found.push(Problem{code:"verifier_mismatch",detail:"the descriptor's verifier does not open with this password, so the descriptor does not describe this data".into()});
        return Ok(found);
    }
    if !stale_files.is_empty() {
        found.push(Problem {
            code: "wrong_generation",
            detail: format!(
                "{} file(s) hold secrets this descriptor does not open: {}",
                stale_files.len(),
                stale_files
                    .iter()
                    .filter_map(|path| path.file_name())
                    .map(|name| name.to_string_lossy().into_owned())
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
        });
    }
    Ok(found)
}
