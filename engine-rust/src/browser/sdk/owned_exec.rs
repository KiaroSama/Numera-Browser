//! POSIX vendor browsers publish their detached group before browser exec.
use crate::ownership::{identity, unit_report};
use serde_json::{Value, json};
use std::{io, path::Path, process::Command};

pub(crate) const RECEIPT: &str = "NUMERA_BROWSER_UNITS";
const EXECUTABLE: &str = "NUMERA_BROWSER_EXECUTABLE";
const OWNER: &str = "NUMERA_BROWSER_EXEC_OWNER";
const PARENT_RECEIPT: &str = "NUMERA_BROWSER_PARENT_UNITS";
const PARENT_SCOPE: &str = "NUMERA_BROWSER_PARENT_SCOPE";

pub(super) fn prepare(options: &mut Value, default_executable: &str) -> io::Result<()> {
    let Some(receipt) = std::env::var_os(RECEIPT) else {
        return Ok(());
    };
    let receipt = unit_report::validate(Path::new(&receipt))?;
    let inherited = std::env::var_os(unit_report::ENV)
        .map(|path| unit_report::validate(Path::new(&path)))
        .transpose()?
        .filter(|parent| {
            parent.parent().and_then(Path::parent) != receipt.parent().and_then(Path::parent)
        });
    let explicit = options["executablePath"].as_str();
    if explicit.is_none()
        && (options["channel"]
            .as_str()
            .is_some_and(|channel| channel != "chromium")
            || (options["headless"] == true && options["channel"].is_null()))
    {
        return Err(io::Error::other(
            "Owned browser launch requires an explicit executable for this SDK channel",
        ));
    }
    let executable = explicit.unwrap_or(default_executable).to_owned();
    if !Path::new(&executable).is_file() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Owned browser executable is unavailable",
        ));
    }
    let worker = std::process::id();
    let birth = identity::started_at(worker)?
        .ok_or_else(|| io::Error::other("Browser worker birth identity is unavailable"))?;
    #[allow(unsafe_code)]
    // SAFETY: getpgrp reads this process's group without pointers or mutation.
    let group = unsafe { libc::getpgrp() };
    let private = |name: &str| {
        matches!(
            name,
            "NUMERA_PASSWORD"
                | "NUMERA_API_TOKEN"
                | "NUMERA_JOB_ARGUMENT_KEY"
                | "NUMERA_UPDATE_TOKEN"
                | "NUMERA_UPDATE_TICKET"
                | "NUMERA_UPDATE_ACTIVATION"
                | "NUMERA_HELPER_UNITS"
                | "NUMERA_HELPER_SCOPE"
                | "NUMERA_TREE"
                | "NUMERA_READY_FILE"
                | "NUMERA_PARENT_PID"
                | "NUMERA_NATIVE_SUPERVISE"
                | "NUMERA_NATIVE_JOB_GATE"
                | "NUMERA_JOB_FILE"
                | "NUMERA_JOB_PARENT_PID"
                | "NUMERA_JOB_PARENT_TOKEN"
                | PARENT_RECEIPT
                | PARENT_SCOPE
        )
    };
    let mut environment = options["env"].as_array().cloned().unwrap_or_else(|| {
        let mut values = std::env::vars()
            .filter(|(key, _)| !private(key))
            .map(|(name, value)| json!({"name":name,"value":value}))
            .collect::<Vec<_>>();
        for (name, value) in [
            ("PW_LANG_NAME", "rust"),
            ("PW_LANG_NAME_VERSION", env!("CARGO_PKG_RUST_VERSION")),
            ("PW_CLI_DISPLAY_VERSION", env!("CARGO_PKG_VERSION")),
            ("CLOAKBROWSER_AUTO_UPDATE", "false"),
        ] {
            values.retain(|entry| entry["name"] != name);
            values.push(json!({"name":name,"value":value}));
        }
        values
    });
    environment.retain(|entry| entry["name"].as_str().is_some_and(|name| !private(name)));
    for (name, value) in [
        (RECEIPT, receipt.to_string_lossy().into_owned()),
        (EXECUTABLE, executable),
        (OWNER, format!("{worker}:{birth}:{group}")),
    ] {
        environment.retain(|entry| entry["name"] != name);
        environment.push(json!({"name":name,"value":value}));
    }
    if let Some(parent) = inherited {
        if parent
            .parent()
            .and_then(Path::file_name)
            .is_none_or(|name| name != "jobs")
        {
            return Err(io::Error::other(
                "Browser capture receipt is outside its managed owner",
            ));
        }
        let scope = match std::env::var(unit_report::SCOPE_ENV) {
            Ok(scope) => Some(scope),
            Err(std::env::VarError::NotPresent) => None,
            Err(_) => return Err(io::Error::other("Browser capture scope is invalid")),
        };
        environment.push(json!({"name":PARENT_RECEIPT,"value":parent.to_string_lossy()}));
        environment.push(json!({"name":PARENT_SCOPE,"value":serde_json::to_string(&scope)?}));
    }
    options["env"] = environment.into();
    options["executablePath"] = std::env::current_exe()?
        .to_str()
        .ok_or_else(|| io::Error::other("Native browser entry path is not Unicode"))?
        .into();
    Ok(())
}

/// The vendor has created this process in its own group. No browser code runs
/// until the group has a durable receipt, guarded by the original worker's birth.
pub(crate) fn enter() -> Option<std::process::ExitCode> {
    let executable = std::env::var_os(EXECUTABLE)?;
    let result = (|| -> io::Result<()> {
        use std::os::unix::process::CommandExt;
        let receipt = std::env::var_os(RECEIPT)
            .ok_or_else(|| io::Error::other("Browser ownership receipt is missing"))?;
        let owner =
            std::env::var(OWNER).map_err(|_| io::Error::other("Browser exec owner is missing"))?;
        let fields = owner.split(':').collect::<Vec<_>>();
        let invalid_owner = || io::Error::other("Browser exec owner is invalid");
        if fields.len() != 3 {
            return Err(invalid_owner());
        }
        let pid = fields[0].parse::<u32>().map_err(|_| invalid_owner())?;
        let birth = fields[1].parse::<u64>().map_err(|_| invalid_owner())?;
        let group = fields[2].parse::<u32>().map_err(|_| invalid_owner())?;
        let current = std::process::id();
        let token = identity::started_at(current)?
            .ok_or_else(|| io::Error::other("Browser exec birth identity is missing"))?;
        let record = json!({"pid":current,"pidToken":token,"tree":{"kind":"pgid","group":current}});
        if !crate::ownership::recorded_tree::current_record_matches(&record)? {
            return Err(io::Error::other(
                "Vendor browser is not in its own captured group",
            ));
        }
        let path = Path::new(&receipt);
        // The owner settles its worker before reading this same locked receipt.
        // A late wrapper cannot activate after the worker's death; a damaged
        // publication cannot be mistaken for an empty tree.
        unit_report::record_browser_for_owner(
            path,
            &json!({"pid":current,"token":token,"tree":record["tree"]}),
            pid,
            birth,
            group,
        )?;
        // A cross-root capture can outlive deletion of a command's temporary
        // data. Publish the same flat identity there, independently guarded,
        // before exec so its owner never depends on that temporary receipt.
        match (
            std::env::var_os(PARENT_RECEIPT),
            std::env::var_os(PARENT_SCOPE),
        ) {
            (Some(parent), Some(scope)) => {
                let scope: Option<String> = serde_json::from_str(
                    scope
                        .to_str()
                        .ok_or_else(|| io::Error::other("Browser capture scope is invalid"))?,
                )?;
                unit_report::record_browser_scoped_for_owner(
                    Path::new(&parent),
                    &json!({"pid":current,"token":token,"tree":record["tree"]}),
                    pid,
                    birth,
                    group,
                    scope.as_deref(),
                )?;
            }
            (None, None) => {}
            _ => return Err(io::Error::other("Browser capture ownership is incomplete")),
        }
        let error = Command::new(executable)
            .args(std::env::args_os().skip(1))
            .env_remove(EXECUTABLE)
            .env_remove(OWNER)
            .env_remove(RECEIPT)
            .env_remove(PARENT_RECEIPT)
            .env_remove(PARENT_SCOPE)
            .exec();
        Err(error)
    })();
    if result.is_err() {
        let log = crate::runtime_log::RuntimeLog::without_file();
        log.message(
            "ERROR",
            "numera.browser-adapter",
            "Owned browser exec was refused",
        );
    }
    Some(std::process::ExitCode::FAILURE)
}
