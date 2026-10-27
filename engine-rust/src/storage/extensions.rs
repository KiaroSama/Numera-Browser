use super::archive_paths::member_path;
use crate::conversion::{truth, value_text};
use serde::Serialize;
use serde_json::{Map, Value};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

#[derive(Serialize)]
pub struct Description {
    pub path: PathBuf,
    pub name: Value,
    pub icon: Option<PathBuf>,
}
#[derive(Serialize)]
pub struct Installed {
    pub id: String,
    #[serde(flatten)]
    pub description: Description,
}

fn json(path: &Path) -> io::Result<Option<Value>> {
    let bytes = match fs::read(path) {
        Ok(bytes) => bytes,
        Err(_) => return Ok(None),
    };
    let text = std::str::from_utf8(&bytes).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Extension resource is not UTF-8",
        )
    })?;
    Ok(serde_json::from_str(text).ok())
}

fn contained_file(folder: &Path, relative: &str) -> Option<PathBuf> {
    let path = member_path(folder, relative).ok()?;
    let base = fs::canonicalize(folder).ok()?;
    let physical = fs::canonicalize(&path).ok()?;
    if !physical.starts_with(&base) || !physical.is_file() {
        return None;
    }
    Some(path)
}

fn display_name(folder: &Path, manifest: Option<&Value>) -> io::Result<Value> {
    let fallback = || {
        Value::String(
            folder
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .into_owned(),
        )
    };
    let Some(manifest) = manifest else {
        return Ok(fallback());
    };
    let name = manifest
        .get("name")
        .filter(|value| truth(value))
        .map(value_text)
        .unwrap_or_default();
    if let Some(rest) = name.strip_prefix("__MSG_") {
        let key = rest.strip_suffix("__").unwrap_or(rest);
        let locale = manifest["default_locale"].as_str().unwrap_or("");
        for locale in ["en", "en_US", locale] {
            if locale.is_empty()
                || locale.contains(['/', '\\', ':'])
                || matches!(locale, "." | "..")
            {
                continue;
            }
            let relative = format!("_locales/{locale}/messages.json");
            if let Some(path) = contained_file(folder, &relative)
                && let Some(messages) = json(&path)?
                && let Some(value) = messages
                    .get(key)
                    .and_then(|entry| entry.get("message"))
                    .filter(|value| truth(value))
            {
                return Ok(value.clone());
            }
        }
        Ok(fallback())
    } else if name.is_empty() {
        Ok(fallback())
    } else {
        Ok(Value::String(name))
    }
}

fn icon_size(key: &str) -> (bool, String) {
    let integer = crate::decimal::integer(key)
        .filter(|value| value.trim_start_matches('-').len() <= 4300)
        .unwrap_or_else(|| "0".into());
    let negative = integer.starts_with('-');
    let magnitude = integer.trim_start_matches('-').trim_start_matches('0');
    if magnitude.is_empty() {
        (false, "0".into())
    } else {
        (negative, magnitude.into())
    }
}

fn select_icon(folder: &Path, manifest: Option<&Value>) -> Option<PathBuf> {
    let manifest = manifest?;
    let mut icons = manifest["icons"].as_object().cloned().unwrap_or_default();
    for action in ["action", "browser_action", "page_action"] {
        match &manifest[action]["default_icon"] {
            Value::Object(default) => {
                let mut merged: Map<String, Value> = default.clone();
                merged.extend(icons);
                icons = merged;
            }
            Value::String(default) => {
                icons
                    .entry("0")
                    .or_insert_with(|| Value::String(default.clone()));
            }
            _ => {}
        }
    }
    let mut choices: Vec<_> = icons
        .into_iter()
        .map(|(key, value)| (icon_size(&key), value))
        .collect();
    // Stable order preserves manifest insertion order for equally sized keys.
    choices.sort_by(|(left, _), (right, _)| {
        let magnitude = right
            .1
            .len()
            .cmp(&left.1.len())
            .then_with(|| right.1.cmp(&left.1));
        left.0.cmp(&right.0).then(if left.0 {
            magnitude.reverse()
        } else {
            magnitude
        })
    });
    choices
        .into_iter()
        .filter_map(|(_, value)| {
            value
                .as_str()
                .filter(|value| !value.is_empty())
                .and_then(|value| contained_file(folder, value))
        })
        .next()
}

pub(crate) fn icon(folder: &Path) -> io::Result<Option<PathBuf>> {
    let manifest = contained_file(folder, "manifest.json")
        .as_deref()
        .map(json)
        .transpose()?
        .flatten();
    Ok(select_icon(folder, manifest.as_ref()))
}

pub fn describe(folder: &Path) -> io::Result<Description> {
    let manifest_path = contained_file(folder, "manifest.json");
    let manifest = manifest_path.as_deref().map(json).transpose()?.flatten();
    Ok(Description {
        path: folder.to_owned(),
        name: display_name(folder, manifest.as_ref())?,
        icon: select_icon(folder, manifest.as_ref()),
    })
}

pub fn is_store_url(text: &str) -> bool {
    let text = text.trim().to_lowercase();
    text.contains("chromewebstore.google.com") || text.contains("chrome.google.com/webstore")
}

fn word(character: char) -> bool {
    character == '_' || character.is_alphanumeric()
}

pub fn extension_id(text: &str) -> io::Result<String> {
    let text = text.trim();
    if text.is_empty() {
        return Err(io::Error::other("no URL or extension id given"));
    }
    let characters: Vec<char> = text.chars().collect();
    for start in 0..characters.len() {
        if start > 0 && word(characters[start - 1]) {
            continue;
        }
        let end = start + 32;
        if end <= characters.len()
            && characters[start..end]
                .iter()
                .all(|character| ('a'..='p').contains(character))
            && (end == characters.len() || !word(characters[end]))
        {
            return Ok(characters[start..end].iter().collect());
        }
    }
    let detail = text.match_indices("/detail/").find_map(|(offset, _)| {
        let rest = &text[offset + 8..];
        let (name, id) = rest.split_once('/')?;
        if name.is_empty() {
            return None;
        }
        let candidate: String = id
            .chars()
            .take(40)
            .take_while(char::is_ascii_lowercase)
            .collect();
        (candidate.len() >= 20).then_some(candidate)
    });
    let near = detail.or_else(|| {
        for start in 0..characters.len() {
            if !characters[start].is_ascii_lowercase() || (start > 0 && word(characters[start - 1]))
            {
                continue;
            }
            let count = characters[start..]
                .iter()
                .take(40)
                .take_while(|character| character.is_ascii_lowercase())
                .count();
            if count >= 20
                && (start + count == characters.len() || !word(characters[start + count]))
            {
                return Some(characters[start..start + count].iter().collect());
            }
        }
        None
    });
    if let Some(candidate) = near {
        let bad: std::collections::BTreeSet<_> = candidate
            .chars()
            .filter(|character| !('a'..='p').contains(character))
            .collect();
        if !bad.is_empty() {
            return Err(io::Error::other(format!(
                "'{candidate}' contains {}, which cannot appear in a Chrome extension id - ids use only the letters a-p. This usually means the address was mistyped or partly copied; open the extension's page and copy the whole URL from the address bar.",
                bad.into_iter()
                    .map(|character| character.to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            )));
        }
        return Err(io::Error::other(format!(
            "'{candidate}' is {} characters; a Chrome extension id is exactly 32. It looks like the id was copied incompletely - open the extension's page and copy the whole URL from the address bar.",
            candidate.len()
        )));
    }
    Err(io::Error::other(format!(
        "could not find an extension id in {}. Paste the full Web Store URL, or the 32-character id on its own.",
        crate::validation::repr(&Value::String(text.into()))
    )))
}

pub fn unloadable(entries: &[String]) -> Vec<String> {
    let mut problems = Vec::new();
    for entry in entries
        .iter()
        .map(|entry| {
            entry.trim_matches(|c: char| c.is_whitespace() || matches!(c, '\u{1c}'..='\u{1f}'))
        })
        .filter(|entry| !entry.is_empty())
    {
        let quoted = crate::validation::repr(&Value::String(entry.into()));
        if entry.contains("://") {
            problems.push(format!("{quoted} is a URL. Install it first - paste it on the Extensions page, where the button reads Install for a link, or run `numera ext install <url>` - then the unpacked folder goes in this list."));
            continue;
        }
        let path = Path::new(entry);
        if !path.is_dir() {
            problems.push(format!("{quoted} is not a folder on this machine."));
        } else if !path.join("manifest.json").is_file() {
            problems.push(format!("{quoted} has no manifest.json, so Chromium will refuse it. Point at the folder that contains the manifest, not its parent."));
        }
    }
    problems
}

pub fn installed(root: &Path) -> io::Result<Vec<Installed>> {
    let directory = root.join("extensions");
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut paths = entries
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    paths.sort();
    let mut found = Vec::new();
    for path in paths {
        if contained_file(&path, "manifest.json").is_some() {
            let id = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or_else(|| io::Error::other("Extension folder name is not UTF8"))?
                .to_owned();
            found.push(Installed {
                id,
                description: describe(&path)?,
            });
        }
    }
    Ok(found)
}
