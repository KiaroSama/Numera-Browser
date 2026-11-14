use numera_core::cli;
use serde_json::Value;

#[test]
fn version_flag_matches_original_root_scope_and_value_admission()
-> Result<(), Box<dyn std::error::Error>> {
    // Frozen execution of the immutable source; native assertions below remain live.
    let rows: Vec<Value> = serde_json::from_str(include_str!(
        "../fixtures/characterization/version_flag_matches_original_root_scope_and_value_admission.json"
    ))?;
    assert_eq!(rows.len(), 117, "Version flag observations missing");
    #[cfg(not(unix))]
    let rows: Vec<Value> = rows
        .into_iter()
        .filter(|row| row["argv"][0].is_string())
        .collect();
    assert_eq!(
        rows.len(),
        if cfg!(unix) { 117 } else { 114 },
        "Applicable version flag observations missing"
    );
    let mut registry = cli::registry();
    registry.build();
    for (index, row) in rows.iter().enumerate() {
        let argv = row["argv"]
            .as_array()
            .ok_or("Version arguments missing")?
            .iter()
            .map(argument)
            .collect::<Result<Vec<_>, _>>()?;
        let result = cli::parse(std::iter::once(std::ffi::OsString::from("numera")).chain(argv));
        assert_eq!(
            i64::from(result.as_ref().err().map_or(0, clap::Error::exit_code)),
            row["exit"].as_i64().ok_or("Version exit missing")?,
            "CLI version flag admission differs: case={index}"
        );
        match result {
            Ok(matches) => assert_eq!(
                cli::reflection::values(&registry, &matches),
                row["values"],
                "CLI version flag values differ: case={index}"
            ),
            Err(error) => {
                assert!(
                    row.get("values").is_none(),
                    "CLI version flag parsed values missing: case={index}"
                );
                if let Some(option) = row["invalid_option"].as_str() {
                    assert!(
                        error.to_string().contains(option),
                        "CLI argument error precedence differs: case={index}"
                    );
                }
                if let Some(scope) = row["help"].as_str() {
                    assert_eq!(error.kind(), clap::error::ErrorKind::DisplayHelp);
                    let usage = format!("Usage: {scope}");
                    let text = error.to_string();
                    let mut selected = &registry;
                    for name in scope.split_whitespace().skip(1) {
                        selected = selected
                            .find_subcommand(name)
                            .ok_or("Help command missing")?;
                    }
                    let expected = selected
                        .clone()
                        .try_get_matches_from([
                            "numera",
                            row["help_flag"].as_str().ok_or("Help flag missing")?,
                        ])
                        .expect_err("Help must return a display");
                    assert_eq!(
                        text,
                        expected.to_string(),
                        "CLI help rendering differs: case={index}"
                    );
                    assert!(
                        text.lines()
                            .any(|line| line
                                .strip_prefix(&usage)
                                .is_some_and(|rest| rest.is_empty()
                                    || rest.trim_start().starts_with(['[', '<']))),
                        "CLI help scope differs: case={index}"
                    );
                }
                if let Some(version) = row["version"].as_str() {
                    assert_eq!(error.kind(), clap::error::ErrorKind::DisplayVersion);
                    assert_eq!(
                        error.to_string(),
                        version,
                        "CLI version output differs: case={index}"
                    );
                }
            }
        }
    }
    Ok(())
}

fn argument(value: &Value) -> Result<std::ffi::OsString, Box<dyn std::error::Error>> {
    if let Some(text) = value.as_str() {
        return Ok(text.into());
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStringExt;
        Ok(std::ffi::OsString::from_vec(serde_json::from_value(
            value.clone(),
        )?))
    }
    #[cfg(not(unix))]
    Err("Native-byte arguments require Unix".into())
}
