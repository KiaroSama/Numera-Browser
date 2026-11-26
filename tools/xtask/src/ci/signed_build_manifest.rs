//! Inventory and authenticate the immutable portable build contents.
use super::super::{Result, fail, sha};
use ed25519_dalek::{Signature, VerifyingKey};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Read},
    path::Path,
};

pub fn installed_manifest(
    stage: &Path,
    platform: &str,
    source: &str,
    generation: u64,
    version: &str,
    components: Value,
) -> Result<Value> {
    installed_manifest_with_mode(
        stage,
        platform,
        source,
        generation,
        version,
        components,
        |path| {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                Ok(fs::metadata(path)?.permissions().mode() & 0o111 != 0)
            }
            #[cfg(not(unix))]
            {
                Ok(path.extension().is_some_and(|ext| ext == "exe"))
            }
        },
    )
}
pub fn installed_manifest_with_mode(
    stage: &Path,
    platform: &str,
    source: &str,
    generation: u64,
    version: &str,
    components: Value,
    executable: impl Fn(&Path) -> io::Result<bool>,
) -> Result<Value> {
    if generation == 0 || !super::super::canonical_sha(source) {
        return fail("Installed build generation requires actual hosted run identity");
    }
    let mut rows = Vec::new();
    for (name, path) in super::super::release::files(stage)? {
        if ["BUILD.json", "BUILD.sig"].contains(&name.as_str()) {
            continue;
        }
        super::super::release::public_path(&name, true)?;
        let executable = executable(&path)?;
        rows.push(json!({"path":name,"size":fs::metadata(&path)?.len(),"sha256":super::super::digest(&path)?,"executable":executable}));
    }
    let value = json!({"schema":1,"owner":"KiaroSama","product":"Numera Browser","version":version,"source_sha":source,"generation":generation,"platform":platform,"minimum_protocol":1,"components":components,"files":rows});
    let bytes = serde_json::to_vec(&value)?;
    if bytes.len() > 8 * 1024 * 1024 {
        return fail("Installed immutable inventory exceeds signed manifest bound");
    }
    fs::write(stage.join("BUILD.json"), bytes)?;
    Ok(value)
}
pub fn validate_archive_build(
    archive: &Path,
    metadata: &Value,
    platform: &str,
    generation: u64,
    source: &str,
    key: &VerifyingKey,
) -> Result<()> {
    let mut contents = BTreeMap::new();
    let mut count = 0u64;
    let mut expanded = 0u64;
    let mut accept = |name: &str, size: u64, mut input: Box<dyn Read + '_>| -> Result<()> {
        count += 1;
        expanded = expanded
            .checked_add(size)
            .ok_or("Archive expanded size overflow")?;
        if ["Numera Browser/BUILD.json", "Numera Browser/BUILD.sig"].contains(&name) {
            let limit = if name.ends_with(".sig") {
                64
            } else {
                8 * 1024 * 1024
            };
            if size > limit || contents.contains_key(name) {
                return fail("Packaged signed inventory duplicated or oversized");
            }
            let mut bytes = Vec::new();
            input.by_ref().take(limit + 1).read_to_end(&mut bytes)?;
            if bytes.len() as u64 != size {
                return fail("Packaged inventory length differs");
            }
            contents.insert(name.to_owned(), bytes);
        }
        Ok(())
    };
    if archive.extension().is_some_and(|s| s == "zip") {
        let mut packed = zip::ZipArchive::new(fs::File::open(archive)?)?;
        for i in 0..packed.len() {
            let file = packed.by_index(i)?;
            let name = file.name()?.into_owned();
            let size = file.size();
            accept(&name, size, Box::new(file))?;
        }
    } else {
        let mut packed = tar::Archive::new(flate2::read::GzDecoder::new(fs::File::open(archive)?));
        for file in packed.entries()? {
            let file = file?;
            let name = file.path()?.to_string_lossy().into_owned();
            let size = if file.header().entry_type().is_file() {
                file.size()
            } else {
                0
            };
            if name.ends_with("BUILD.json") && !file.header().entry_type().is_file() {
                return fail("Packaged inventory linked");
            }
            accept(&name, size, Box::new(file))?;
        }
    }
    if count
        > metadata["max_members"]
            .as_u64()
            .ok_or("Archive member limit missing")?
        || expanded
            > metadata["expanded_size"]
                .as_u64()
                .ok_or("Archive expansion limit missing")?
    {
        return fail("Packaged archive exceeds signed expansion bounds");
    }
    let bytes = contents
        .get("Numera Browser/BUILD.json")
        .ok_or("Packaged signed inventory missing")?;
    let proof = contents
        .get("Numera Browser/BUILD.sig")
        .ok_or("Packaged signed inventory missing")?;
    if proof.len() != 64 || sha(bytes) != metadata["build_manifest_sha256"] {
        return fail("Packaged inventory hash/signature differs");
    }
    key.verify_strict(bytes, &Signature::from_slice(proof)?)
        .map_err(|_| "Packaged build inventory is not authenticated by owner")?;
    let value: Value = serde_json::from_slice(bytes)?;
    if value["generation"] != generation
        || value["source_sha"] != source
        || value["platform"] != platform
        || value["components"] != metadata["components"]
    {
        return fail("Packaged build generation, source or SDK compatibility differs");
    }
    Ok(())
}
