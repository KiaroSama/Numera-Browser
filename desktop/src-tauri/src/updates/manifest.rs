use ed25519_dalek::{Signature, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
    path::{Component, Path},
};

pub const REPOSITORY: &str = "KiaroSama/Numera-Browser";
pub const PRODUCT: &str = "Numera Browser";
#[cfg(not(feature = "update-acceptance"))]
pub const PUBLIC_KEY: &str = "d4ab0a9e265f108225bc28089cbcba0823beaad39a3639574c1a56e416dd2fad";
#[cfg(feature = "update-acceptance")]
pub const PUBLIC_KEY: &str = env!("NUMERA_ACCEPTANCE_KEY");
pub const METADATA_LIMIT: u64 = 1_048_576;
pub const BUILD_LIMIT: u64 = 8_388_608;
pub const PROTOCOL: u32 = 1;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Asset {
    pub platform: String,
    pub name: String,
    pub size: u64,
    pub sha256: String,
    pub expanded_size: u64,
    pub max_members: u64,
    pub archive_root: String,
    pub build_manifest_sha256: String,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema: u32,
    pub repository: String,
    pub product: String,
    pub version: String,
    pub source_sha: String,
    pub generation: u64,
    pub created_at: u64,
    pub expires_at: u64,
    pub minimum_protocol: u32,
    pub assets: Vec<Asset>,
    pub components: BTreeMap<String, Value>,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(untagged)]
pub enum InstalledFile {
    Regular(RegularFile),
    Link(LinkFile),
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RegularFile {
    pub path: String,
    pub size: u64,
    pub sha256: String,
    pub executable: bool,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct LinkFile {
    pub path: String,
    pub link: String,
}
impl InstalledFile {
    pub fn path(&self) -> &str {
        match self {
            Self::Regular(file) => &file.path,
            Self::Link(file) => &file.path,
        }
    }
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Build {
    pub schema: u32,
    pub owner: String,
    pub product: String,
    pub version: String,
    pub source_sha: String,
    pub generation: u64,
    pub platform: String,
    pub minimum_protocol: u32,
    pub components: Value,
    pub files: Vec<InstalledFile>,
}
pub fn error(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
pub fn digest(bytes: &[u8]) -> String {
    hex(&Sha256::digest(bytes))
}
pub fn decode_hex<const N: usize>(text: &str) -> io::Result<[u8; N]> {
    if text.len() != N * 2
        || !text
            .bytes()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    {
        return Err(error("Invalid update identity encoding"));
    }
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let digit = |byte: u8| {
                if byte <= b'9' {
                    byte - b'0'
                } else {
                    byte - b'a' + 10
                }
            };
            (digit(pair[0]) << 4) | digit(pair[1])
        })
        .collect::<Vec<_>>()
        .try_into()
        .map_err(|_| error("Invalid hexadecimal identity length"))
}
pub fn verify(bytes: &[u8], signature: &[u8], key: &[u8; 32]) -> io::Result<()> {
    let key =
        VerifyingKey::from_bytes(key).map_err(|_| error("Update verification key is invalid"))?;
    let signature =
        Signature::from_slice(signature).map_err(|_| error("Update signature is not 64 bytes"))?;
    key.verify_strict(bytes, &signature)
        .map_err(|_| error("Update signature is not trusted"))
}
pub fn signed_manifest(
    bytes: &[u8],
    signature: &[u8],
    key: &[u8; 32],
    platform: &str,
    generation: u64,
    now: u64,
) -> io::Result<Manifest> {
    if bytes.len() as u64 > METADATA_LIMIT {
        return Err(error("Update metadata exceeds its limit"));
    }
    verify(bytes, signature, key)?;
    // Typed struct decoding rejects duplicate/unknown protocol fields.
    let value: Manifest =
        serde_json::from_slice(bytes).map_err(|_| error("Update metadata is malformed"))?;
    if value.schema != 1
        || value.repository != REPOSITORY
        || value.product != PRODUCT
        || value.minimum_protocol > PROTOCOL
        || value.generation == 0
        || value.generation <= generation
        || value.created_at > now.saturating_add(300)
        || value.expires_at <= now
        || value.expires_at <= value.created_at
        || value.version.is_empty()
        || value.version.len() > 64
    {
        return Err(error("Update generation is stale or incompatible"));
    }
    decode_hex::<20>(&value.source_sha)?;
    let mut platforms = BTreeSet::new();
    for asset in &value.assets {
        if !platforms.insert(&asset.platform)
            || !["windows-x64", "linux-x64", "macos-arm64"].contains(&asset.platform.as_str())
            || asset.name.is_empty()
            || !asset.name.starts_with("numera-")
            || !asset
                .name
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"-._".contains(&c))
            || !(asset.name.ends_with(".zip") || asset.name.ends_with(".tar.gz"))
            || asset.archive_root != PRODUCT
            || asset.size == 0
            || asset.size > 34_359_738_368
            || asset.expanded_size == 0
            || asset.expanded_size > 68_719_476_736
            || asset.max_members == 0
            || asset.max_members > 200_000
            || !value.components.contains_key(&asset.platform)
        {
            return Err(error("Update archive identity or bounds are invalid"));
        }
        decode_hex::<32>(&asset.sha256)?;
        decode_hex::<32>(&asset.build_manifest_sha256)?;
    }
    if !platforms.contains(&platform.to_owned()) {
        return Err(error("No compatible update asset"));
    }
    Ok(value)
}
pub fn signed_build(
    bytes: &[u8],
    signature: &[u8],
    key: &[u8; 32],
    platform: &str,
) -> io::Result<Build> {
    if bytes.len() as u64 > BUILD_LIMIT {
        return Err(error("Installed file inventory exceeds its limit"));
    }
    verify(bytes, signature, key)?;
    let build: Build = serde_json::from_slice(bytes)
        .map_err(|_| error("Installed generation inventory is malformed"))?;
    if build.schema != 1
        || build.owner != "KiaroSama"
        || build.product != PRODUCT
        || build.platform != platform
        || build.minimum_protocol > PROTOCOL
        || build.generation == 0
        || build.files.is_empty()
        || build.files.len() > 200_000
    {
        return Err(error("Installed generation is incompatible"));
    }
    decode_hex::<20>(&build.source_sha)?;
    let mut paths = BTreeSet::new();
    for file in &build.files {
        safe_path(file.path())?;
        let name = file.path().to_ascii_lowercase();
        if !paths.insert(name.clone()) {
            return Err(error("Duplicate installed file path"));
        }
        let descendant = format!("{name}/");
        if paths
            .range(descendant.clone()..)
            .next()
            .is_some_and(|other| other.starts_with(&descendant))
            || name
                .match_indices('/')
                .any(|(index, _)| paths.contains(&name[..index]))
        {
            return Err(error("Installed file inventory has a file-parent conflict"));
        }
        match file {
            InstalledFile::Regular(file) => {
                decode_hex::<32>(&file.sha256)?;
            }
            InstalledFile::Link(file) => {
                safe_link(&file.path, &file.link)?;
            }
        }
    }
    for name in ["BUILD.json", "BUILD.sig"] {
        if paths.contains(&name.to_ascii_lowercase()) {
            return Err(error("Inventory includes its own signature"));
        }
    }
    if !paths.contains(if platform == "windows-x64" {
        "numera.exe"
    } else {
        "numera"
    }) {
        return Err(error("Installed shell missing from inventory"));
    }
    Ok(build)
}
pub fn safe_path(text: &str) -> io::Result<()> {
    let path = Path::new(text);
    if text.is_empty()
        || text.len() > 4096
        || text.contains(['\\', ':', '\0'])
        || text.bytes().any(|c| c < 32)
        || path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(error("Unsafe update file path"));
    }
    for part in text.split('/') {
        let basename = part.split('.').next().unwrap_or("").to_ascii_lowercase();
        if part.is_empty()
            || part.ends_with(['.', ' '])
            || matches!(
                basename.as_str(),
                "con"
                    | "prn"
                    | "aux"
                    | "nul"
                    | "com1"
                    | "com2"
                    | "com3"
                    | "com4"
                    | "com5"
                    | "com6"
                    | "com7"
                    | "com8"
                    | "com9"
                    | "lpt1"
                    | "lpt2"
                    | "lpt3"
                    | "lpt4"
                    | "lpt5"
                    | "lpt6"
                    | "lpt7"
                    | "lpt8"
                    | "lpt9"
            )
        {
            return Err(error("Nonportable update file path"));
        }
    }
    let first = text.split('/').next().unwrap_or("").to_ascii_lowercase();
    if [
        "data",
        "logs",
        "user-data",
        "profiles",
        "dashboard",
        "vault",
        ".updates",
        ".git",
        ".ai",
        ".claude",
        ".specify",
        "specs",
        "plans",
        "secrets.md",
    ]
    .contains(&first.as_str())
        || first == ".numera-update.lock"
        || first.starts_with(".numera-update-")
        || text.split('/').any(|part| {
            part == ".ignoreme"
                || part.starts_with(".env")
                || part.eq_ignore_ascii_case("secrets.md")
        })
    {
        return Err(error("Update archive includes mutable or private files"));
    }
    Ok(())
}
pub fn safe_link(path: &str, link: &str) -> io::Result<()> {
    if link.is_empty() || link.contains(['\\', ':', '\0']) || link.starts_with('/') {
        return Err(error("Unsafe update symbolic link"));
    }
    let mut depth = path.split('/').count() as i64 - 1;
    for part in link.split('/') {
        match part {
            "" | "." => {}
            ".." => depth -= 1,
            _ => depth += 1,
        };
        if depth < 0 {
            return Err(error("Escaping update symbolic link"));
        }
    }
    Ok(())
}
pub fn platform() -> io::Result<&'static str> {
    match (std::env::consts::OS, std::env::consts::ARCH) {
        ("windows", "x86_64") => Ok("windows-x64"),
        ("linux", "x86_64") => Ok("linux-x64"),
        ("macos", "aarch64") => Ok("macos-arm64"),
        _ => Err(error("This platform has no signed portable update")),
    }
}
