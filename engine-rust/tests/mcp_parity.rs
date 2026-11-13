use numera_core::{cli::profile_commands::Outcome, mcp};
use serde_json::{Value, json};
use std::{process::Command, time::Duration};
mod common;
use numera_core as core;
#[allow(dead_code)]
#[path = "common/browser_fixture.rs"]
mod fixture;
#[allow(dead_code)]
#[path = "common/http_fixture.rs"]
mod http_fixture;

#[test]
fn stdio_text_boundaries_match_original_responses() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::{BufReader, Cursor};

    let temporary = common::TempDirectory::new("mcp-text")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let rows: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/characterization/stdio_text_boundaries_match_original_responses.json"
    ))?;
    assert_eq!(rows.len(), 99, "MCP text observations missing");
    for (index, row) in rows.iter().enumerate() {
        let body = row["body"].as_str().ok_or("MCP input missing")?;
        for capacity in [1, 7] {
            let mut input = BufReader::with_capacity(capacity, Cursor::new(body.as_bytes()));
            let mut output = Vec::new();
            mcp::serve(&mut input, &mut output, |message| {
                mcp::handle(message, |_| panic!("MCP text fixture executed a tool"))
            })?;
            let responses: Vec<Value> = output
                .split(|byte| *byte == b'\n')
                .filter(|line| !line.is_empty())
                .map(serde_json::from_slice)
                .collect::<Result<_, _>>()?;
            assert_eq!(
                json!(responses),
                row["responses"],
                "MCP stdio text response differs: case={index} capacity={capacity}"
            );
            assert!(output.is_empty() || output.last() == Some(&b'\n'));
        }
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn captured_command_errors_match_original_stream_selection()
-> Result<(), Box<dyn std::error::Error>> {
    use numera_core::{mcp_command, ownership::capture};
    use std::fs;

    const CHILD_ROOT: &str = "NUMERA_MCP_OUTPUT_FIXTURE_ROOT";
    if let Some(root) = std::env::var_os(CHILD_ROOT) {
        let root = std::path::PathBuf::from(root);
        let rows: Vec<Value> = serde_json::from_slice(&fs::read(root.join("cases.json"))?)?;
        // Only this exact-test child changes CWD; sibling tests keep their own directory.
        std::env::set_current_dir(&root)?;
        let executable = root.join(if cfg!(windows) {
            "mcp-output.exe"
        } else {
            "mcp-output"
        });
        for (index, row) in rows.iter().enumerate() {
            let arguments = json!({"tag":index.to_string()});
            let actual = mcp_command::call(
                &executable,
                None,
                None,
                "list",
                arguments.as_object().ok_or("Fixture arguments missing")?,
            )?;
            let exit = if actual["isError"] == true {
                let payload: Value = serde_json::from_str(
                    actual["content"][0]["text"]
                        .as_str()
                        .ok_or("MCP error text missing")?,
                )?;
                payload["exit"].clone()
            } else {
                actual["_meta"]["exitCode"].clone()
            };
            assert_eq!(
                exit, row["exit"],
                "MCP fixture command did not reach output interpretation"
            );
            println!(
                "\nMCP output result {}",
                json!({"index":index,"actual":actual})
            );
        }
        return Ok(());
    }

    let temporary = common::TempDirectory::new("mcp-output")?;
    // Frozen immutable source observation; all native assertions below remain live.
    let rows: Vec<Value> = serde_json::from_str(include_str!(
        "fixtures/characterization/captured_command_errors_match_original_stream_selection.json"
    ))?;
    assert_eq!(rows.len(), 44, "MCP output observations missing");
    fs::write(
        temporary.path().join("cases.json"),
        include_bytes!(
            "fixtures/characterization/captured_command_errors_match_original_stream_selection.json"
        ),
    )?;
    fixture::executable(
        &temporary.path().join("mcp-output"),
        "mcp-output",
        json!({"cases":temporary.path().join("cases.json")}),
    )?;
    let mut child = Command::new(std::env::current_exe()?);
    child
        .args([
            "--exact",
            "captured_command_errors_match_original_stream_selection",
            "--test-threads=1",
            "--nocapture",
        ])
        .env(CHILD_ROOT, temporary.path());
    let mut units =
        numera_core::ownership::capture_units::Units::new(temporary.path(), &mut child)?;
    let captured = match capture::run(
        &mut child,
        capture::Limits {
            wall: Duration::from_secs(60),
            idle: Duration::from_secs(30),
            bytes: 262144,
            cleanup: Duration::from_secs(5),
        },
    ) {
        Ok(output) => Ok(output),
        Err(mut error) => {
            if error.settle(Duration::from_secs(5)).is_err() {
                eprintln!(
                    "MCP output leader unresolved; fixture retained at {}",
                    temporary.path().display()
                );
                std::mem::forget(units);
                std::mem::forget(temporary);
                return Err(error.into());
            }
            Err(error)
        }
    };
    // Inner captures create separate POSIX groups; retain their receipts on uncertainty.
    if let Err(error) = units.settle(Duration::from_secs(5)) {
        let path = temporary.path().display().to_string();
        std::mem::forget(temporary);
        return Err(format!(
            "MCP output ownership unresolved; fixture retained at {path}: {error}"
        )
        .into());
    }
    let result = captured?;
    let transcript = String::from_utf8(result.stdout)?;
    let diagnostics = String::from_utf8(result.stderr)?;
    assert_eq!(
        result.exit, 0,
        "MCP output child failed\n{transcript}\n{diagnostics}"
    );
    assert!(
        transcript.contains("1 passed; 0 failed"),
        "MCP output child did not run"
    );
    let results: Vec<Value> = transcript
        .lines()
        .filter_map(|line| line.strip_prefix("MCP output result "))
        .map(serde_json::from_str)
        .collect::<Result<_, _>>()?;
    assert_eq!(
        results.len(),
        rows.len(),
        "MCP output child observations missing"
    );
    for (index, (actual, row)) in results.iter().zip(&rows).enumerate() {
        assert_eq!(
            actual["index"],
            json!(index),
            "MCP output child order differs"
        );
        assert_eq!(
            actual["actual"], row["expected"],
            "MCP captured result differs: {index}"
        );
    }
    temporary.cleanup()?;
    Ok(())
}

#[test]
fn native_mcp_preserves_original_messages_verdicts_and_bounded_framing()
-> Result<(), Box<dyn std::error::Error>> {
    let root = common::TempDirectory::new("mcp-native")?;
    let document = http_fixture::oracle("mcp")?;
    let cases = document["messages"]
        .as_array()
        .ok_or("MCP source cases missing")?;
    assert_eq!(cases.len(), 9);
    for case in cases {
        let mut called = false;
        let input = mcp::encode(&case["message"])?;
        let mut output = Vec::new();
        mcp::serve(&mut std::io::Cursor::new(input), &mut output, |message| {
            mcp::handle(message, |argv| {
                called = true;
                assert_eq!(argv.last().map(String::as_str), Some("--json"));
                Ok(Outcome {
                    payload: case["payload"].clone(),
                    exit: case["exit"].as_i64().expect("Source exit") as i32,
                })
            })
        })?;
        let actual = mcp::decode(&output)?;
        assert_eq!(actual.clone().unwrap_or(Value::Null), case["expected"]);
        assert_eq!(
            called,
            matches!(case["message"]["id"].as_u64(), Some(5 | 6))
        );
        if let Some(response) = actual {
            let bytes = mcp::encode(&response)?;
            assert_eq!(bytes.last(), Some(&b'\n'));
            assert_eq!(bytes.iter().filter(|b| **b == b'\n').count(), 1);
            assert_eq!(mcp::decode(&bytes)?, Some(response));
        }
    }
    let tools = mcp::handle(&json!({"id":10,"method":"tools/list"}), |_| {
        panic!("Listing executed a command")
    })?
    .ok_or("Tools response missing")?;
    assert_eq!(
        tools["result"]["tools"],
        json!(numera_core::cli::reflection::tools())
    );
    let native = root.path().join("native");
    let repo = numera_core::storage::repo::ProfileRepo::open(native.clone(), None, None)?;
    let sessions = numera_core::ownership::registry::Registry::new(
        native.clone(),
        "fixture-host".into(),
        None,
    )?;
    let installed = std::collections::BTreeMap::new();
    let application = numera_core::cli::application::Application {
        root: &native,
        home: &native,
        repo: &repo,
        installed: &installed,
        host: "fixture-host",
        password: None,
        environment_token: false,
        sessions: &sessions,
        browser: None,
    };
    for case in document["application"]
        .as_array()
        .ok_or("Native application MCP cases missing")?
    {
        let actual = mcp::handle(&case["message"], |argv| {
            mcp::execute_native(&application, argv, 1000.0)
        })?
        .ok_or("Application MCP response missing")?;
        assert_eq!(actual, case["expected"]);
    }
    root.cleanup()?;
    assert!(mcp::decode(b"not JSON\n")?.is_none());
    assert!(mcp::decode(b" \n")?.is_none());
    assert!(mcp::decode(&vec![b' '; mcp::MAX_FRAME + 1]).is_err());
    assert!(mcp::decode(&[0xff]).is_err());
    assert!(mcp::encode(&json!({"text":"x".repeat(mcp::MAX_FRAME)})).is_err());
    Ok(())
}
