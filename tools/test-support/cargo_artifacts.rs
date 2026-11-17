//! Locate the libraries linked by this Cargo test when compiling a fixture with rustc.
use serde_json::Value;
use std::{
    fs, io,
    path::{Path, PathBuf},
    process::Command,
};

enum Layout {
    Flat { fingerprints: PathBuf },
    Split { build: PathBuf },
}

pub struct Artifacts {
    output_directory: PathBuf,
    fingerprint: Value,
    layout: Layout,
}

impl Artifacts {
    pub fn current() -> io::Result<Self> {
        let executable = std::env::current_exe()?;
        let output_directory = executable
            .parent()
            .ok_or_else(|| io::Error::other("Test executable has no parent"))?;
        let stem = executable
            .file_stem()
            .and_then(|name| name.to_str())
            .ok_or_else(|| io::Error::other("Test executable name is unavailable"))?;
        let (_, hash) = stem
            .rsplit_once('-')
            .ok_or_else(|| io::Error::other("Test executable has no Cargo unit hash"))?;
        let (fingerprint_directory, layout) = if output_directory.ends_with("deps") {
            let profile = output_directory
                .parent()
                .ok_or_else(|| io::Error::other("Cargo profile directory is unavailable"))?;
            let fingerprints = profile.join(".fingerprint");
            let own = fingerprints.join(format!("{}-{hash}", env!("CARGO_PKG_NAME")));
            (own, Layout::Flat { fingerprints })
        } else {
            let mut ancestors = output_directory.ancestors();
            ancestors.next();
            let unit = ancestors.next();
            let package = ancestors.next();
            let build = ancestors.next();
            match (unit, package, build) {
                (Some(unit), Some(package), Some(build))
                    if output_directory.ends_with("out")
                        && unit.ends_with(hash)
                        && package.ends_with(env!("CARGO_PKG_NAME"))
                        && build.ends_with("build") =>
                {
                    (
                        unit.join("fingerprint"),
                        Layout::Split {
                            build: build.to_owned(),
                        },
                    )
                }
                _ => return Err(io::Error::other("Unrecognized Cargo test artifact layout")),
            }
        };
        let fingerprint = fs::read_dir(&fingerprint_directory)?
            .collect::<io::Result<Vec<_>>>()?
            .into_iter()
            .map(|entry| entry.path())
            .find(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.starts_with("test-") && name.ends_with(".json"))
            })
            .ok_or_else(|| io::Error::other("Cargo test fingerprint is unavailable"))?;
        Ok(Self {
            output_directory: output_directory.to_owned(),
            fingerprint: serde_json::from_slice(&fs::read(fingerprint)?)?,
            layout,
        })
    }

    pub fn output_directory(&self) -> &Path {
        &self.output_directory
    }

    pub fn library(&self, name: &str) -> io::Result<PathBuf> {
        self.library_with(&self.fingerprint, name)
            .map(|(artifact, _)| artifact)
    }

    #[allow(dead_code)] // Only fixtures that directly compile a transitive library use this.
    pub fn library_dependency(&self, owner: &str, name: &str) -> io::Result<PathBuf> {
        let (_, fingerprint_directory) = self.library_with(&self.fingerprint, owner)?;
        let fingerprint = fingerprint_directory.join(format!("lib-{owner}.json"));
        let fingerprint = serde_json::from_reader(fs::File::open(fingerprint)?)?;
        self.library_with(&fingerprint, name)
            .map(|(artifact, _)| artifact)
    }

    fn library_with(&self, owner: &Value, name: &str) -> io::Result<(PathBuf, PathBuf)> {
        let fingerprint = owner["deps"]
            .as_array()
            .and_then(|deps| deps.iter().find(|row| row[1].as_str() == Some(name)))
            .and_then(|row| row[3].as_u64())
            .ok_or_else(|| io::Error::other(format!("Cargo dependency {name} is unavailable")))?;
        let fingerprint = format!("{:016x}", fingerprint.swap_bytes());
        let lib_prefix = format!("lib{name}-");
        let fingerprint_name = format!("lib-{name}");
        match &self.layout {
            Layout::Flat { fingerprints } => {
                for entry in fs::read_dir(self.output_directory())? {
                    let artifact = entry?.path();
                    let Some(hash) = artifact
                        .file_stem()
                        .and_then(|stem| stem.to_str())
                        .and_then(|stem| stem.strip_prefix(&lib_prefix))
                    else {
                        continue;
                    };
                    if artifact
                        .extension()
                        .is_none_or(|extension| extension != "rlib")
                    {
                        continue;
                    }
                    for package in [name.to_owned(), name.replace('_', "-")] {
                        let directory = fingerprints.join(format!("{package}-{hash}"));
                        if fingerprint_matches(&directory.join(&fingerprint_name), &fingerprint)? {
                            return Ok((artifact, directory));
                        }
                    }
                }
            }
            Layout::Split { build } => {
                for package in fs::read_dir(build)? {
                    let package = package?;
                    let key = package.file_name();
                    if !key
                        .to_str()
                        .is_some_and(|key| key.replace('-', "_") == name)
                    {
                        continue;
                    }
                    for unit in fs::read_dir(package.path())? {
                        let unit = unit?;
                        let unit_path = unit.path();
                        let key = unit.file_name();
                        let Some(hash) = key.to_str() else { continue };
                        let artifact = unit_path
                            .join("out")
                            .join(format!("{lib_prefix}{hash}.rlib"));
                        let directory = unit_path.join("fingerprint");
                        if artifact.is_file()
                            && fingerprint_matches(
                                &directory.join(&fingerprint_name),
                                &fingerprint,
                            )?
                        {
                            return Ok((artifact, directory));
                        }
                    }
                }
            }
        }
        Err(io::Error::other(format!(
            "Exact Cargo dependency {name} is unavailable"
        )))
    }

    pub fn add_dependency_search_paths(
        &self,
        command: &mut Command,
        response_file: &Path,
    ) -> io::Result<()> {
        match &self.layout {
            Layout::Flat { .. } => {
                let mut argument = std::ffi::OsString::from("dependency=");
                argument.push(self.output_directory());
                command.arg("-L").arg(argument);
            }
            Layout::Split { build } => {
                // In the split layout rustc still needs to resolve transitive
                // libraries by metadata hash. Use one argument file so a large
                // dependency graph fits the Windows process command-line limit.
                let mut paths = Vec::new();
                for package in fs::read_dir(build)? {
                    let package = package?;
                    if package.file_type()?.is_dir() {
                        for unit in fs::read_dir(package.path())? {
                            let out = unit?.path().join("out");
                            if out.is_dir() {
                                paths.push(out);
                            }
                        }
                    }
                }
                paths.sort_unstable();
                let mut arguments = String::new();
                for path in paths {
                    let text = path
                        .to_str()
                        .filter(|text| !text.contains(['\r', '\n']))
                        .ok_or_else(|| io::Error::other("Rustc search path is not a UTF-8 line"))?;
                    arguments.push_str("-L\ndependency=");
                    arguments.push_str(text);
                    arguments.push('\n');
                }
                fs::write(response_file, arguments)?;
                let mut argument = std::ffi::OsString::from("@");
                argument.push(response_file);
                command.arg(argument);
            }
        }
        Ok(())
    }
}

fn fingerprint_matches(path: &Path, expected: &str) -> io::Result<bool> {
    match fs::read_to_string(path) {
        Ok(actual) => Ok(actual.trim() == expected),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}
