//! Offline validation of explicitly provisioned, distributable vendor resources.
//! A receipt is an operator assertion, never an upstream signature or source offer
//! acquisition step. Every copied byte must be listed in the receipt.
use crate::artifact_identity::{BuildIdentity, hash};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeSet,
    env,
    error::Error,
    fs,
    path::{Path, PathBuf},
};

pub const RECEIPT: &str = "RESOURCE-PROVENANCE.json";

fn linked(metadata: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        metadata.file_attributes() & 0x400 != 0
    }
    #[cfg(not(windows))]
    {
        metadata.file_type().is_symlink()
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct FileRecord {
    pub path: String,
    pub size: u64,
    pub sha256: String,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SourceOffer {
    pub component: String,
    pub version: String,
    pub license: String,
    pub source_url: String,
    /// GPL/MPL patches and add-on sources are supplied by the operator, offline.
    pub archive: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Receipt {
    pub schema: u32,
    pub platform: String,
    pub architecture: String,
    pub files: Vec<FileRecord>,
    pub source_offers: Vec<SourceOffer>,
}

/// Explicitly inventory already acquired resources. This never acquires vendor
/// code and does not certify the operator's licensing/authenticity assertions.
pub fn prepare(root: &Path, offers: &Path) -> Result<(), Box<dyn Error>> {
    if root.join(RECEIPT).exists() {
        return Err("resource receipt already exists; refusing to overwrite its provenance".into());
    }
    let source_offers: Vec<SourceOffer> = serde_json::from_slice(&fs::read(offers)?)?;
    let mut inventory = Vec::new();
    for relative in files(root)? {
        let path = portable_name(&relative)?;
        public_path(&path, true)?;
        inventory.push(FileRecord {
            path,
            size: fs::metadata(root.join(&relative))?.len(),
            sha256: hash(&root.join(relative))?,
        });
    }
    let receipt = Receipt {
        schema: 1,
        platform: env::consts::OS.into(),
        architecture: env::consts::ARCH.into(),
        files: inventory,
        source_offers,
    };
    let identity = BuildIdentity::parse(&"0".repeat(40), "1", env::consts::OS, env::consts::ARCH)?;
    receipt.validate_layout(root, &identity)?;
    let mut output = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(root.join(RECEIPT))?;
    serde_json::to_writer_pretty(&mut output, &receipt)?;
    output.sync_all()?;
    drop(output);
    if let Err(error) = Receipt::validate(root, &identity) {
        fs::remove_file(root.join(RECEIPT))?;
        return Err(error);
    }
    println!(
        "Inventoried provisioned resources at {} (operator source/license assertions, unsigned)",
        root.display()
    );
    Ok(())
}

/// Portable publication names, including on a host with different path syntax.
pub fn public_path(name: &str, vendor: bool) -> Result<(), Box<dyn Error>> {
    let forbidden = [
        ".ai",
        ".specify",
        "specs",
        ".git",
        ".claude",
        ".agents",
        ".kiro",
        ".codex",
        ".cursor",
        ".cline",
        ".codebase-memory",
        "graphify-out",
        ".ignoreme",
        "__pycache__",
        ".venv",
        "venv",
    ];
    let parts: Vec<_> = name.split('/').collect();
    if name.is_empty()
        || name.contains(['\\', ':'])
        || name.chars().any(char::is_control)
        || parts.iter().any(|part| {
            part.is_empty()
                || *part == "."
                || *part == ".."
                || forbidden.contains(part)
                || (!vendor && *part == "node_modules")
                || (part.starts_with(".env")
                    && ![".env.example", ".env.sample", ".env.template", ".env.dist"]
                        .contains(part))
        })
        || ["data", "plans", ".ci-work", ".ci-cache", ".ci-runner-win"].contains(&parts[0])
        || ["secrets.md", "explain-ai.md", "reference.md"].contains(
            &parts
                .last()
                .copied()
                .unwrap_or("")
                .to_ascii_lowercase()
                .as_str(),
        )
        || ["CLAUDE.md", "AGENTS.md"].contains(&name)
    {
        return Err(format!("refusing non-public publication path: {name}").into());
    }
    Ok(())
}

pub fn files(root: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    fn walk(
        root: &Path,
        directory: &Path,
        depth: usize,
        result: &mut Vec<PathBuf>,
    ) -> Result<(), Box<dyn Error>> {
        if depth > 128 {
            return Err("resource nesting exceeds its bound".into());
        }
        if linked(&fs::symlink_metadata(directory)?) {
            return Err("resource directory must not be a symbolic link".into());
        }
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let path = entry.path();
            let metadata = fs::symlink_metadata(&path)?;
            if linked(&metadata) {
                return Err(
                    format!("unchecked symbolic link in resources: {}", path.display()).into(),
                );
            }
            if metadata.is_dir() {
                walk(root, &path, depth + 1, result)?;
            } else if metadata.is_file() {
                result.push(path.strip_prefix(root)?.to_owned());
            } else {
                return Err("resources contain a non-file entry".into());
            }
        }
        Ok(())
    }
    let mut result = Vec::new();
    walk(root, root, 0, &mut result)?;
    result.sort();
    Ok(result)
}

pub fn portable_name(path: &Path) -> Result<String, Box<dyn Error>> {
    path.components()
        .map(|part| {
            part.as_os_str()
                .to_str()
                .ok_or("resource name is not Unicode")
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|parts| parts.join("/"))
        .map_err(Into::into)
}

impl Receipt {
    pub fn validate(root: &Path, identity: &BuildIdentity) -> Result<Self, Box<dyn Error>> {
        let receipt_path = root.join(RECEIPT);
        if linked(&fs::symlink_metadata(&receipt_path)?)
            || fs::metadata(&receipt_path)?.len() > 16 * 1024 * 1024
        {
            return Err("resource receipt is linked or oversized".into());
        }
        let receipt: Self = serde_json::from_slice(&fs::read(receipt_path)?)?;
        if receipt.schema != 1
            || receipt.platform != identity.platform
            || receipt.architecture != identity.architecture
        {
            return Err(
                "resource receipt platform/architecture differs from this native host".into(),
            );
        }
        let actual = files(root)?
            .iter()
            .map(|path| portable_name(path))
            .collect::<Result<BTreeSet<_>, _>>()?;
        let mut expected = BTreeSet::from([RECEIPT.to_owned()]);
        for record in &receipt.files {
            public_path(&record.path, true)?;
            if !["drivers", "browsers", "sources", "notices"]
                .contains(&record.path.split('/').next().unwrap_or(""))
                || !expected.insert(record.path.clone())
                || record.sha256.len() != 64
                || !record
                    .sha256
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err("invalid, duplicate or reserved resource receipt entry".into());
            }
            let path = root.join(&record.path);
            if linked(&fs::symlink_metadata(&path)?)
                || fs::metadata(&path)?.len() != record.size
                || hash(&path)? != record.sha256
            {
                return Err(format!(
                    "resource differs from its provisioned receipt: {}",
                    record.path
                )
                .into());
            }
        }
        if actual != expected {
            return Err("resource receipt must inventory every file exactly once".into());
        }
        receipt.validate_layout(root, identity)?;
        Ok(receipt)
    }

    fn validate_layout(&self, root: &Path, identity: &BuildIdentity) -> Result<(), Box<dyn Error>> {
        let node = if identity.platform == "windows" {
            "node.exe"
        } else {
            "node"
        };
        for backend in ["playwright", "patchright", "camoufox", "cloak"] {
            for relative in [
                format!("drivers/{backend}/{node}"),
                format!("drivers/{backend}/package/cli.js"),
                format!("drivers/{backend}/package/package.json"),
            ] {
                if !root.join(&relative).is_file() {
                    return Err(format!("required vendor SDK resource missing: {relative}").into());
                }
            }
            let manifest: Value = serde_json::from_slice(&fs::read(
                root.join(format!("drivers/{backend}/package/package.json")),
            )?)?;
            let version = manifest["version"].as_str().unwrap_or("");
            let parts: Vec<_> = version.split('.').collect();
            if parts.len() != 3
                || parts[0] != "1"
                || parts[2].parse::<u32>().is_err()
                || !match backend {
                    "camoufox" => parts[1] == "62",
                    "playwright" => ["64", "65"].contains(&parts[1]),
                    "cloak" => ["64", "65"].contains(&parts[1]),
                    _ => parts[1] == "64",
                }
            {
                return Err(format!("unsupported {backend} vendor SDK protocol version").into());
            }
        }
        let manifest: Value =
            serde_json::from_slice(&fs::read(root.join("browsers/manifest.json"))?)?;
        if manifest.get("cloak").is_some() || root.join("browsers/cloak").exists() {
            return Err("Cloak browser binary redistribution is not authorized; provision it after installation".into());
        }
        for backend in ["playwright", "patchright", "camoufox"] {
            let relative = manifest[backend]["executable"]
                .as_str()
                .ok_or("browser manifest executable missing")?;
            public_path(relative, true)?;
            if !root.join("browsers").join(relative).is_file()
                || manifest[backend]["sha256"].as_str()
                    != Some(hash(&root.join("browsers").join(relative))?.as_str())
            {
                return Err("bundled browser executable differs from its manifest".into());
            }
        }
        let mut offers = BTreeSet::new();
        for offer in &self.source_offers {
            if !offers.insert(offer.component.as_str())
                || offer.version.trim().is_empty()
                || offer.license.trim().is_empty()
                || !offer.source_url.starts_with("https://")
                || offer.source_url.chars().any(char::is_control)
            {
                return Err(
                    "resource corresponding-source offer is incomplete or duplicated".into(),
                );
            }
            if let Some(archive) = &offer.archive {
                public_path(archive, true)?;
                if !archive.starts_with("sources/third-party/")
                    || !self.files.iter().any(|file| file.path == *archive)
                {
                    return Err(
                        "source offer archive is not inventoried corresponding source".into(),
                    );
                }
            }
            if ["camoufox", "ublock-origin"].contains(&offer.component.as_str())
                && offer.archive.is_none()
            {
                return Err(
                    "Camoufox/addon corresponding source archive must be provisioned explicitly"
                        .into(),
                );
            }
        }
        if ![
            "node",
            "playwright",
            "patchright",
            "camoufox",
            "chromium",
            "firefox",
        ]
        .into_iter()
        .all(|name| offers.contains(name))
        {
            return Err("required vendor source/license offers are missing".into());
        }
        let addons = manifest["camoufox"]["addons"]
            .as_array()
            .ok_or("Camoufox default addon list missing")?;
        for addon in addons {
            let relative = addon.as_str().ok_or("Camoufox addon path is not text")?;
            public_path(relative, true)?;
            if !root
                .join("browsers")
                .join(relative)
                .join("manifest.json")
                .is_file()
                || !offers.contains("ublock-origin")
            {
                return Err("Camoufox addon or corresponding-source offer missing".into());
            }
        }
        if !self
            .files
            .iter()
            .any(|file| file.path.starts_with("notices/"))
        {
            return Err("vendor resources must retain license notices".into());
        }
        Ok(())
    }

    pub fn copy(&self, source: &Path, destination: &Path) -> Result<(), Box<dyn Error>> {
        for relative in
            std::iter::once(RECEIPT).chain(self.files.iter().map(|record| record.path.as_str()))
        {
            copy_file(&source.join(relative), &destination.join(relative))?;
        }
        let copied: Receipt = serde_json::from_slice(&fs::read(destination.join(RECEIPT))?)?;
        if serde_json::to_value(&copied)? != serde_json::to_value(self)? {
            return Err("provisioned resource receipt changed during packaging".into());
        }
        for record in &self.files {
            let path = destination.join(&record.path);
            if fs::metadata(&path)?.len() != record.size || hash(&path)? != record.sha256 {
                return Err("provisioned resources changed during packaging".into());
            }
        }
        Ok(())
    }
}

pub fn copy_file(source: &Path, target: &Path) -> Result<(), Box<dyn Error>> {
    if linked(&fs::symlink_metadata(source)?) || !source.is_file() || target.exists() {
        return Err("copy requires a regular source and an absent target".into());
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source, target)?;
    Ok(())
}

pub fn copy_tree(source: &Path, destination: &Path) -> Result<(), Box<dyn Error>> {
    for relative in files(source)? {
        public_path(&portable_name(&relative)?, true)?;
        copy_file(&source.join(&relative), &destination.join(&relative))?;
    }
    Ok(())
}
