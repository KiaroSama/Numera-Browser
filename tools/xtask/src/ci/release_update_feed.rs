//! Build an authenticated versioned update feed over the same portable archives.
use super::super::{Result, canonical_sha, decode, fail, release, release_signing as signing};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

pub(in crate::ci) fn run(root: &Path, args: &[std::ffi::OsString]) -> Result<()> {
    let sha = std::env::var("GITHUB_SHA")?;
    let version = env!("CARGO_PKG_VERSION");
    let generation = std::env::var("GITHUB_RUN_ID")?.parse::<u64>()?;
    if std::env::var("GITHUB_ACTIONS").as_deref() != Ok("true")
        || std::env::var("GITHUB_REPOSITORY").as_deref() != Ok("KiaroSama/Numera-Browser")
        || std::env::var("GITHUB_REF").as_deref() != Ok(format!("refs/tags/v{version}").as_str())
        || !canonical_sha(&sha)
        || generation == 0
    {
        return fail("Versioned update signing requires the exact hosted release tag");
    }
    let public = signing::public_key(&root.join("docs/release-signing-key.pem"))?;
    let mut raw = std::env::var("NUMERA_RELEASE_SIGNING_KEY")?.into_bytes();
    let key = signing::private_key(&raw);
    raw.fill(0);
    let key = key?;
    match args {
        [operation, stage, platform] if operation == "stage" => {
            let stage = Path::new(stage);
            let platform = platform
                .to_str()
                .filter(|target| release::NATIVE_PLATFORMS.contains(target))
                .ok_or("Invalid portable update target")?;
            let mut sdks = serde_json::Map::new();
            for backend in ["playwright", "patchright", "camoufox", "cloak"] {
                let package = stage.join(format!("engine/drivers/{backend}/package/package.json"));
                if package.is_file() {
                    let value: Value = decode(&fs::read(package)?)?;
                    let version = value["version"]
                        .as_str()
                        .ok_or("Bundled SDK version missing")?;
                    sdks.insert(backend.into(), json!({"version":version}));
                }
            }
            let browser_manifest = stage.join("engine/browsers/manifest.json");
            let browsers = if browser_manifest.is_file() {
                decode(&fs::read(&browser_manifest)?)?
            } else {
                json!({})
            };
            let unsigned = stage.join("BUILD.json");
            let bundled_browsers = if unsigned.exists() {
                let value: Value = decode(&fs::read(&unsigned)?)?;
                value["bundled_browsers"]
                    .as_bool()
                    .ok_or("Unsigned stage omits bundled browser metadata")?
            } else {
                browser_manifest.is_file()
            };
            if bundled_browsers != browser_manifest.is_file() {
                return fail("Portable browser receipt differs from the unsigned stage");
            }
            let mut build = signing::installed_manifest(
                stage,
                platform,
                &sha,
                generation,
                version,
                json!({"sdk":sdks,"browsers":browsers,"bundled_browsers":bundled_browsers}),
            )?;
            build["bundled_browsers"] = json!(bundled_browsers);
            fs::write(&unsigned, serde_json::to_vec(&build)?)?;
            signing::sign_manifest(
                &stage.join("BUILD.json"),
                &stage.join("BUILD.sig"),
                &key,
                &public,
            )
        }
        [operation, directory] if operation == "publish" => {
            let directory = Path::new(directory);
            let mut assets = Vec::new();
            let mut components = BTreeMap::new();
            for platform in release::NATIVE_PLATFORMS {
                let name = format!("numera-browser-{version}-{platform}-portable.zip");
                let path = directory.join(&name);
                let (mut metadata, component) = portable_metadata(&path)?;
                metadata["source_sha"] = json!(sha);
                metadata["platform"] = json!(platform);
                metadata["signing_scope"] = json!("owner");
                signing::validate_archive_build(
                    &path, &metadata, platform, generation, &sha, &public,
                )?;
                let mut asset = release::asset(&path)?;
                asset["platform"] = json!(platform);
                asset["archive_root"] = json!("Numera Browser");
                for field in ["expanded_size", "max_members", "build_manifest_sha256"] {
                    asset[field] = metadata[field].clone();
                }
                assets.push(asset);
                components.insert(platform, component);
            }
            let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs();
            let manifest = directory.join("update-manifest.json");
            let signature = directory.join("update-manifest.sig");
            let value = json!({"schema":1,"repository":"KiaroSama/Numera-Browser","product":"Numera Browser","version":version,"source_sha":sha,"generation":generation,"created_at":now,"expires_at":now+90*24*60*60,"minimum_protocol":1,"assets":assets,"components":components});
            fs::write(&manifest, serde_json::to_vec(&value)?)?;
            signing::sign_manifest(&manifest, &signature, &key, &public)?;
            let mut changed = fs::read(&manifest)?;
            changed.push(b' ');
            let tampered =
                PathBuf::from(std::env::var_os("RUNNER_TEMP").ok_or("Runner temp is missing")?)
                    .join("numera-tampered-update-manifest.json");
            fs::write(&tampered, changed)?;
            let rejected = signing::verify_manifest(&tampered, &signature, &public).is_err();
            fs::remove_file(tampered)?;
            if !rejected {
                return fail("Modified update manifest passed verification");
            }
            Ok(())
        }
        _ => fail(
            "Expected release-update-feed stage <portable-dir> <platform> or publish <artifact-dir>",
        ),
    }
}

fn portable_metadata(path: &Path) -> Result<(Value, Value)> {
    use std::io::Read;
    let mut zip = zip::ZipArchive::new(fs::File::open(path)?)?;
    if zip.len() > 200_000 {
        return fail("Portable update archive has too many files");
    }
    let mut expanded = 0u64;
    for index in 0..zip.len() {
        expanded = expanded
            .checked_add(zip.by_index(index)?.size())
            .ok_or("Portable archive size overflow")?;
    }
    if expanded > 68_719_476_736 {
        return fail("Portable update archive is too large");
    }
    let mut bytes = Vec::new();
    zip.by_name("Numera Browser/BUILD.json")?
        .take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > 8 * 1024 * 1024 {
        return fail("Portable update inventory is too large");
    }
    let build: Value = decode(&bytes)?;
    let components = build
        .get("components")
        .ok_or("Portable update component inventory missing")?
        .clone();
    let metadata = json!({"asset":release::asset(path)?,"expanded_size":expanded,"max_members":zip.len(),"build_manifest_sha256":super::super::sha(&bytes),"components":components});
    Ok((metadata, components))
}
