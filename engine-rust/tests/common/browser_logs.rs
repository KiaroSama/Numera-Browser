use std::{error::Error, fs, path::Path};

pub fn inspect(root: &Path, expected: &[&str]) -> Result<String, Box<dyn Error>> {
    let logs = fs::read_dir(root.join("logs"))?.collect::<Result<Vec<_>, _>>()?;
    assert_eq!(logs.len(), 2, "Unexpected browser diagnostic file count");
    let named = |prefix: &str| {
        logs.iter()
            .filter(|entry| {
                entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| name.starts_with(prefix))
            })
            .collect::<Vec<_>>()
    };
    let sdk = named("browser-adapter_");
    let native = named("native-browser_");
    assert_eq!(sdk.len(), 1, "SDK log missing or duplicated");
    assert_eq!(native.len(), 1, "Native timing log missing or duplicated");
    let content = fs::read_to_string(native[0].path())?;
    let phases = content
        .lines()
        .map(|line| {
            let (stamp, event) = line
                .split_once("] [INFO] [entry] Native timing phase=")
                .ok_or("Unexpected native browser log entry")?;
            chrono::NaiveDateTime::parse_from_str(
                stamp.strip_prefix('[').ok_or("Missing timing timestamp")?,
                "%Y-%m-%d %H:%M:%S UTC",
            )?;
            let (phase, elapsed) = event
                .split_once(" elapsed_ns=")
                .ok_or("Missing native browser duration")?;
            assert!(
                !elapsed.is_empty() && elapsed.bytes().all(|byte| byte.is_ascii_digit()),
                "Unsafe native browser timing value"
            );
            elapsed.parse::<u64>()?;
            Ok(phase.to_owned())
        })
        .collect::<Result<Vec<_>, Box<dyn Error>>>()?;
    assert_eq!(phases, expected, "Native browser lifecycle differs");
    Ok(fs::read_to_string(sdk[0].path())?)
}
