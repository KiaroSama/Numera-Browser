//! Exact public identity and file hashing shared by native packaging and CI.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::PathBuf;

pub fn compiler_artifact(messages: &[u8], name: &str) -> Result<PathBuf, Box<dyn Error>> {
    let mut artifacts = Vec::new();
    let mut finished = None;
    for line in messages
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.is_empty())
    {
        let value: Value = serde_json::from_slice(line)?;
        match value["reason"].as_str() {
            Some("compiler-artifact")
                if value["target"]["name"] == name
                    && value["target"]["kind"]
                        .as_array()
                        .is_some_and(|kinds| kinds.iter().any(|k| k == "bin")) =>
            {
                if let Some(path) = value["executable"].as_str() {
                    artifacts.push(PathBuf::from(path));
                }
            }
            Some("build-finished") => {
                if finished.is_some() {
                    return Err("duplicate Cargo completion record".into());
                }
                finished = value["success"].as_bool();
            }
            _ => {}
        }
    }
    if finished != Some(true) || artifacts.len() != 1 || !artifacts[0].is_file() {
        return Err(
            format!("Cargo did not produce exactly one successful {name} executable").into(),
        );
    }
    Ok(artifacts.remove(0))
}
use sha2::{Digest, Sha256};
use std::{env, error::Error, fs::File, io::Read, path::Path};

#[derive(Debug, Serialize, Deserialize)]
pub struct BuildIdentity {
    pub source_sha: String,
    pub generation: u64,
    pub platform: String,
    pub architecture: String,
}

impl BuildIdentity {
    pub fn from_environment() -> Result<Self, Box<dyn Error>> {
        Self::parse(
            &env::var("GITHUB_SHA")?,
            &env::var("GITHUB_RUN_ID")?,
            env::consts::OS,
            env::consts::ARCH,
        )
    }

    pub fn parse(
        source: &str,
        generation: &str,
        os: &str,
        arch: &str,
    ) -> Result<Self, Box<dyn Error>> {
        if source.len() != 40
            || !source
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            || generation.is_empty()
            || generation.len() > 16
            || generation.starts_with('0')
            || !generation.bytes().all(|b| b.is_ascii_digit())
            || !["linux", "windows", "macos"].contains(&os)
            || !["x86_64", "aarch64"].contains(&arch)
        {
            return Err("invalid candidate build identity".into());
        }
        Ok(Self {
            source_sha: source.into(),
            generation: generation.parse()?,
            platform: os.into(),
            architecture: arch.into(),
        })
    }

    pub fn verify(&self, info: &Value) -> Result<(), Box<dyn Error>> {
        if info["schema"] != 1
            || info["owner"] != "KiaroSama"
            || info["product"] != "Numera Browser"
            || info["version"] != env!("CARGO_PKG_VERSION")
            || info["source_sha"] != self.source_sha
            || info["generation"] != self.generation
            || info["application_runtime"] != "native-candidate"
            || info["acceptance"] != true
        {
            return Err("compiled candidate identity differs from the packaging job".into());
        }
        Ok(())
    }
}

pub fn hash(path: &Path) -> Result<String, Box<dyn Error>> {
    let mut source = File::open(path)?;
    let mut digest = Sha256::new();
    let mut bytes = [0u8; 65536];
    loop {
        let read = source.read(&mut bytes)?;
        if read == 0 {
            break;
        }
        digest.update(&bytes[..read]);
        crate::progress::completed("hashed", read);
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}
