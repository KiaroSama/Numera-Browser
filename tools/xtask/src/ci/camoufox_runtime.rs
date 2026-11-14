//! Exact hosted Ubuntu package admission. Unknown package states are refused;
//! only missing allowlisted packages reach noninteractive APT, then rechecked.
use super::{Result, fail, owned};
use numera_core::runtime_log::RuntimeLog;
use std::{collections::BTreeMap, fs, path::Path, process::Command, time::Duration};
pub const PACKAGES: [&str; 30] = [
    "libasound2t64",
    "libatk1.0-0t64",
    "libavcodec60",
    "libcairo-gobject2",
    "libcairo2",
    "libdbus-1-3",
    "libfontconfig1",
    "libfreetype6",
    "libgdk-pixbuf-2.0-0",
    "libglib2.0-0t64",
    "libgtk-3-0t64",
    "libpango-1.0-0",
    "libpangocairo-1.0-0",
    "libx11-6",
    "libx11-xcb1",
    "libxcb-shm0",
    "libxcb1",
    "libxcomposite1",
    "libxcursor1",
    "libxdamage1",
    "libxext6",
    "libxfixes3",
    "libxi6",
    "libxrandr2",
    "libxrender1",
    "xvfb",
    "libdbus-glib-1-2",
    "libxtst6",
    "libpci3",
    "zstd",
];
#[derive(Clone, Debug)]
pub struct Host {
    pub actions: String,
    pub repository: String,
    pub runner: String,
    pub runner_arch: String,
    pub engine: String,
    pub system: String,
    pub machine: String,
    pub distro: String,
    pub release: String,
    pub sdk: String,
    pub browser: String,
}
impl Host {
    pub fn require(&self) -> Result<()> {
        if self.actions != "true"
            || self.repository != "KiaroSama/Numera-Browser"
            || self.runner != "Linux"
            || self.runner_arch != "X64"
            || self.engine != "camoufox"
            || self.system != "Linux"
            || self.machine != "x86_64"
            || self.distro != "ubuntu"
            || self.release != "24.04"
            || self.sdk != "1.62.0"
            || self.browser != "158.0.1"
        {
            return fail(
                "Camoufox runtime installation requires the exact owning Ubuntu hosted SDK job",
            );
        }
        Ok(())
    }
    pub fn current(resources: &Path) -> Result<Self> {
        let value = |key| std::env::var(key).unwrap_or_default();
        let release = fs::read_to_string("/etc/os-release")?;
        let releases = release
            .lines()
            .filter_map(|line| line.split_once('='))
            .map(|(key, value)| (key, value.trim_matches('"')))
            .collect::<BTreeMap<_, _>>();
        let manifest: serde_json::Value = serde_json::from_slice(&fs::read(
            resources.join("drivers/camoufox/package/package.json"),
        )?)?;
        let browsers: serde_json::Value =
            serde_json::from_slice(&fs::read(resources.join("browsers/manifest.json"))?)?;
        Ok(Self {
            actions: value("GITHUB_ACTIONS"),
            repository: value("GITHUB_REPOSITORY"),
            runner: value("RUNNER_OS"),
            runner_arch: value("RUNNER_ARCH"),
            engine: value("NUMERA_TEST_ENGINE"),
            system: if cfg!(target_os = "linux") {
                "Linux".into()
            } else {
                std::env::consts::OS.into()
            },
            machine: std::env::consts::ARCH.into(),
            distro: releases.get("ID").copied().unwrap_or("").into(),
            release: releases.get("VERSION_ID").copied().unwrap_or("").into(),
            sdk: manifest["version"].as_str().unwrap_or("").into(),
            browser: browsers["camoufox"]["version"]
                .as_str()
                .unwrap_or("")
                .into(),
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invocation {
    pub argv: Vec<String>,
    pub seconds: u64,
    pub environment: BTreeMap<String, String>,
}
pub struct Captured {
    pub exit: i32,
    pub stdout: Vec<u8>,
}
pub trait Runner {
    fn command(&mut self, invocation: &Invocation) -> Result<Captured>;
}
fn environment() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("LC_ALL".into(), "C.UTF-8".into()),
        ("LANGUAGE".into(), "C".into()),
        ("DEBIAN_FRONTEND".into(), "noninteractive".into()),
    ])
}
pub fn query() -> Invocation {
    let mut argv = [
        "dpkg-query",
        "-W",
        "-f=${Package}\t${Architecture}\t${db:Status-Status}\t${db:Status-Eflag}\n",
        "--",
    ]
    .map(str::to_owned)
    .to_vec();
    argv.extend(PACKAGES.map(str::to_owned));
    Invocation {
        argv,
        seconds: 10,
        environment: environment(),
    }
}
pub fn missing(output: &Captured) -> Result<Vec<String>> {
    if ![0, 1].contains(&output.exit) || output.stdout.len() > 32768 {
        return fail("Camoufox runtime package database query failed");
    }
    let text = std::str::from_utf8(&output.stdout)?;
    let mut found = BTreeMap::new();
    for line in text.lines() {
        let fields = line.split('\t').collect::<Vec<_>>();
        if fields.len() != 4
            || !PACKAGES.contains(&fields[0])
            || found.contains_key(fields[0])
            || !["amd64", "all"].contains(&fields[1])
            || fields[3] != "ok"
            || !["installed", "not-installed", "config-files"].contains(&fields[2])
        {
            return fail("Camoufox runtime package state is ambiguous or unhealthy");
        }
        found.insert(fields[0], fields[2]);
    }
    if output.exit == 0 && found.len() != PACKAGES.len()
        || output.exit == 1 && found.len() == PACKAGES.len()
    {
        return fail("Camoufox runtime package query result is inconsistent");
    }
    Ok(PACKAGES
        .into_iter()
        .filter(|name| found.get(name) != Some(&"installed"))
        .map(str::to_owned)
        .collect())
}
pub fn prepare(host: &Host, runner: &mut impl Runner) -> Result<()> {
    host.require()?;
    let absent = missing(&runner.command(&query())?)?;
    if !absent.is_empty() {
        let prefix = [
            "sudo",
            "-n",
            "env",
            "DEBIAN_FRONTEND=noninteractive",
            "LC_ALL=C.UTF-8",
            "LANGUAGE=C",
            "apt-get",
        ]
        .map(str::to_owned)
        .to_vec();
        let mut update = prefix.clone();
        update.push("update".into());
        let mut install = prefix;
        install.extend(
            [
                "install",
                "-y",
                "--no-install-recommends",
                "--no-remove",
                "--no-upgrade",
            ]
            .map(str::to_owned),
        );
        install.extend(absent);
        for argv in [update, install] {
            if runner
                .command(&Invocation {
                    argv,
                    seconds: 270,
                    environment: environment(),
                })?
                .exit
                != 0
            {
                return fail("Camoufox runtime package installation failed");
            }
        }
        if !missing(&runner.command(&query())?)?.is_empty() {
            return fail("Camoufox runtime packages remain missing after installation");
        }
    }
    Ok(())
}
pub fn logged(host: &Host, runner: &mut impl Runner, logs: &Path) -> i32 {
    let log = RuntimeLog::new(logs, "camoufox-runtime");
    let code = if prepare(host, runner).is_ok() {
        log.message(
            "INFO",
            "numera.camoufox-runtime",
            &format!("Runtime packages verified count={}", PACKAGES.len()),
        );
        0
    } else {
        log.event(
            "ERROR",
            "numera.camoufox-runtime",
            "Camoufox runtime prerequisite verification failed",
        );
        1
    };
    if code == 0 {
        log.event(
            "INFO",
            "numera.camoufox-runtime",
            "Runtime preparation finished exit=0",
        );
    } else {
        log.event(
            "INFO",
            "numera.camoufox-runtime",
            "Runtime preparation finished exit=1",
        );
    }
    code
}
struct Native<'a> {
    root: &'a Path,
}
impl Runner for Native<'_> {
    fn command(&mut self, invocation: &Invocation) -> Result<Captured> {
        let mut command = Command::new(&invocation.argv[0]);
        command
            .args(&invocation.argv[1..])
            .envs(&invocation.environment);
        let output = owned::run(
            self.root,
            &mut command,
            owned::limits(
                Duration::from_secs(invocation.seconds),
                Duration::from_secs(invocation.seconds.min(120)),
                32768,
            ),
        )?
        .command;
        Ok(Captured {
            exit: output.exit,
            stdout: output.stdout,
        })
    }
}
pub fn run(root: &Path, resources: &Path) -> Result<()> {
    let host =
        Host::current(resources).map_err(|_| "Camoufox runtime host metadata is unavailable")?;
    let logs = log_directory(root, std::env::var_os("NUMERA_LOG_DIR").as_deref());
    if logged(&host, &mut Native { root }, &logs) != 0 {
        return fail("Camoufox runtime prerequisites failed");
    }
    Ok(())
}
pub fn log_directory(root: &Path, explicit: Option<&std::ffi::OsStr>) -> std::path::PathBuf {
    explicit
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| root.join("build/logs"))
}
