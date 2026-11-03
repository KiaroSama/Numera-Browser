//! Native portable resource selection; SDK enum conversion stays in the adapter.
use crate::storage::{
    archive_paths::Staging,
    locks::{DataLock, resolved},
};
use serde_json::{Map, Value};
use std::{
    collections::BTreeMap,
    fs, io,
    path::{Path, PathBuf},
};

pub struct Discovery<'a> {
    pub app: &'a Path,
    pub environment: &'a BTreeMap<String, String>,
    pub default_addons: &'a [String],
}

#[derive(Default)]
pub struct Resources {
    pub options: Map<String, Value>,
    pub environment: BTreeMap<String, String>,
    pub geoip: Option<PathBuf>,
}

pub fn discover(app: &Path) -> io::Result<Option<PathBuf>> {
    for base in std::iter::once(app).chain(app.parent()) {
        let directory = base.join("browsers");
        if directory.is_dir() && fs::read_dir(&directory)?.next().transpose()?.is_some() {
            return Ok(Some(directory));
        }
    }
    Ok(None)
}
fn contained(root: &Path, relative: &str) -> io::Result<PathBuf> {
    let base = resolved(root)?;
    let path = crate::storage::archive_paths::member_path(&base, relative)?;
    let target = resolved(&path)?;
    if !target.starts_with(&base) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Bundled browser resource escapes its folder",
        ));
    }
    Ok(target)
}
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
fn seed(source: &Path, target: &Path, depth: usize) -> io::Result<()> {
    if depth > 128 {
        return Err(io::Error::other("Bundled GeoIP nesting exceeds limit"));
    }
    for root in [source, target] {
        if fs::symlink_metadata(root).is_ok_and(|metadata| linked(&metadata)) {
            return Err(io::Error::other(
                "GeoIP symbolic-link resources were not modified",
            ));
        }
    }
    fs::create_dir_all(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        if linked(&metadata) {
            return Err(io::Error::other(
                "Bundled GeoIP symbolic links require explicit handling",
            ));
        }
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| io::Error::other("GeoIP resource name is not Unicode"))?;
        let destination = crate::storage::archive_paths::member_path(target, &name)?;
        if destination.try_exists()? && linked(&fs::symlink_metadata(&destination)?) {
            return Err(io::Error::other("GeoIP destination link was not modified"));
        }
        if metadata.is_dir() {
            seed(&entry.path(), &destination, depth + 1)?;
        } else if metadata.is_file() && !destination.try_exists()? {
            let mut staging = Staging::new(target, ".geoip-seed-")?;
            let temporary = staging.path()?.join("resource");
            fs::copy(entry.path(), &temporary)?;
            let mut permissions = fs::metadata(&temporary)?.permissions();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                permissions.set_mode(permissions.mode() | 0o200);
            }
            #[cfg(windows)]
            {
                // Windows readonly flags do not change Unix permission bits.
                #[allow(clippy::permissions_set_readonly_false)]
                permissions.set_readonly(false);
            }
            fs::set_permissions(&temporary, permissions)?;
            let file = fs::File::options().write(true).open(&temporary)?;
            file.set_times(
                fs::FileTimes::new()
                    .set_accessed(metadata.accessed()?)
                    .set_modified(metadata.modified()?),
            )?;
            file.sync_all()?;
            drop(file);
            // Link publication is exclusive: a vendor refresh appearing meanwhile wins.
            match fs::hard_link(&temporary, &destination) {
                Ok(()) => {}
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
            staging.cleanup()?;
        }
    }
    Ok(())
}
fn text<'a>(object: &'a Value, key: &str) -> io::Result<&'a str> {
    object[key].as_str().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Bundled {key} is not text"),
        )
    })
}
pub fn select(
    root: Option<&Path>,
    engine: &str,
    data: &Path,
    environment: &BTreeMap<String, String>,
    default_addons: &[String],
) -> io::Result<Resources> {
    if !["playwright", "patchright", "camoufox", "cloak"].contains(&engine) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Unknown browser backend",
        ));
    }
    if matches!(engine, "playwright" | "patchright")
        && environment
            .get("PLAYWRIGHT_BROWSERS_PATH")
            .is_some_and(|path| !path.is_empty())
    {
        return Ok(Resources::default());
    }
    let Some(root) = root else {
        return Ok(Resources::default());
    };
    if !root.join("manifest.json").is_file() {
        return Ok(Resources::default());
    }
    let manifest: Value =
        serde_json::from_slice(&fs::read(root.join("manifest.json"))?).map_err(io::Error::other)?;
    let mut resources = Resources::default();
    if let Some(relative) = manifest["_geoip"][engine]
        .as_str()
        .filter(|path| !path.is_empty())
    {
        let source = contained(root, relative)?;
        if !source.is_dir() {
            return Err(io::Error::new(
                io::ErrorKind::NotFound,
                format!("The bundled {engine} GeoIP data is missing"),
            ));
        }
        let destination = data.join("browser-cache").join(engine);
        for directory in [
            data.to_owned(),
            data.join("browser-cache"),
            destination.clone(),
        ] {
            if fs::symlink_metadata(&directory).is_ok_and(|metadata| linked(&metadata)) {
                return Err(io::Error::other("GeoIP cache link was not modified"));
            }
        }
        let lock = DataLock::transaction(data)?;
        let _guard = lock.acquire_configured()?;
        seed(&source, &destination, 0)?;
        if engine == "cloak" && !environment.contains_key("CLOAKBROWSER_CACHE_DIR") {
            resources.environment.insert(
                "CLOAKBROWSER_CACHE_DIR".into(),
                destination
                    .to_str()
                    .ok_or_else(|| io::Error::other("GeoIP cache path is not Unicode"))?
                    .into(),
            );
        } else if engine == "camoufox" {
            resources.geoip = Some(destination);
        }
    }
    let entry = &manifest[engine];
    if entry.is_null() {
        if engine == "cloak" {
            return Ok(resources);
        }
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("The portable bundle has no {engine} browser"),
        ));
    }
    let executable = contained(root, text(entry, "executable")?)?;
    if !executable.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!(
                "The bundled {engine} executable is missing: {}",
                executable.display()
            ),
        ));
    }
    let path = executable
        .to_str()
        .ok_or_else(|| io::Error::other("Browser executable path is not Unicode"))?;
    if engine == "cloak" {
        if environment
            .get("CLOAKBROWSER_BINARY_PATH")
            .is_some_and(|override_path| !override_path.is_empty() && override_path != path)
            || environment
                .get("CLOAKBROWSER_LICENSE_KEY")
                .is_some_and(|key| !key.is_empty())
        {
            return Ok(resources);
        }
        resources
            .environment
            .insert("CLOAKBROWSER_BINARY_PATH".into(), path.into());
        resources
            .options
            .insert("browser_version".into(), text(entry, "version")?.into());
        return Ok(resources);
    }
    resources
        .options
        .insert("executable_path".into(), path.into());
    if engine == "camoufox" {
        resources.options.insert(
            "ff_version".into(),
            text(entry, "version")?
                .split('.')
                .next()
                .unwrap_or("")
                .into(),
        );
        let addons = entry["addons"]
            .as_array()
            .ok_or_else(|| io::Error::other("Bundled Camoufox addons are not a list"))?;
        let paths = addons
            .iter()
            .map(|addon| {
                let path = contained(
                    root,
                    addon
                        .as_str()
                        .ok_or_else(|| io::Error::other("Bundled addon path is not text"))?,
                )?;
                if !path.join("manifest.json").is_file() {
                    return Err(io::Error::new(
                        io::ErrorKind::NotFound,
                        format!(
                            "The bundled Camoufox addon is incomplete: {}",
                            path.display()
                        ),
                    ));
                }
                path.into_os_string()
                    .into_string()
                    .map_err(|_| io::Error::other("Addon path is not Unicode"))
            })
            .collect::<io::Result<Vec<_>>>()?;
        if paths.len() != default_addons.len() {
            return Err(io::Error::other(
                "The portable bundle is missing a default Camoufox addon",
            ));
        }
        resources.options.insert(
            "addons".into(),
            serde_json::to_value(paths).map_err(io::Error::other)?,
        );
        resources.options.insert(
            "exclude_addons".into(),
            serde_json::to_value(default_addons).map_err(io::Error::other)?,
        );
    }
    Ok(resources)
}
