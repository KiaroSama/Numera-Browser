use numera_core::{browser::adapter::Adapter, cli::profile_commands::Outcome, mcp};
use serde_json::{Value, json};
use std::{path::Path, process::Command, time::Duration};
mod common;

#[test]
fn stdio_text_boundaries_match_original_responses() -> Result<(), Box<dyn std::error::Error>> {
    use numera_core::ownership::capture;
    use std::io::{BufReader, Cursor};

    let temporary = common::TempDirectory::new("mcp-text")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(
        std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Original interpreter required")?,
    );
    command.args(["-I", "-B", "-c", r#"
import io,json,sys
from pathlib import Path
repository,root=map(Path,sys.argv[1:])
sys.path.insert(0,str(repository/'scripts/migration'))
from baseline_source import activate
activate()
from numera import mcp
from run_logging import run_log
whitespace=[9,10,11,12,13,28,29,30,31,32,133,160,5760,8192,8193,8194,8195,8196,8197,8198,8199,8200,8201,8202,8232,8233,8239,8287,12288]
ping='{"id":1,"method":"ping"}'
bodies=[chr(code)+value+chr(code)+ending for code in whitespace
        for value,ending in (('','\n'),(ping,'\n'),(ping,''))]
bodies += ['', 'not JSON\n', '{"id":90,"method":"ping"}\r\n',
           ' \u001c{"id":91,"method":"ping"}\u001f\nnot JSON\n{"id":92,"method":"ping"}',
           '{"method":"notifications/initialized"}\n{"id":93,"method":"ping"}\n',
           '{"id":null,"method":"unknown"}\n', '{"id":false,"method":"unknown"}\n',
           '{"id":94,"method":"initialize","params":{"protocolVersion":"فارسی😀\\u001cinside"}}\n',
           '{"id":95,"method":"ping","unused":"\\u001c\\u001f"}\n',
           '\x00'+ping+'\x00\n', chr(0x200b)+ping+chr(0x200b)+'\n',
           chr(0xfeff)+ping+chr(0xfeff)+'\n']
rows=[]
with run_log('mcp-text-oracle',root/'logs') as log:
    for body in bodies:
        output=io.StringIO()
        assert mcp.serve(stdin=io.StringIO(body),stdout=output)==0
        rows.append({'body':body,'responses':[json.loads(line) for line in output.getvalue().splitlines()]})
    log.info('Observed MCP stdio cases=%s',len(rows))
    print(json.dumps(rows,ensure_ascii=True))
"#]).arg(repository).arg(temporary.path()).env("PYTHONUTF8", "1");
    let observed = capture::run(
        &mut command,
        capture::Limits {
            wall: Duration::from_secs(30),
            idle: Duration::from_secs(20),
            bytes: 262144,
            cleanup: Duration::from_secs(5),
        },
    )?;
    assert_eq!(observed.exit, 0, "Original MCP text observer failed");
    let rows: Vec<Value> = serde_json::from_slice(&observed.stdout)?;
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
fn native_mcp_preserves_original_messages_verdicts_and_bounded_framing()
-> Result<(), Box<dyn std::error::Error>> {
    let python = std::env::var_os("NUMERA_ORACLE_PYTHON").ok_or("Fixture interpreter required")?;
    let root = common::TempDirectory::new("mcp-source")?;
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or("Repository missing")?;
    let mut command = Command::new(python);
    command
        .args([
            "-u",
            "-c",
            r#"
import json,os,sys
request=json.loads(sys.stdin.readline())
print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':{}}),flush=True)
request=json.loads(sys.stdin.readline())
from mcp_cases import cases
print(json.dumps({'protocol':1,'id':request['id'],'ok':True,'result':cases(os.environ['FIXTURE_ROOT'])}),flush=True)
"#,
        ])
        .env("PYTHONUTF8", "1")
        .env("FIXTURE_ROOT",root.path().join("source"))
        .env(
            "PYTHONPATH",
            std::env::join_paths([
                repository.join("scripts/migration"),
                repository.join("engine"),
            ])?,
        );
    let mut source = Adapter::spawn(
        root.path(),
        "a",
        "fixture-host",
        &mut command,
        Duration::from_secs(5),
    )?;
    let document = source.call("observe-mcp", Default::default(), Duration::from_secs(30))?;
    source.finish(Duration::from_secs(5))?;
    drop(source);
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
