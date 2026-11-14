//! Read-only admission of the provisioned Camoufox font bundle. Literal SDK
//! marker replacement must bind an active XML directory, never a comment/CDATA.
use quick_xml::{Reader, events::Event};
use serde_json::Value;
use std::{
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};
fn bad() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "Camoufox bundled fonts or font configuration are incomplete",
    )
}
fn read(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.is_empty() || bytes.len() as u64 > limit {
        return Err(bad());
    }
    Ok(bytes)
}
fn binding(source: &str) -> io::Result<()> {
    const MARKER: &str = "<dir prefix=\"cwd\">fonts</dir>";
    const PROBE: &str = "numera-bundled-font-directory-probe";
    if !source.contains(MARKER) {
        return Err(bad());
    }
    let transformed = source.replace(MARKER, &format!("<dir>{PROBE}</dir>"));
    let mut reader = Reader::from_str(&transformed);
    let mut stack = Vec::<(Vec<u8>, bool)>::new();
    let mut completed = false;
    let mut found = false;
    loop {
        match reader.read_event().map_err(|_| bad())? {
            Event::Start(element) => {
                if stack.len() >= 128 {
                    return Err(bad());
                }
                let name = element.name().as_ref().to_vec();
                let mut attributes = 0;
                for attribute in element.attributes() {
                    attribute.map_err(|_| bad())?;
                    attributes += 1;
                }
                if stack.is_empty() && (completed || name != b"fontconfig") {
                    return Err(bad());
                }
                stack.push((name, attributes == 0));
            }
            Event::Empty(element) => {
                for attribute in element.attributes() {
                    attribute.map_err(|_| bad())?;
                }
                if stack.is_empty() {
                    if completed || element.name().as_ref() != b"fontconfig" {
                        return Err(bad());
                    }
                    completed = true;
                }
            }
            Event::End(element) => {
                let (name, _) = stack.pop().ok_or_else(bad)?;
                if name != element.name().as_ref() {
                    return Err(bad());
                }
                if stack.is_empty() {
                    completed = true;
                }
            }
            Event::Text(text) => {
                let text = text.decode().map_err(|_| bad())?;
                if stack.len() == 2 && stack[1].0 == b"dir" && stack[1].1 && text == PROBE {
                    found = true;
                }
                if stack.is_empty() && !text.trim().is_empty() {
                    return Err(bad());
                }
            }
            Event::Eof => break,
            Event::CData(_)
            | Event::Comment(_)
            | Event::Decl(_)
            | Event::PI(_)
            | Event::DocType(_)
            | Event::GeneralRef(_) => (),
        }
    }
    if !completed || !stack.is_empty() || !found {
        return Err(bad());
    }
    Ok(())
}
pub fn require(bundle: &Path) -> io::Result<()> {
    let fonts = bundle.join("fonts");
    let groups = fonts.join("groups.json");
    let mapping = if groups.exists() {
        let groups: Value = serde_json::from_slice(&read(&groups, 32768)?).map_err(|_| bad())?;
        Some(groups["readBy"].as_object().ok_or_else(bad)?.clone())
    } else {
        None
    };
    for (key, name) in [("lin", "linux"), ("mac", "macos"), ("win", "windows")] {
        let config = [
            bundle.join("fontconfig").join(name).join("fonts.conf"),
            bundle.join("fontconfigs").join(name).join("fonts.conf"),
        ]
        .into_iter()
        .find(|path| path.is_file())
        .ok_or_else(bad)?;
        let bytes = read(&config, 1048576)?;
        binding(std::str::from_utf8(&bytes).map_err(|_| bad())?)?;
        let directories = if let Some(mapping) = &mapping {
            let groups = mapping
                .get(key)
                .and_then(Value::as_array)
                .filter(|groups| !groups.is_empty())
                .ok_or_else(bad)?;
            groups
                .iter()
                .map(|group| {
                    let group = group
                        .as_str()
                        .filter(|group| {
                            !group.is_empty()
                                && group.len() <= 16
                                && group.bytes().all(|byte| byte.is_ascii_uppercase())
                        })
                        .ok_or_else(bad)?;
                    Ok(fonts.join(group))
                })
                .collect::<io::Result<Vec<_>>>()?
        } else {
            vec![if fonts.join(name).is_dir() {
                fonts.join(name)
            } else {
                fonts.clone()
            }]
        };
        for directory in directories {
            if !directory.is_dir()
                || !fs::read_dir(directory)?.any(|entry| {
                    entry.is_ok_and(|entry| {
                        let path = entry.path();
                        path.extension()
                            .and_then(|extension| extension.to_str())
                            .is_some_and(|extension| {
                                ["ttf", "otf", "ttc"]
                                    .contains(&extension.to_ascii_lowercase().as_str())
                            })
                            && path
                                .metadata()
                                .is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0)
                    })
                })
            {
                return Err(bad());
            }
        }
    }
    Ok(())
}
pub fn prepare(binary: &Path) -> io::Result<PathBuf> {
    if !binary.is_file() {
        return Err(bad());
    }
    let root = binary.parent().ok_or_else(bad)?;
    require(root)?;
    binary.canonicalize()
}
