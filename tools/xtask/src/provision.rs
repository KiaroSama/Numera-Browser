//! Explicit vendor provisioning for native distributions. Application/Cargo
//! builds never call this command implicitly, and no Python code is installed.
use crate::ci::{self, Result};
use numera_core::{browser::adapter::restrict_environment, ownership::capture};
use serde_json::{Value, json};
use std::{
    collections::BTreeSet,
    fs,
    io::{self, Read, Write},
    path::{Component, Path, PathBuf},
    process::Command,
    time::Duration,
};

pub fn pins() -> Result<Value> {
    Ok(serde_json::from_str(include_str!(
        "../provision-pins.json"
    ))?)
}

fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value[key]
        .as_str()
        .ok_or_else(|| io::Error::other("Provision pin is missing").into())
}

fn diagnostic(bytes: &[u8]) -> String {
    const LIMIT: usize = 16 * 1024;
    // Redact before clipping so a cut through a credential cannot publish its
    // suffix without the key/prefix that identifies it to the redactor.
    let text =
        numera_core::ownership::resources::redact_diagnostics(&String::from_utf8_lossy(bytes));
    let mut start = text.len().saturating_sub(LIMIT);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    let tail = text[start..]
        .chars()
        .filter(|character| !character.is_control() || ['\n', '\r', '\t'].contains(character))
        .collect::<String>();
    if start > 0 {
        format!("[earlier output omitted]\n{tail}")
    } else {
        tail
    }
}

/// Run an explicitly selected vendor tool with retained ownership and bounded,
/// redacted failure output. No caller may infer success from cleanup alone.
pub fn execute(command: &mut Command, seconds: u64) -> Result<capture::Output> {
    restrict_environment(command);
    command.env_remove("GH_TOKEN").env_remove("GITHUB_TOKEN");
    let mut natural = None;
    let mut partial = [Vec::new(), Vec::new()];
    let result = capture::run_observed_streamed(
        command,
        capture::Limits {
            wall: Duration::from_secs(seconds),
            idle: Duration::from_secs(seconds),
            bytes: 2 * 1024 * 1024,
            cleanup: Duration::from_secs(15),
        },
        || false,
        |event, _| {
            if let capture::Observation::BeforeCleanup {
                natural_settlement, ..
            } = event
            {
                natural = natural_settlement;
            }
        },
        |index, bytes| {
            partial[index].extend_from_slice(bytes);
            crate::progress::completed(
                if index == 0 {
                    "vendor-stdout"
                } else {
                    "vendor-stderr"
                },
                bytes.len(),
            );
        },
    );
    let result = match result {
        Ok(output) => output,
        Err(error) => {
            eprintln!(
                "Vendor provisioning capture failed: {error}\nCaptured stdout:\n{}\nCaptured stderr:\n{}",
                diagnostic(&partial[0]),
                diagnostic(&partial[1])
            );
            return Err(error.into());
        }
    };
    if result.exit != 0 || natural != Some(true) {
        return ci::fail(format!(
            "Vendor provisioning command failed (exit={}, natural_settlement={natural:?})\nCaptured stdout:\n{}\nCaptured stderr:\n{}",
            result.exit,
            diagnostic(&result.stdout),
            diagnostic(&result.stderr)
        ));
    }
    Ok(result)
}

fn download(url: &str, target: &Path, expected: &str, limit: u64) -> Result<()> {
    if ![
        "https://files.pythonhosted.org/packages/",
        "https://github.com/daijro/",
        "https://codeload.github.com/daijro/camoufox/",
        "https://codeload.github.com/gorhill/uBlock/",
        "https://raw.githubusercontent.com/daijro/geoip-all-in-one/",
        "https://addons.mozilla.org/firefox/downloads/",
    ]
    .iter()
    .any(|prefix| url.starts_with(prefix))
        || url.chars().any(char::is_control)
        || expected.len() != 64
        || !expected
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        || target.exists()
    {
        return ci::fail("Invalid, unpinned or existing vendor download");
    }
    fs::create_dir_all(target.parent().ok_or("Download parent missing")?)?;
    fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)?;
    let result = execute(
        Command::new("curl")
            .args([
                "--fail",
                "--silent",
                "--show-error",
                "--location",
                "--proto",
                "=https",
                "--proto-redir",
                "=https",
                "--connect-timeout",
                "20",
                "--max-time",
                "600",
                "--max-filesize",
                &limit.to_string(),
                "--output",
            ])
            .arg(target)
            .arg(url),
        630,
    );
    if let Err(error) = result {
        fs::remove_file(target)?;
        return Err(error);
    }
    if fs::metadata(target)?.len() > limit || ci::digest(target)? != expected {
        fs::remove_file(target)?;
        return ci::fail("Vendor download failed its pinned digest or size check");
    }
    Ok(())
}

fn member(name: &str) -> Result<PathBuf> {
    let path = Path::new(name.trim_end_matches('/'));
    if name.is_empty()
        || name.contains(['\\', ':'])
        || name.chars().any(char::is_control)
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
        || name
            .trim_end_matches('/')
            .split('/')
            .any(|part| part.is_empty() || matches!(part, "." | ".."))
    {
        return ci::fail("Vendor archive contains an unsafe path");
    }
    Ok(path.to_owned())
}

/// Unpack regular bytes and materialize only contained vendor symlinks. The
/// resulting resource tree contains no links and can be inventoried portably.
pub fn extract(archive: &Path, destination: &Path, prefix: &str) -> Result<()> {
    fs::create_dir_all(
        destination
            .parent()
            .ok_or("Vendor destination parent missing")?,
    )?;
    fs::create_dir(destination)?;
    let mut archive = zip::ZipArchive::new(fs::File::open(archive)?)?;
    if archive.len() > 100_000 {
        return ci::fail("Vendor archive has too many entries");
    }
    let mut total = 0u64;
    let mut names = BTreeSet::new();
    let mut links = Vec::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;
        let entry_name = entry.name()?;
        let Some(name) = entry_name.strip_prefix(prefix) else {
            continue;
        };
        if name.is_empty() {
            continue;
        }
        let relative = member(name)?;
        let target = destination.join(&relative);
        if !names.insert(relative.clone()) {
            return ci::fail("Duplicate vendor archive member");
        }
        total = total
            .checked_add(entry.size())
            .ok_or("Vendor archive exceeds its bound")?;
        if total > 8 * 1024 * 1024 * 1024 || entry.size() > 2 * 1024 * 1024 * 1024 {
            return ci::fail("Expanded vendor archive exceeds its bound");
        }
        if entry.is_dir() {
            fs::create_dir_all(target)?;
            continue;
        }
        let mode = entry.unix_mode().unwrap_or(0o100644);
        if mode & 0o170000 == 0o120000 {
            let mut value = String::new();
            entry.take(4097).read_to_string(&mut value)?;
            if value.len() > 4096
                || value.contains(['\\', ':'])
                || value.chars().any(char::is_control)
            {
                return ci::fail("Vendor link target is invalid");
            }
            let mut resolved = relative
                .parent()
                .ok_or("Vendor link parent missing")?
                .to_owned();
            for component in Path::new(&value).components() {
                match component {
                    Component::Normal(value) => resolved.push(value),
                    Component::CurDir => {}
                    Component::ParentDir if resolved.pop() => {}
                    _ => return ci::fail("Vendor archive link escapes its root"),
                }
            }
            links.push((relative, resolved));
            continue;
        }
        if mode & 0o170000 != 0 && mode & 0o170000 != 0o100000 {
            return ci::fail("Vendor archive contains a special file");
        }
        fs::create_dir_all(target.parent().ok_or("Vendor member parent missing")?)?;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)?;
        let copied = crate::progress::copy(&mut entry, &mut output)?;
        if copied != entry.size() {
            return ci::fail("Vendor archive member is truncated");
        }
        output.flush()?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(&target, fs::Permissions::from_mode(mode & 0o777))?;
        }
    }
    let mut copied_bytes = total;
    let mut copied_entries = names.len();
    for _ in 0..16 {
        if links.is_empty() {
            return Ok(());
        }
        let mut pending = Vec::new();
        let waiting: BTreeSet<_> = links.iter().map(|(path, _)| path.clone()).collect();
        for (relative, resolved) in links {
            if relative.starts_with(&resolved) {
                return ci::fail("Vendor archive link points to itself or an ancestor");
            }
            let source = destination.join(&resolved);
            let depends_on_link = source.is_dir()
                && waiting
                    .range(resolved.clone()..)
                    .next()
                    .is_some_and(|path| path.starts_with(&resolved));
            if source.exists() && !depends_on_link {
                copy_tree(
                    &source,
                    &destination.join(relative),
                    0,
                    &mut copied_bytes,
                    &mut copied_entries,
                )?;
            } else {
                pending.push((relative, resolved));
            }
        }
        links = pending;
    }
    ci::fail("Vendor archive has unresolved or cyclic links")
}

fn copy_tree(
    source: &Path,
    target: &Path,
    depth: usize,
    bytes: &mut u64,
    entries: &mut usize,
) -> Result<()> {
    if depth > 128 || fs::symlink_metadata(source)?.file_type().is_symlink() {
        return ci::fail("Vendor resource has an unresolved link or excessive depth");
    }
    *entries += 1;
    if *entries > 100_000 {
        return ci::fail("Materialized vendor archive has too many entries");
    }
    if source.is_file() {
        *bytes = bytes
            .checked_add(fs::metadata(source)?.len())
            .ok_or("Materialized vendor archive exceeds its bound")?;
        if *bytes > 8 * 1024 * 1024 * 1024 {
            return ci::fail("Materialized vendor archive exceeds its bound");
        }
        fs::create_dir_all(target.parent().ok_or("Resource target parent missing")?)?;
        if target.exists() {
            return ci::fail("Vendor link materialization would overwrite a file");
        }
        fs::copy(source, target)?;
    } else if source.is_dir() {
        fs::create_dir_all(target)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy_tree(
                &entry.path(),
                &target.join(entry.file_name()),
                depth + 1,
                bytes,
                entries,
            )?;
        }
    } else {
        return ci::fail("Vendor resource is not a regular file or directory");
    }
    Ok(())
}

// Vendor installers create framework links on macOS. Copy the contained target
// bytes while those links still exist, then replace each link itself. The
// stack rejects a link back to an ancestor instead of recursively copying it.
fn materialize(root: &Path, scratch: &Path) -> Result<()> {
    fn linked(path: &Path) -> Result<bool> {
        let metadata = fs::symlink_metadata(path)?;
        #[cfg(windows)]
        {
            use std::os::windows::fs::MetadataExt;
            if metadata.file_attributes() & 0x400 != 0 {
                return Ok(true);
            }
        }
        Ok(metadata.file_type().is_symlink())
    }
    fn collect(at: &Path, found: &mut Vec<PathBuf>, depth: usize) -> Result<()> {
        if depth > 128 {
            return ci::fail("Installed browser nesting exceeds its bound");
        }
        for entry in fs::read_dir(at)? {
            let path = entry?.path();
            if linked(&path)? {
                found.push(path);
            } else if path.is_dir() {
                collect(&path, found, depth + 1)?;
            }
        }
        Ok(())
    }
    fn copy(
        base: &Path,
        at: &Path,
        target: &Path,
        stack: &mut BTreeSet<PathBuf>,
        depth: usize,
    ) -> Result<()> {
        let source = fs::canonicalize(at)?;
        if depth > 128 || !source.starts_with(base) || !stack.insert(source.clone()) {
            return ci::fail("Installed browser link escapes or cycles within its root");
        }
        if source.is_file() {
            fs::create_dir_all(target.parent().ok_or("Materialization parent missing")?)?;
            fs::copy(&source, target)?;
        } else if source.is_dir() {
            fs::create_dir_all(target)?;
            for entry in fs::read_dir(&source)? {
                let entry = entry?;
                copy(
                    base,
                    &entry.path(),
                    &target.join(entry.file_name()),
                    stack,
                    depth + 1,
                )?;
            }
        } else {
            return ci::fail("Installed browser contains a nonregular link target");
        }
        stack.remove(&source);
        Ok(())
    }
    let base = fs::canonicalize(root)?;
    let mut links = Vec::new();
    collect(&base, &mut links, 0)?;
    for (index, path) in links.iter().enumerate() {
        let copy_at = scratch.join(format!("materialized-{index}"));
        copy(&base, path, &copy_at, &mut BTreeSet::new(), 0)?;
        #[cfg(windows)]
        {
            if path.is_dir() {
                fs::remove_dir(path)?;
            } else {
                fs::remove_file(path)?;
            }
        }
        #[cfg(not(windows))]
        fs::remove_file(path)?;
        fs::rename(&copy_at, path)?;
    }
    Ok(())
}

fn find(root: &Path, name: &str, depth: usize, matches: &mut Vec<PathBuf>) -> Result<()> {
    if depth > 128 {
        return ci::fail("Vendor resource nesting exceeds its bound");
    }
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            find(&entry.path(), name, depth + 1, matches)?;
        } else if entry.file_type()?.is_file() && entry.file_name() == name {
            matches.push(entry.path());
        }
    }
    Ok(())
}

fn executable(root: &Path, name: &str, required_suffix: Option<&str>) -> Result<PathBuf> {
    let mut found = Vec::new();
    find(root, name, 0, &mut found)?;
    if let Some(suffix) = required_suffix {
        found.retain(|path| path.to_string_lossy().replace('\\', "/").ends_with(suffix));
    }
    if found.len() != 1 {
        return ci::fail("Vendor browser executable is absent or ambiguous");
    }
    Ok(found.remove(0))
}

fn portable(path: &Path) -> Result<String> {
    Ok(path
        .to_str()
        .ok_or("Vendor path is not UTF-8")?
        .replace('\\', "/"))
}

/// Provision the reviewed platform-specific drivers, browsers, complete patch /
/// addon source archives, notices and GeoIP database into an absent directory.
/// Returns the source-offer document for `prepare-resources` to inventory.
pub fn run(destination: &Path) -> Result<PathBuf> {
    if destination.exists() {
        return ci::fail("Provision destination already exists");
    }
    let parent = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let mut staging =
        numera_core::storage::archive_paths::Staging::new(parent, ".numera-provision-")?;
    let root = staging.path()?.join("resources");
    fs::create_dir(&root)?;
    let result = provision(&root, staging.path()?);
    match result {
        Ok(()) => {
            if destination.exists() {
                return ci::fail("Provision destination appeared during acquisition");
            }
            fs::rename(&root, destination)?;
            staging.cleanup()?;
            Ok(destination.join("sources/SOURCE-OFFERS.json"))
        }
        Err(error) => {
            staging.cleanup()?;
            Err(error)
        }
    }
}

fn provision(root: &Path, scratch: &Path) -> Result<()> {
    let pins = pins()?;
    let platform = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH);
    let node = if cfg!(windows) { "node.exe" } else { "node" };
    let mut offers = Vec::new();
    let mut node_versions = BTreeSet::new();
    for backend in ["playwright", "patchright", "camoufox", "cloak"] {
        println!("Provisioning pinned {backend} SDK transport");
        let pin = &pins["drivers"][backend];
        let archive_pin = &pin["platforms"][&platform];
        let archive = scratch.join(format!("{backend}-driver.whl"));
        download(
            text(archive_pin, "url")?,
            &archive,
            text(archive_pin, "sha256")?,
            256 * 1024 * 1024,
        )?;
        let driver = root.join("drivers").join(backend);
        extract(
            &archive,
            &driver,
            &format!("{}/driver/", text(pin, "wheel_package")?),
        )?;
        fs::remove_file(archive)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(driver.join(node), fs::Permissions::from_mode(0o755))?;
        }
        if !driver.join("package/cli.js").is_file() {
            return ci::fail("Vendor SDK wheel has no driver");
        }
        let output = execute(Command::new(driver.join(node)).arg("--version"), 10)?;
        let version = String::from_utf8(output.stdout)?.trim().to_owned();
        if !version.starts_with('v')
            || version[1..].split('.').count() != 3
            || version[1..]
                .split('.')
                .any(|part| part.parse::<u32>().is_err())
        {
            return ci::fail("Vendor Node version is malformed");
        }
        if node_versions.insert(version.clone()) {
            let component = if node_versions.len() == 1 {
                "node".to_owned()
            } else {
                format!("node-{version}")
            };
            offers.push(json!({"component":component,"version":version,"license":"MIT and Node.js component terms","source_url":format!("https://github.com/nodejs/node/tree/{version}"),"archive":null}));
        }
        offers.push(json!({"component":backend,"version":text(pin,"version")?,"license":"Apache-2.0","source_url":if backend=="patchright" {"https://github.com/Kaliiiiiiiiii-Vinyzu/patchright"} else {"https://github.com/microsoft/playwright"},"archive":null}));
    }
    // The native Camoufox port and browser have MPL terms; the copied driver's
    // Playwright source offer is separately retained above and in its notices.
    offers.retain(|offer| offer["component"] != "camoufox");
    let mut manifest = json!({});
    let browsers = root.join("browsers");
    fs::create_dir(&browsers)?;
    for backend in ["playwright", "patchright"] {
        println!("Provisioning {backend} Chromium with its matching vendor driver");
        let driver = root.join("drivers").join(backend);
        let cache = browsers.join(backend);
        execute(
            Command::new(driver.join(node))
                .arg(driver.join("package/cli.js"))
                .args(["install", "chromium", "--no-shell"])
                .env("PLAYWRIGHT_BROWSERS_PATH", &cache),
            900,
        )?;
        if cache.join(".links").exists() {
            fs::remove_dir_all(cache.join(".links"))?;
        }
        materialize(&cache, scratch)?;
        let name = if cfg!(windows) {
            "chrome.exe"
        } else if cfg!(target_os = "macos") {
            "Google Chrome for Testing"
        } else {
            "chrome"
        };
        let binary = if cfg!(target_os = "macos") {
            let mut candidates = Vec::new();
            for name in ["Chromium", "Google Chrome for Testing"] {
                find(&cache, name, 0, &mut candidates)?;
            }
            candidates.retain(|path| {
                portable(path).is_ok_and(|name| name.contains(".app/Contents/MacOS/"))
            });
            if candidates.len() != 1 {
                return ci::fail("Vendor macOS browser executable is absent or ambiguous");
            }
            candidates.remove(0)
        } else {
            executable(&cache, name, None)?
        };
        let registry: Value =
            serde_json::from_slice(&fs::read(driver.join("package/browsers.json"))?)?;
        let chromium = registry["browsers"]
            .as_array()
            .ok_or("Vendor browser registry missing")?
            .iter()
            .find(|entry| entry["name"] == "chromium")
            .ok_or("Vendor Chromium pin missing")?;
        manifest[backend] = json!({"executable":portable(binary.strip_prefix(&browsers)?)?,"sha256":ci::digest(&binary)?,"version":text(chromium,"browserVersion")?,"revision":chromium["revision"]});
        offers.push(json!({"component":if backend=="playwright" {"chromium"} else {"chromium-patchright"},"version":text(chromium,"browserVersion")?,"license":"BSD-3-Clause and Chromium component terms","source_url":format!("https://chromium.googlesource.com/chromium/src/+/{}",text(chromium,"browserVersion")?),"archive":null}));
    }
    println!("Provisioning pinned Camoufox browser and default addon");
    let os = if cfg!(windows) {
        "win"
    } else if cfg!(target_os = "macos") {
        "mac"
    } else {
        "lin"
    };
    let arch = if cfg!(target_arch = "aarch64") {
        "arm64"
    } else {
        "x86_64"
    };
    let pin = &pins["camoufox"]["platforms"][format!("{os}.{arch}")];
    let archive = scratch.join("camoufox.zip");
    download(
        text(pin, "url")?,
        &archive,
        text(pin, "sha256")?,
        2 * 1024 * 1024 * 1024,
    )?;
    let camo = browsers.join("camoufox");
    extract(&archive, &camo, "")?;
    fs::remove_file(archive)?;
    let binary = executable(
        &camo,
        if cfg!(windows) {
            "camoufox.exe"
        } else if cfg!(target_os = "macos") {
            "camoufox"
        } else {
            "camoufox-bin"
        },
        if cfg!(target_os = "macos") {
            Some("Contents/MacOS/camoufox")
        } else {
            None
        },
    )?;
    let assets = if cfg!(target_os = "macos") {
        binary
            .parent()
            .and_then(Path::parent)
            .ok_or("Camoufox resource directory missing")?
            .join("Resources")
    } else {
        camo.clone()
    };
    let addon = assets.join("addons/UBO");
    let addon_pin = &pins["addons"]["ublock-origin"];
    let addon_archive = scratch.join("ublock-origin.xpi");
    download(
        text(addon_pin, "url")?,
        &addon_archive,
        text(addon_pin, "sha256")?,
        32 * 1024 * 1024,
    )?;
    // Replace only the newly extracted, owned staging copy. The browser binary
    // keeps its official release hash; the addon has its independent digest and
    // matching GPL corresponding-source archive.
    fs::remove_dir_all(&addon)?;
    extract(&addon_archive, &addon, "")?;
    fs::remove_file(addon_archive)?;
    let addon_manifest: Value = serde_json::from_slice(&fs::read(addon.join("manifest.json"))?)?;
    if addon_manifest["version"] != text(addon_pin, "version")?
        || addon_manifest["browser_specific_settings"]["gecko"]["id"] != text(addon_pin, "id")?
    {
        return ci::fail("Pinned Camoufox addon source version differs");
    }
    manifest["camoufox"] = json!({"executable":portable(binary.strip_prefix(&browsers)?)?,"sha256":ci::digest(&binary)?,"version":"158.0.1","addons":[portable(addon.strip_prefix(&browsers)?)?]});
    fs::write(
        camo.join("version.json"),
        serde_json::to_vec_pretty(
            &json!({"version":"158.0.1","build":"beta.41","sha256":text(pin,"sha256")?}),
        )?,
    )?;
    let geo = &pins["geoip"];
    download(
        text(geo, "url")?,
        &camo.join("geoip/mmdb/geoip aio by daijro-combined.mmdb"),
        text(geo, "sha256")?,
        256 * 1024 * 1024,
    )?;
    manifest["_geoip"] = json!({"camoufox":"camoufox/geoip"});
    download(
        text(geo, "notice_url")?,
        &root.join("notices/geoip-aio.md"),
        text(geo, "notice_sha256")?,
        1024 * 1024,
    )?;
    for source in pins["sources"].as_array().ok_or("Pinned sources missing")? {
        download(
            text(source, "source_url")?,
            &root.join(member(text(source, "archive")?)?),
            text(source, "sha256")?,
            512 * 1024 * 1024,
        )?;
        let mut offer = source.clone();
        offer
            .as_object_mut()
            .ok_or("Source offer malformed")?
            .remove("sha256");
        offers.push(offer);
    }
    offers.extend([
        json!({"component":"firefox","version":"158.0.1","license":"MPL-2.0 and Firefox component terms","source_url":"https://archive.mozilla.org/pub/firefox/releases/158.0.1/source/firefox-158.0.1.source.tar.xz","archive":null}),
        json!({"component":"geoip-aio","version":text(geo,"version")?,"license":"Independent database attribution and share-alike terms; see notices/geoip-aio.md","source_url":"https://github.com/daijro/geoip-all-in-one","archive":null}),
    ]);
    fs::create_dir_all(root.join("sources"))?;
    fs::write(
        root.join("sources/SOURCE-OFFERS.json"),
        serde_json::to_vec_pretty(&offers)?,
    )?;
    fs::write(
        browsers.join("manifest.json"),
        serde_json::to_vec_pretty(&manifest)?,
    )?;
    fs::write(
        root.join("notices/VENDOR-RESOURCES.txt"),
        "Vendor SDKs and browsers retain their packaged LICENSE, NOTICE and component credit files. Node.js: https://nodejs.org/en/about/previous-releases. Playwright/Patchright: Apache-2.0. Camoufox and its native port: MPL-2.0. uBlock Origin: GPL-3.0-or-later. Corresponding patch and addon sources are under sources/third-party; Firefox and Chromium source URLs are in sources/SOURCE-OFFERS.json. GeoIP data attribution: notices/geoip-aio.md. Cloak browser binaries are not included. No runtime application interpreter is installed.\n",
    )?;
    println!("Vendor bytes acquired and pinned; run prepare-resources to inventory this directory");
    Ok(())
}
