//! Manual owner-authenticated portable artifacts. This never publishes or edits a ref.
use super::{Result, fail};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};
pub fn sign(root: &Path, output: &Path, platform: &str) -> Result<PathBuf> {
    let environment = std::env::vars().collect::<BTreeMap<_, _>>();
    super::release_signing::owner_context(&environment)?;
    let source = std::env::var("GITHUB_SHA")?;
    let generation = std::env::var("GITHUB_RUN_ID")?.parse::<u64>()?;
    if !super::canonical_sha(&source) || generation == 0 {
        return fail("Manual product signing requires exact hosted identity");
    }
    let stage = output.join("Numera Browser");
    let signed = output.join("owner-signed/Numera Browser");
    super::regular(&stage, true)?;
    let signed_parent = signed.parent().ok_or("Signed stage parent missing")?;
    fs::create_dir(signed_parent)?;
    super::browser_benchmark::copy_tree(&stage, &signed)?;
    let private = std::env::var_os("RUNNER_TEMP")
        .map(PathBuf::from)
        .ok_or("Private signing scratch root missing")?;
    super::release_signing::native_sign(
        &signed,
        platform,
        &root.join("docs/code-signing-cert.pem"),
        &private,
        &environment,
    )?;
    crate::product::inventory(&signed.join("engine"))
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    // The root signed BUILD inventory contains engine/FILES. Keeping a second
    // exhaustive root FILES would create a cyclic inventory after BUILD signing.
    fs::remove_file(signed.join("FILES.json"))?;
    let mut sdks = serde_json::Map::new();
    for backend in ["playwright", "patchright", "camoufox", "cloak"] {
        let package = super::decode(&fs::read(
            signed
                .join("engine/drivers")
                .join(backend)
                .join("package/package.json"),
        )?)?;
        let version = package["version"]
            .as_str()
            .ok_or("Signed SDK version missing")?;
        sdks.insert(backend.into(), json!({"version":version}));
    }
    let components = json!({"sdk":sdks,"browsers":super::decode(&fs::read(signed.join("engine/browsers/manifest.json"))?)?});
    super::release_signing::installed_manifest(
        &signed,
        platform,
        &source,
        generation,
        env!("CARGO_PKG_VERSION"),
        components,
    )?;
    super::release_signing::run(
        root,
        &[
            "sign".into(),
            signed.join("BUILD.json").into_os_string(),
            signed.join("BUILD.sig").into_os_string(),
            root.join("docs/release-signing-key.pem").into_os_string(),
        ],
    )?;
    let name = format!("numera-owner-signed-{platform}-{source}.zip");
    let destination = output.join(name);
    crate::product::archive(&signed, &destination, "Numera Browser")
        .map_err(|e| std::io::Error::other(e.to_string()))?;
    let receipt = json!({"schema":1,"platform":platform,"source_sha":source,"generation":generation,"signing_scope":"owner","release_authorized":false,"published":false,"archive":{"name":destination.file_name().and_then(|v|v.to_str()),"sha256":super::digest(&destination)?,"size":fs::metadata(&destination)?.len()},"build_manifest_sha256":super::digest(&signed.join("BUILD.json"))?,"installers":"unsigned separate build artifacts; excluded from owner-signed archive"});
    let receipt_path = output.join(format!("owner-signed-{platform}.json"));
    fs::write(&receipt_path, serde_json::to_vec_pretty(&receipt)?)?;
    super::release_signing::run(
        root,
        &[
            "sign".into(),
            receipt_path.into_os_string(),
            output
                .join(format!("owner-signed-{platform}.sig"))
                .into_os_string(),
            root.join("docs/release-signing-key.pem").into_os_string(),
        ],
    )?;
    Ok(destination)
}
pub fn verify(root: &Path, directory: &Path, source: &str) -> Result<()> {
    let key = super::release_signing::public_key(&root.join("docs/release-signing-key.pem"))?;
    for platform in ["linux-x64", "windows-x64", "macos-arm64"] {
        let receipt_path = directory.join(format!("owner-signed-{platform}.json"));
        super::release_signing::verify_manifest(
            &receipt_path,
            &directory.join(format!("owner-signed-{platform}.sig")),
            &key,
        )?;
        let receipt: Value = super::decode(&fs::read(receipt_path)?)?;
        if receipt["source_sha"] != source
            || receipt["platform"] != platform
            || receipt["signing_scope"] != "owner"
            || receipt["published"] != false
        {
            return fail("Signed review receipt source/platform/scope differs");
        }
        let name = receipt["archive"]["name"]
            .as_str()
            .filter(|n| Path::new(n).components().count() == 1)
            .ok_or("Signed archive name unsafe")?;
        let archive = directory.join(name);
        if super::digest(&archive)? != receipt["archive"]["sha256"]
            || Some(fs::metadata(&archive)?.len()) != receipt["archive"]["size"].as_u64()
        {
            return fail("Signed archive receipt differs");
        }
        let mut zip = zip::ZipArchive::new(fs::File::open(archive)?)?;
        let mut bytes = Vec::new();
        use std::io::Read;
        zip.by_name("Numera Browser/BUILD.json")?
            .take(8 * 1024 * 1024 + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() > 8 * 1024 * 1024 || super::sha(&bytes) != receipt["build_manifest_sha256"] {
            return fail("Signed installed build manifest differs");
        }
        let mut sig = Vec::new();
        zip.by_name("Numera Browser/BUILD.sig")?
            .take(1025)
            .read_to_end(&mut sig)?;
        let signature = ed25519_dalek::Signature::from_slice(&sig)
            .map_err(|_| "Signed installed signature malformed")?;
        key.verify_strict(&bytes, &signature)
            .map_err(|_| "Signed installed manifest authenticity failed")?;
        let manifest = super::decode(&bytes)?;
        if manifest["source_sha"] != source || manifest["platform"] != platform {
            return fail("Signed installed manifest source differs");
        }
    }
    println!(
        "Verified all three owner-signed native archives for review; no publication performed"
    );
    Ok(())
}
