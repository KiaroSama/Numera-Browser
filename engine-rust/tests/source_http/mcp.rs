use super::*;

fn call(root: &Path, name: &str, arguments: Value) -> Value {
    numera_core::mcp_command::call(
        Path::new(env!("CARGO_BIN_EXE_numera-engine")),
        Some(root),
        None,
        name,
        arguments.as_object().expect("tool arguments"),
    )
    .expect("owned native MCP tool")
}
fn decoded(value: &Value) -> Value {
    serde_json::from_str(value["content"][0]["text"].as_str().expect("MCP text"))
        .expect("complete MCP payload")
}
fn frames(bytes: &[u8]) -> Vec<Value> {
    let mut output = Vec::new();
    numera_core::mcp::serve::<io::Error>(
        &mut std::io::Cursor::new(bytes),
        &mut output,
        |message| {
            numera_core::mcp::handle(message, |_| {
                panic!("protocol frame unexpectedly executed command")
            })
        },
    )
    .expect("MCP stream");
    output
        .split(|b| *b == b'\n')
        .filter(|row| !row.is_empty())
        .map(|row| serde_json::from_slice(row).expect("MCP frame JSON"))
        .collect()
}
source_test!(
    mcp_test_the_tool_list_covers_the_cli_and_nothing_it_should_not,
    "tests/test_mcp.py::test_the_tool_list_covers_the_cli_and_nothing_it_should_not",
    {
        let tools = numera_core::cli::reflection::tools();
        let names = tools
            .iter()
            .map(|t| text(t, "name"))
            .collect::<std::collections::BTreeSet<_>>();
        for required in [
            "create",
            "list",
            "launch",
            "stop",
            "trust",
            "health",
            "cookies_export",
            "pool_assign",
            "schedule_update",
            "jobs_show",
            "bulk_create",
        ] {
            assert!(names.contains(required), "{required}");
        }
        assert!(!names.contains("serve"));
        assert!(!names.contains("mcp"));
        assert!(names.len() >= 40);
        Ok(())
    }
);
source_test!(
    mcp_test_every_schema_is_well_formed_and_matches_its_command,
    "tests/test_mcp.py::test_every_schema_is_well_formed_and_matches_its_command",
    {
        for tool in numera_core::cli::reflection::tools() {
            let schema = &tool["inputSchema"];
            assert_eq!(schema["type"], "object");
            assert!(!text(&tool, "description").is_empty());
            let properties = schema["properties"].as_object().expect("properties");
            for required in schema["required"].as_array().expect("required") {
                assert!(properties.contains_key(required.as_str().expect("required name")));
            }
            for spec in properties.values() {
                assert!(
                    ["string", "boolean", "integer", "number", "array"]
                        .contains(&spec["type"].as_str().expect("type"))
                );
            }
            assert_eq!(properties["detach"]["type"], "boolean");
            assert!(!properties.contains_key("json"));
        }
        Ok(())
    }
);
source_test!(
    mcp_test_required_arguments_are_the_positional_ones,
    "tests/test_mcp.py::test_required_arguments_are_the_positional_ones",
    {
        let by_name = numera_core::cli::reflection::tools()
            .into_iter()
            .map(|t| (text(&t, "name"), t))
            .collect::<BTreeMap<_, _>>();
        assert_eq!(
            by_name["create"]["inputSchema"]["required"],
            json!(["name"])
        );
        assert_eq!(by_name["show"]["inputSchema"]["required"], json!(["ref"]));
        assert_eq!(
            by_name["cookies_export"]["inputSchema"]["required"],
            json!(["ref", "file"])
        );
        assert_eq!(
            by_name["default_engine"]["inputSchema"]["required"],
            json!([])
        );
        let props = &by_name["bulk_create"]["inputSchema"]["properties"];
        assert_eq!(props["count"]["type"], "integer");
        assert_eq!(props["tag"]["type"], "array");
        assert_eq!(props["mobile"]["type"], "boolean");
        assert_eq!(
            props["os"]["enum"],
            json!(["windows", "macos", "linux", "android"])
        );
        Ok(())
    }
);
source_test!(
    mcp_test_a_tool_call_runs_the_command_and_returns_its_payload,
    "tests/test_mcp.py::test_a_tool_call_runs_the_command_and_returns_its_payload",
    {
        let t = fresh();
        let made = call(
            t.path(),
            "create",
            json!({"name":"alpha","os":"linux","tag":["t1","t2"]}),
        );
        assert_eq!(made["isError"], false);
        let payload = decoded(&made);
        assert_eq!(payload["ok"], true);
        assert_eq!(payload["profile"]["fingerprint"]["os"], "linux");
        assert_eq!(payload["profile"]["tags"], json!(["t1", "t2"]));
        assert_eq!(made["structuredContent"], payload);
        let listed = decoded(&call(t.path(), "list", json!({})));
        assert_eq!(listed["profiles"][0]["name"], "alpha");
        assert_eq!(listed["profiles"].as_array().expect("profiles").len(), 1);
        Ok(())
    }
);
source_test!(
    mcp_test_a_verdict_is_a_result_and_a_broken_call_is_an_error,
    "tests/test_mcp.py::test_a_verdict_is_a_result_and_a_broken_call_is_an_error",
    {
        let t = fresh();
        call(t.path(), "create", json!({"name":"alpha"}));
        let verdict = call(t.path(), "check", json!({"ref":"alpha"}));
        assert_eq!(verdict["isError"], false);
        assert!(verdict["_meta"].get("exitCode").is_some());
        let missing = call(t.path(), "show", json!({"ref":"nope"}));
        assert_eq!(missing["isError"], false);
        assert_eq!(decoded(&missing)["ok"], false);
        assert_eq!(call(t.path(), "no_such_tool", json!({}))["isError"], true);
        Ok(())
    }
);
source_test!(
    mcp_test_detach_comes_back_as_a_job_handle,
    "tests/test_mcp.py::test_detach_comes_back_as_a_job_handle",
    {
        let t = fresh();
        let handle = decoded(&call(
            t.path(),
            "bulk_create",
            json!({"count":5,"prefix":"Job","detach":true}),
        ));
        assert_eq!(handle["command"], "bulk-create");
        assert!(handle["pid"].as_u64().is_some_and(|pid| pid > 0));
        let shown = decoded(&call(t.path(), "jobs_show", json!({"id":handle["id"]})));
        assert_eq!(shown["id"], handle["id"]);
        assert!(
            ["starting", "running", "done", "failed"]
                .contains(&shown["state"].as_str().expect("state"))
        );
        call(t.path(), "jobs_stop", json!({"id":handle["id"]}));
        Ok(())
    }
);
source_test!(
    mcp_test_the_handshake_a_client_actually_performs,
    "tests/test_mcp.py::test_the_handshake_a_client_actually_performs",
    {
        let requests = [
            json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"test","version":"1"}}}),
            json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
            json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
            json!({"jsonrpc":"2.0","id":3,"method":"ping"}),
            json!({"jsonrpc":"2.0","id":4,"method":"nonsense/method"}),
        ];
        let input = requests
            .iter()
            .map(|v| {
                v.to_string()
                    + "
"
            })
            .collect::<String>();
        let replies = frames(input.as_bytes());
        assert_eq!(
            replies.iter().map(|r| r["id"].clone()).collect::<Vec<_>>(),
            vec![json!(1), json!(2), json!(3), json!(4)]
        );
        assert_eq!(replies[0]["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(replies[0]["result"]["serverInfo"]["name"], "numera");
        assert!(replies[0]["result"]["capabilities"].get("tools").is_some());
        assert!(
            replies[1]["result"]["tools"]
                .as_array()
                .expect("tools")
                .len()
                >= 40
        );
        assert_eq!(replies[2]["result"], json!({}));
        assert_eq!(replies[3]["error"]["code"], -32601);
        Ok(())
    }
);
source_test!(
    mcp_test_a_malformed_frame_does_not_end_the_session,
    "tests/test_mcp.py::test_a_malformed_frame_does_not_end_the_session",
    {
        let mut input = b"{not json\n".to_vec();
        input.extend_from_slice(
            (json!({"jsonrpc":"2.0","id":9,"method":"ping"}).to_string() + "\n").as_bytes(),
        );
        let replies = frames(&input);
        assert_eq!(replies.len(), 1);
        assert_eq!(replies[0]["id"], 9);
        Ok(())
    }
);
source_test!(
    mcp_test_it_runs_as_a_real_subprocess_over_stdio,
    "tests/test_mcp.py::test_it_runs_as_a_real_subprocess_over_stdio",
    {
        use std::{io::Write, process::Stdio};
        let t = fresh();
        let mut process = Command::new(env!("CARGO_BIN_EXE_numera-engine"))
            .arg("--data-dir")
            .arg(t.path())
            .arg("mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        process.stdin.take().expect("MCP input").write_all(
            (json!({"jsonrpc":"2.0","id":1,"method":"tools/list"}).to_string() + "\n").as_bytes(),
        )?;
        let output = process.wait_with_output()?;
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let reply = payload(&output);
        assert!(
            reply["result"]["tools"]
                .as_array()
                .expect("tools")
                .iter()
                .any(|t| t["name"] == "create")
        );
        Ok(())
    }
);
