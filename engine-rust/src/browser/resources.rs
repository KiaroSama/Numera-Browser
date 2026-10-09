use crate::{models::Profile, ownership::session::Session, storage::atomic::write_json};
use serde_json::{Value, json};
use std::{fs, io, path::Path};

fn clear_paths(key: &str) -> &'static [&'static str] {
    match key {
        "history" => &[
            "Default/History",
            "Default/History-journal",
            "Default/Top Sites",
            "Default/Top Sites-journal",
            "Default/Visited Links",
        ],
        "passwords" => &[
            "Default/Login Data",
            "Default/Login Data-journal",
            "Default/Login Data For Account",
            "Default/Login Data For Account-journal",
        ],
        "cookies" => &["Default/Network/Cookies", "Default/Network/Cookies-journal"],
        "localStorage" => &["Default/Local Storage"],
        "indexedDb" => &["Default/IndexedDB"],
        "sessions" => &[
            "Default/Sessions",
            "Default/Current Session",
            "Default/Current Tabs",
            "Default/Last Session",
            "Default/Last Tabs",
        ],
        "extensionData" => &[
            "Default/Local Extension Settings",
            "Default/Extension State",
            "Default/Sync Extension Settings",
        ],
        "bookmarks" => &["Default/Bookmarks", "Default/Bookmarks.bak"],
        _ => &[],
    }
}

fn contained(root: &Path, relative: &str) -> io::Result<std::path::PathBuf> {
    let mut path = root.to_owned();
    for component in std::iter::once(root.to_owned()).chain(relative.split('/').scan(
        root.to_owned(),
        |path, component| {
            path.push(component);
            Some(path.clone())
        },
    )) {
        match fs::symlink_metadata(&component) {
            Ok(metadata) if metadata.file_type().is_symlink() => {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Browser preparation refuses symbolic-link resources",
                ));
            }
            Ok(_) => {}
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        path = component;
    }
    Ok(path)
}

fn seed(profile: &Profile, root: &Path, now: f64) -> io::Result<()> {
    let wanted: Vec<_> = profile
        .bookmarks
        .iter()
        .filter(|bookmark| {
            bookmark.is_object() && bookmark["url"].as_str().is_some_and(|url| !url.is_empty())
        })
        .collect();
    if wanted.is_empty() {
        return Ok(());
    }
    let path = contained(root, "Default/Bookmarks")?;
    let mut document = match fs::read(&path) {
        Ok(bytes) => serde_json::from_slice::<Value>(&bytes).ok(),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(error),
    }.filter(|value| value["roots"]["bookmark_bar"].is_object()).unwrap_or_else(|| json!({
        "roots": {
            "bookmark_bar":{"children":[],"date_added":"0","id":"1","name":"Bookmarks bar","type":"folder"},
            "other":{"children":[],"date_added":"0","id":"2","name":"Other bookmarks","type":"folder"},
            "synced":{"children":[],"date_added":"0","id":"3","name":"Mobile bookmarks","type":"folder"}
        },"version":1
    }));
    let bar = &mut document["roots"]["bookmark_bar"];
    if bar.get("children").is_none() {
        bar["children"] = json!([]);
    }
    let bar_id = bar["id"]
        .as_str()
        .unwrap_or("1")
        .parse::<u64>()
        .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Bookmark bar id is invalid"))?;
    let children = bar["children"].as_array_mut().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Bookmark children are not an array",
        )
    })?;
    let have: Vec<Value> = children
        .iter()
        .filter(|child| child.is_object())
        .map(|child| child["url"].clone())
        .collect();
    let mut next = children
        .iter()
        .filter_map(|child| child["id"].as_str()?.parse::<u64>().ok())
        .max()
        .unwrap_or(bar_id);
    let micros = ((now + 11_644_473_600.0) * 1_000_000.0).trunc();
    if !micros.is_finite() || micros < 0.0 || micros > u64::MAX as f64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Bookmark clock is outside the supported range",
        ));
    }
    let stamp = format!("{micros:.0}");
    for bookmark in wanted {
        if have.contains(&bookmark["url"]) {
            continue;
        }
        next = next
            .checked_add(1)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "Bookmark id exhausted"))?;
        let name = bookmark
            .get("name")
            .filter(|value| value.as_str().is_some_and(|name| !name.is_empty()))
            .unwrap_or(&bookmark["url"]);
        children.push(json!({"date_added":stamp,"id":next.to_string(),"type":"url","name":name,"url":bookmark["url"]}));
    }
    if let Some(object) = document.as_object_mut() {
        object.remove("checksum");
    }
    write_json(&path, &document)
}

/// Mutate only this admitted profile's resources, before any helper can open them.
pub fn prepare(session: &Session, profile: &Profile, now: f64) -> io::Result<()> {
    let root = session.preparation_path(&profile.id)?;
    for key in &profile.clear_on_start {
        for relative in clear_paths(key) {
            let path = contained(&root, relative)?;
            let metadata = match fs::symlink_metadata(&path) {
                Ok(metadata) => metadata,
                Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
                Err(error) => return Err(error),
            };
            let result = if metadata.is_dir() {
                fs::remove_dir_all(&path)
            } else {
                fs::remove_file(&path)
            };
            if let Err(error) = result {
                eprintln!("Browser carryover cleanup was incomplete: {}", error.kind());
            }
        }
    }
    seed(profile, &root, now)
}
