//! Read-only checks for a provisioned native installation and its local server.
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::{self, Read},
    path::{Component, Path},
    time::Duration,
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Inventory {
    schema: u32,
    signing: String,
    files: Vec<File>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    path: String,
    size: u64,
    sha256: String,
}

fn invalid() -> io::Error {
    io::Error::other("Native installation inventory does not match its files")
}

fn relative(name: &str) -> io::Result<&Path> {
    let path = Path::new(name);
    if name.is_empty()
        || name.contains(['\\', ':'])
        || name.chars().any(char::is_control)
        || name.split('/').any(|part| part.is_empty())
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(invalid());
    }
    Ok(path)
}

fn regular(path: &Path) -> io::Result<fs::Metadata> {
    let metadata = fs::symlink_metadata(path)?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        if metadata.file_attributes() & 0x400 != 0 {
            return Err(invalid());
        }
    }
    if metadata.file_type().is_symlink() {
        return Err(invalid());
    }
    Ok(metadata)
}

/// Check the complete packaged file inventory, including browser/SDK resources.
/// The unsigned inventory proves consistency, not publisher authentication.
pub fn verify(root: &Path) -> io::Result<()> {
    let path = root.join("FILES.json");
    if !regular(&path)?.is_file() {
        return Err(invalid());
    }
    let mut raw = Vec::new();
    fs::File::open(&path)?
        .take(16 * 1024 * 1024 + 1)
        .read_to_end(&mut raw)?;
    if raw.len() > 16 * 1024 * 1024 {
        return Err(invalid());
    }
    let inventory: Inventory = serde_json::from_slice(&raw).map_err(|_| invalid())?;
    if inventory.schema != 1
        || inventory.signing != "unsigned"
        || inventory.files.is_empty()
        || inventory.files.len() > 200_000
    {
        return Err(invalid());
    }
    let mut expected = BTreeSet::from(["FILES.json".to_owned()]);
    for record in inventory.files {
        let path = root.join(relative(&record.path)?);
        if !expected.insert(record.path)
            || !regular(&path)?.is_file()
            || regular(&path)?.len() != record.size
            || record.sha256.len() != 64
            || !record
                .sha256
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase())
        {
            return Err(invalid());
        }
        let mut file = fs::File::open(&path)?;
        let mut hash = Sha256::new();
        let mut buffer = [0; 65_536];
        loop {
            let count = file.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            hash.update(&buffer[..count]);
        }
        let digest: String = hash
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        if digest != record.sha256 {
            return Err(invalid());
        }
    }
    fn walk(root: &Path, at: &Path, depth: usize, found: &mut BTreeSet<String>) -> io::Result<()> {
        if depth > 128 || !regular(at)?.is_dir() {
            return Err(invalid());
        }
        for entry in fs::read_dir(at)? {
            let path = entry?.path();
            let metadata = regular(&path)?;
            if metadata.is_dir() {
                walk(root, &path, depth + 1, found)?;
            } else if metadata.is_file() {
                let name = path.strip_prefix(root).map_err(|_| invalid())?;
                let name = name.to_str().ok_or_else(invalid)?.replace('\\', "/");
                if found.len() >= 200_001 || !found.insert(name) {
                    return Err(invalid());
                }
            } else {
                return Err(invalid());
            }
        }
        Ok(())
    }
    let mut actual = BTreeSet::new();
    walk(root, root, 0, &mut actual)?;
    if actual != expected {
        return Err(invalid());
    }
    Ok(())
}

/// Probe only the local API, with a bounded response and no proxy/redirect path.
pub async fn health(url: &str, token: &str) -> io::Result<()> {
    let url = reqwest::Url::parse(url).map_err(|_| invalid())?;
    if url.scheme() != "http"
        || !matches!(url.host_str(), Some("127.0.0.1" | "[::1]"))
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/api/health"
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(invalid());
    }
    let client = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(4))
        .build()
        .map_err(|_| io::Error::other("Local health probe failed"))?;
    let mut request = client.get(url);
    if !token.trim().is_empty() {
        request = request.bearer_auth(token.trim());
    }
    let mut response = request
        .send()
        .await
        .map_err(|_| io::Error::other("Local health probe failed"))?;
    if !response.status().is_success() {
        return Err(io::Error::other("Local server is not healthy"));
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| invalid())? {
        if bytes.len() + chunk.len() > 8192 {
            return Err(invalid());
        }
        bytes.extend_from_slice(&chunk);
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    if value["ok"] != true || value["version"] != env!("CARGO_PKG_VERSION") {
        return Err(io::Error::other("Local server identity or health differs"));
    }
    Ok(())
}
