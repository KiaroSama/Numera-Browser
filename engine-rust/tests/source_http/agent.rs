use super::*;

struct Capability {
    name: &'static str,
    method: &'static str,
    path: &'static str,
    commands: &'static [&'static str],
    transport: bool,
}
fn capabilities() -> Vec<Capability> {
    vec![
        Capability {
            name: "health",
            method: "GET",
            path: "/api/health",
            commands: &[],
            transport: true,
        },
        Capability {
            name: "engines",
            method: "GET",
            path: "/api/engines",
            commands: &["engines"],
            transport: false,
        },
        Capability {
            name: "cloakOffer",
            method: "GET",
            path: "/api/engines/cloak/acquisition",
            commands: &["engines"],
            transport: false,
        },
        Capability {
            name: "acquireCloak",
            method: "POST",
            path: "/api/engines/cloak/acquisition",
            commands: &["engines"],
            transport: false,
        },
        Capability {
            name: "getConfig",
            method: "GET",
            path: "/api/config",
            commands: &["config"],
            transport: false,
        },
        Capability {
            name: "setDefaultEngine",
            method: "PUT",
            path: "/api/config",
            commands: &["default_engine"],
            transport: false,
        },
        Capability {
            name: "setDefaultExtensions",
            method: "PUT",
            path: "/api/config",
            commands: &["config"],
            transport: false,
        },
        Capability {
            name: "installExtension",
            method: "POST",
            path: "/api/extensions/install",
            commands: &["ext_install"],
            transport: false,
        },
        Capability {
            name: "installedExtensions",
            method: "GET",
            path: "/api/extensions/installed",
            commands: &["ext_installed"],
            transport: false,
        },
        Capability {
            name: "extensionIcon",
            method: "GET",
            path: "/api/extensions/icon",
            commands: &[],
            transport: true,
        },
        Capability {
            name: "createFolder",
            method: "POST",
            path: "/api/folders",
            commands: &["folder_create"],
            transport: false,
        },
        Capability {
            name: "renameFolder",
            method: "POST",
            path: "/api/folders/rename",
            commands: &["folder_rename"],
            transport: false,
        },
        Capability {
            name: "deleteFolder",
            method: "POST",
            path: "/api/folders/delete",
            commands: &["folder_delete"],
            transport: false,
        },
        Capability {
            name: "list",
            method: "GET",
            path: "/api/profiles",
            commands: &["list"],
            transport: false,
        },
        Capability {
            name: "create",
            method: "POST",
            path: "/api/profiles",
            commands: &["create"],
            transport: false,
        },
        Capability {
            name: "update",
            method: "PUT",
            path: "/api/profiles/${p.id}",
            commands: &["edit", "apply", "regen", "export", "import"],
            transport: false,
        },
        Capability {
            name: "remove",
            method: "DELETE",
            path: "/api/profiles/${id}",
            commands: &["delete"],
            transport: false,
        },
        Capability {
            name: "removeMany",
            method: "POST",
            path: "/api/profiles/delete",
            commands: &["delete"],
            transport: false,
        },
        Capability {
            name: "launch",
            method: "POST",
            path: "/api/profiles/${id}/launch",
            commands: &["launch"],
            transport: false,
        },
        Capability {
            name: "stop",
            method: "POST",
            path: "/api/profiles/${id}/stop",
            commands: &["stop"],
            transport: false,
        },
        Capability {
            name: "exportCookies",
            method: "GET",
            path: "/api/profiles/${id}/cookies",
            commands: &["cookies_export"],
            transport: false,
        },
        Capability {
            name: "importCookies",
            method: "POST",
            path: "/api/profiles/${id}/cookies",
            commands: &["cookies_import"],
            transport: false,
        },
        Capability {
            name: "proxyTest",
            method: "POST",
            path: "/api/profiles/${id}/proxy-test",
            commands: &["proxy_test"],
            transport: false,
        },
        Capability {
            name: "trust",
            method: "POST",
            path: "/api/profiles/${id}/trust",
            commands: &["trust"],
            transport: false,
        },
        Capability {
            name: "tls",
            method: "POST",
            path: "/api/profiles/${id}/tls",
            commands: &["tls"],
            transport: false,
        },
        Capability {
            name: "dns",
            method: "POST",
            path: "/api/profiles/${id}/dns",
            commands: &["dns"],
            transport: false,
        },
        Capability {
            name: "align",
            method: "POST",
            path: "/api/profiles/${id}/align",
            commands: &["align"],
            transport: false,
        },
        Capability {
            name: "attach",
            method: "POST",
            path: "/api/profiles/${id}/attach",
            commands: &["attach"],
            transport: false,
        },
        Capability {
            name: "warmup",
            method: "POST",
            path: "/api/profiles/${id}/warmup",
            commands: &["warmup"],
            transport: false,
        },
        Capability {
            name: "bulkUpdate",
            method: "POST",
            path: "/api/profiles/bulk",
            commands: &["apply"],
            transport: false,
        },
        Capability {
            name: "batchCreate",
            method: "POST",
            path: "/api/profiles/batch",
            commands: &["bulk_create"],
            transport: false,
        },
        Capability {
            name: "importProfiles",
            method: "POST",
            path: "/api/profiles/import",
            commands: &["import_from"],
            transport: false,
        },
        Capability {
            name: "listProxies",
            method: "GET",
            path: "/api/proxies",
            commands: &["pool_list"],
            transport: false,
        },
        Capability {
            name: "addProxies",
            method: "POST",
            path: "/api/proxies",
            commands: &["pool_add"],
            transport: false,
        },
        Capability {
            name: "deleteProxy",
            method: "DELETE",
            path: "/api/proxies/${id}",
            commands: &["pool_remove"],
            transport: false,
        },
        Capability {
            name: "deleteProxies",
            method: "POST",
            path: "/api/proxies/delete",
            commands: &["pool_remove"],
            transport: false,
        },
        Capability {
            name: "testProxy",
            method: "POST",
            path: "/api/proxies/${id}/test",
            commands: &["pool_test"],
            transport: false,
        },
        Capability {
            name: "testProxies",
            method: "POST",
            path: "/api/proxies/test",
            commands: &["pool_test"],
            transport: false,
        },
        Capability {
            name: "updateProxy",
            method: "PUT",
            path: "/api/proxies/${id}",
            commands: &["pool_edit"],
            transport: false,
        },
        Capability {
            name: "exportProxies",
            method: "POST",
            path: "/api/proxies/export",
            commands: &["pool_export"],
            transport: false,
        },
        Capability {
            name: "rotateProxy",
            method: "POST",
            path: "/api/proxies/${id}/rotate",
            commands: &["pool_rotate"],
            transport: false,
        },
        Capability {
            name: "rotateProxies",
            method: "POST",
            path: "/api/proxies/rotate",
            commands: &["pool_rotate"],
            transport: false,
        },
        Capability {
            name: "archiveProxies",
            method: "POST",
            path: "/api/proxies/archive",
            commands: &["pool_archive"],
            transport: false,
        },
        Capability {
            name: "listTokens",
            method: "GET",
            path: "/api/tokens",
            commands: &["token_list"],
            transport: false,
        },
        Capability {
            name: "createToken",
            method: "POST",
            path: "/api/tokens",
            commands: &["token_create"],
            transport: false,
        },
        Capability {
            name: "revokeToken",
            method: "DELETE",
            path: "/api/tokens/${id}",
            commands: &["token_revoke"],
            transport: false,
        },
        Capability {
            name: "assignProxies",
            method: "POST",
            path: "/api/proxies/assign",
            commands: &["pool_assign"],
            transport: false,
        },
        Capability {
            name: "listSchedules",
            method: "GET",
            path: "/api/schedules",
            commands: &["schedule_list"],
            transport: false,
        },
        Capability {
            name: "addSchedule",
            method: "POST",
            path: "/api/schedules",
            commands: &["schedule_add"],
            transport: false,
        },
        Capability {
            name: "updateSchedule",
            method: "PUT",
            path: "/api/schedules/${id}",
            commands: &["schedule_update"],
            transport: false,
        },
        Capability {
            name: "deleteSchedule",
            method: "DELETE",
            path: "/api/schedules/${id}",
            commands: &["schedule_remove"],
            transport: false,
        },
        Capability {
            name: "profileHealth",
            method: "GET",
            path: "/api/profiles/health",
            commands: &["health"],
            transport: false,
        },
        Capability {
            name: "collisions",
            method: "GET",
            path: "/api/profiles/collisions",
            commands: &["collisions"],
            transport: false,
        },
    ]
}
fn tools() -> std::collections::BTreeSet<String> {
    numera_core::cli::reflection::tools()
        .iter()
        .map(|t| text(t, "name"))
        .collect()
}
fn native_routes() -> std::collections::BTreeSet<String> {
    let source = include_str!("../../../dashboard/rust/browser.rs");
    let pattern = regex::Regex::new(r#""(/api/[^"\\]*)""#).expect("native request inventory");
    pattern
        .captures_iter(source)
        .map(|c| shape(&c[1]))
        .collect()
}
fn shape(path: &str) -> String {
    regex::Regex::new(r"\$\{[^}]+\}|\{[^}]*\}")
        .expect("route shape")
        .replace_all(path.split('?').next().expect("path"), "{}")
        .into_owned()
}
fn compatible(left: &str, right: &str) -> bool {
    let a = left.split('/').collect::<Vec<_>>();
    let b = right.split('/').collect::<Vec<_>>();
    a.len() == b.len()
        && a.iter()
            .zip(b)
            .all(|(a, b)| *a == "{}" || b == "{}" || *a == b)
}
fn offline(root: &Path) -> Vec<Vec<String>> {
    let extension = root.join("ext");
    fs::create_dir(&extension).expect("extension");
    fs::write(
        extension.join("manifest.json"),
        r#"{"manifest_version":3,"name":"fixture","version":"1.0"}"#,
    )
    .expect("extension manifest");
    let exported = root.join("p.json").to_string_lossy().into_owned();
    let backup = root.join("p.zip").to_string_lossy().into_owned();
    let proxies = root.join("proxies.txt");
    fs::write(&proxies, "http://u:p@1.2.3.4:8080:US\n5.6.7.8:3128\n").expect("proxy input");
    let proxy_path = proxies.to_string_lossy().into_owned();
    let ext = extension.to_string_lossy().into_owned();
    let invocations = vec![
        vec!["create", "alpha", "--tag", "t1"],
        vec!["create", "beta", "--os", "macos"],
        vec!["list"],
        vec!["list", "--tag", "t1"],
        vec!["show", "alpha"],
        vec!["show", "nope"],
        vec!["check", "alpha"],
        vec!["health"],
        vec!["collisions"],
        vec!["engines"],
        vec!["default-engine"],
        vec!["config"],
        vec!["config", "--headless-launch", "on"],
        vec!["edit", "alpha", "--notes", "hello", "--add-tag", "t2"],
        vec!["regen", "alpha", "--os", "linux"],
        vec!["export", "alpha", &exported],
        vec!["import", &exported],
        vec!["backup", "alpha", &backup],
        vec!["restore", &backup, "--new-id"],
        vec!["bulk-create", "--count", "3", "--prefix", "Bulk"],
        vec!["apply", "--all", "--notes", "bulk note", "-y"],
        vec!["ext", "list", "alpha"],
        vec!["ext", "add", "alpha", &ext],
        vec!["ext", "remove", "alpha", &ext],
        vec!["ext", "default-list"],
        vec!["ext", "default-add", &ext],
        vec!["ext", "default-remove", &ext],
        vec!["pool", "add", &proxy_path],
        vec!["pool", "list"],
        vec!["schedule", "run-due"],
        vec!["schedule", "add", "alpha", "--every", "6"],
        vec!["schedule", "list"],
        vec!["jobs", "list"],
        vec!["jobs", "clear"],
        vec!["stop", "alpha"],
        vec!["vault", "status"],
        vec!["import-from", &exported],
        vec!["delete", "beta", "-y"],
    ];
    invocations
        .into_iter()
        .map(|v| v.into_iter().map(str::to_owned).collect())
        .collect()
}
source_test!(
    agent_parity_test_the_readers_actually_read_something,
    "tests/test_agent_parity.py::test_the_readers_actually_read_something",
    {
        let routes = native_routes();
        assert!(
            routes.len() >= 30,
            "Native browser request reader found {routes:?}"
        );
        assert!(routes.contains("/api/profiles"));
        assert!(routes.iter().any(|r| r.contains("/warmup")));
        let commands = tools();
        assert!(commands.len() >= 30);
        for name in ["create", "cookies_export", "pool_assign"] {
            assert!(commands.contains(name));
        }
        Ok(())
    }
);
source_test!(
    agent_parity_test_no_unmapped_dashboard_capabilities,
    "tests/test_agent_parity.py::test_no_unmapped_dashboard_capabilities",
    {
        let declared = capabilities();
        for route in native_routes() {
            assert!(
                declared
                    .iter()
                    .filter(|c| !c.path.is_empty())
                    .any(|c| compatible(&route, &shape(c.path))),
                "Native dashboard added an unmapped capability: {route}"
            );
        }
        let names = declared
            .iter()
            .map(|c| c.name)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(names.len(), declared.len());
        Ok(())
    }
);
source_test!(
    agent_parity_test_no_api_member_escapes_a_verdict,
    "tests/test_agent_parity.py::test_no_api_member_escapes_a_verdict",
    {
        for cap in capabilities() {
            assert!(
                cap.transport || !cap.commands.is_empty(),
                "No headless verdict for {}",
                cap.name
            );
        }
        for route in native_routes() {
            assert!(
                capabilities()
                    .iter()
                    .any(|c| !c.path.is_empty() && compatible(&route, &shape(c.path))),
                "Unaccounted native transport {route}"
            );
        }
        Ok(())
    }
);
source_test!(
    agent_parity_test_every_mapped_cli_command_exists,
    "tests/test_agent_parity.py::test_every_mapped_cli_command_exists",
    {
        let commands = tools();
        for cap in capabilities() {
            for command in cap.commands {
                assert!(
                    commands.contains(*command),
                    "{} maps to missing native {command}",
                    cap.name
                );
            }
        }
        Ok(())
    }
);
source_test!(
    agent_parity_test_known_gaps_have_not_drifted,
    "tests/test_agent_parity.py::test_known_gaps_have_not_drifted",
    {
        let commands = tools();
        let gaps = capabilities()
            .into_iter()
            .filter(|cap| !cap.transport && cap.commands.iter().all(|c| !commands.contains(*c)))
            .map(|c| c.name)
            .collect::<Vec<_>>();
        assert!(gaps.is_empty(), "Headless gap set changed: {gaps:?}");
        Ok(())
    }
);
source_test!(
    agent_parity_test_a_gap_is_really_a_gap,
    "tests/test_agent_parity.py::test_a_gap_is_really_a_gap",
    {
        let commands = tools();
        for cap in capabilities().into_iter().filter(|c| !c.transport) {
            assert!(
                cap.commands.iter().any(|c| commands.contains(*c)),
                "Claimed mapping for {} has no native command",
                cap.name
            );
        }
        Ok(())
    }
);
source_test!(
    agent_parity_test_dashboard_routes_all_exist_on_the_api,
    "tests/test_agent_parity.py::test_dashboard_routes_all_exist_on_the_api",
    {
        let t = fresh();
        let root = t.path();
        let h = Http::new(root);
        let unknown = h.get("/api/definitely-not-a-real-endpoint", None);
        let parameters = regex::Regex::new(r"\$\{[^}]+\}|\{[^}]*\}")?;
        for cap in capabilities().into_iter().filter(|c| !c.method.is_empty()) {
            let path = parameters
                .replace_all(cap.path, "missing-record")
                .into_owned();
            let r = h.request(cap.method, &path, json!({}), None);
            assert!(
                r.status != 405,
                "{} uses unregistered method {} {path}",
                cap.name,
                cap.method
            );
            assert!(
                !(r.status == unknown.status && r.body == unknown.body),
                "{} calls unknown native route {} {path}",
                cap.name,
                cap.method
            );
        }
        Ok(())
    }
);
source_test!(
    agent_parity_test_every_offline_command_emits_only_json,
    "tests/test_agent_parity.py::test_every_offline_command_emits_only_json",
    {
        let t = fresh();
        for arguments in offline(t.path()) {
            let args = arguments.iter().map(String::as_str).collect::<Vec<_>>();
            let output = cli(t.path(), &args, true);
            assert!(
                !output.stdout.windows(2).any(|b| b == b"\x1b["),
                "Color leaked from {arguments:?}"
            );
            let value = payload(&output);
            assert!(value.is_object() || value.is_array(), "{arguments:?}");
            assert!(output.status.code().is_some());
        }
        Ok(())
    }
);
source_test!(
    agent_parity_test_the_commands_keyed_by_a_generated_id_emit_json_too,
    "tests/test_agent_parity.py::test_the_commands_keyed_by_a_generated_id_emit_json_too",
    {
        let t = fresh();
        let root = t.path();
        let input = root.join("px.txt");
        fs::write(
            &input,
            "http://1.2.3.4:8080:US
",
        )?;
        let added = payload(&cli(root, &["pool", "add", &input.to_string_lossy()], true));
        let proxy_id = text(&added["proxies"][0], "id");
        payload(&cli(root, &["create", "alpha"], true));
        let scheduled = payload(&cli(
            root,
            &["schedule", "add", "alpha", "--every", "6"],
            true,
        ));
        let sid = text(&scheduled, "id");
        for args in [
            vec!["pool", "assign", "--all"],
            vec!["pool", "remove", &proxy_id],
            vec!["schedule", "update", &sid, "--minutes", "9"],
            vec!["schedule", "remove", &sid],
            vec!["jobs", "show", "no-such-job"],
            vec!["jobs", "stop", "no-such-job"],
        ] {
            assert!(payload(&cli(root, &args, true)).is_object());
        }
        Ok(())
    }
);
source_test!(
    agent_parity_test_pool_assign_actually_rewrites_the_profile,
    "tests/test_agent_parity.py::test_pool_assign_actually_rewrites_the_profile",
    {
        let t = fresh();
        let root = t.path();
        let input = root.join("px.txt");
        fs::write(
            &input,
            "http://user:pw@9.9.9.9:8080:DE
",
        )?;
        payload(&cli(root, &["pool", "add", &input.to_string_lossy()], true));
        payload(&cli(root, &["create", "alpha", "--locale", "en-US"], true));
        assert_eq!(
            payload(&cli(root, &["pool", "assign", "--all"], true))["assigned"],
            1
        );
        let p = payload(&cli(root, &["show", "alpha"], true));
        assert_eq!(p["proxy"]["server"], "http://9.9.9.9:8080");
        assert_eq!(p["proxy"]["username"], "user");
        assert_eq!(p["fingerprint"]["locale"], "de-DE");
        assert!(text(&p["fingerprint"], "timezone").starts_with("Europe/"));
        Ok(())
    }
);
source_test!(
    agent_parity_test_human_output_is_still_the_default,
    "tests/test_agent_parity.py::test_human_output_is_still_the_default",
    {
        let t = fresh();
        let root = t.path();
        cli(root, &["create", "alpha"], false);
        for args in [
            vec!["list"],
            vec!["show", "alpha"],
            vec!["health"],
            vec!["config"],
        ] {
            let output = cli(root, &args, false);
            assert!(!output.stdout.is_empty());
            assert!(
                serde_json::from_slice::<Value>(&output.stdout).is_err(),
                "Human {args:?} became JSON"
            );
        }
        Ok(())
    }
);
source_test!(
    agent_parity_test_json_mode_never_waits_for_a_keyboard,
    "tests/test_agent_parity.py::test_json_mode_never_waits_for_a_keyboard",
    {
        let t = fresh();
        let root = t.path();
        payload(&cli(root, &["create", "alpha"], true));
        let output = cli(root, &["delete", "alpha"], true);
        assert_eq!(output.status.code(), Some(1));
        assert!(text(&payload(&output), "error").contains("confirmation"));
        assert_eq!(payload(&cli(root, &["list"], true))["total"], 1);
        Ok(())
    }
);
source_test!(
    agent_parity_test_export_edit_import_is_a_real_full_profile_edit,
    "tests/test_agent_parity.py::test_export_edit_import_is_a_real_full_profile_edit",
    {
        let t = fresh();
        let root = t.path();
        let made = payload(&cli(root, &["create", "alpha"], true));
        let id = text(&made["profile"], "id");
        let exported = root.join("alpha.json");
        payload(&cli(
            root,
            &["export", "alpha", &exported.to_string_lossy()],
            true,
        ));
        let mut record = read(&exported);
        record["fingerprint"]["hardware_concurrency"] = json!(24);
        fs::write(&exported, serde_json::to_vec(&record)?)?;
        payload(&cli(root, &["import", &exported.to_string_lossy()], true));
        let after = payload(&cli(root, &["show", "alpha"], true));
        assert_eq!(after["id"], id);
        assert_eq!(after["fingerprint"]["hardware_concurrency"], 24);
        Ok(())
    }
);
source_test!(
    agent_parity_test_the_installed_entry_point_agrees_with_the_in_process_one,
    "tests/test_agent_parity.py::test_the_installed_entry_point_agrees_with_the_in_process_one",
    {
        let t = fresh();
        let root = t.path();
        let output = cli(root, &["engines"], true);
        assert!(output.status.success());
        let external = payload(&output);
        assert!(external.get("default").is_some());
        let installed =
            serde_json::from_value::<BTreeMap<String, bool>>(external["engines"].clone())?;
        let repo = ProfileRepo::new(root.to_owned(), None);
        let settings = numera_core::cli::settings_commands::Settings {
            root,
            repo: &repo,
            installed: &installed,
            environment_token: false,
        };
        let direct = settings.engines()?;
        assert_eq!(external, direct.payload);
        Ok(())
    }
);
source_test!(
    agent_parity_test_the_api_can_still_parse_every_command_it_shells_out_to,
    "tests/test_agent_parity.py::test_the_api_can_still_parse_every_command_it_shells_out_to",
    {
        for argv in [
            vec![
                "engines",
                "--acquire-cloak",
                "OFFER_ID",
                "--approve",
                "--json",
            ],
            vec!["cookies", "export", "PID", "out.json"],
            vec!["cookies", "import", "PID", "in.json"],
            vec!["cookies", "import", "PID", "in.json", "--clear"],
            vec!["proxy", "test", "PID", "--json"],
            vec!["trust", "PID", "--json"],
            vec!["tls", "PID", "--json"],
            vec!["dns", "PID", "--json"],
            vec!["align", "PID", "--json"],
            vec!["launch", "PID", "--headless"],
            vec!["attach", "PID", "--port", "9333"],
            vec!["attach", "PID", "--port", "9333", "--headless"],
            vec!["warmup", "PID", "--minutes", "5.0", "--headless"],
            vec![
                "warmup",
                "PID",
                "--minutes",
                "5.0",
                "--headless",
                "--preset",
                "ads",
            ],
        ] {
            let matches = numera_core::cli::parse(
                std::iter::once("numera")
                    .chain(["--data-dir", "isolated"])
                    .chain(argv.iter().copied()),
            )?;
            let mut registry = numera_core::cli::registry();
            registry.build();
            let values = numera_core::cli::reflection::values(&registry, &matches);
            assert!(values["command"].is_string());
            if argv.contains(&"--json") {
                assert_eq!(values["json"], true);
            }
        }
        Ok(())
    }
);
source_test!(
    agent_parity_test_trust_and_align_exit_zero_under_json_whatever_the_grade_trust_23ec52,
    "tests/test_agent_parity.py::test_trust_and_align_exit_zero_under_json_whatever_the_grade[trust]",
    {
        let poor = json!({"score":62,"grade":"C","passed":5,"total":8,"checks":[{"name":"canvas","ok":false,"detail":"noisy"}]});
        let report = poor.clone();
        for json_mode in [true, false] {
            let mut output =
                numera_core::cli::output::Output::new(Vec::new(), Vec::new(), json_mode, false);
            let outcome = numera_core::cli::profile_commands::Outcome {
                payload: report.clone(),
                exit: 1,
            };
            let outcome = numera_core::cli::probe_commands::complete_report(
                &mut output,
                "trust",
                "alpha",
                outcome,
            )?;
            assert_eq!(outcome.exit, if json_mode { 0 } else { 1 });
            output.emit(&outcome.payload, outcome.exit)?;
            let (stdout, _stderr) = output.into_parts();
            if json_mode {
                let payload: Value = serde_json::from_slice(&stdout)?;
                let score = payload.get("score").unwrap_or(&payload["after"]["score"]);
                assert_eq!(score, 62);
            } else {
                assert!(!stdout.is_empty());
                assert!(serde_json::from_slice::<Value>(&stdout).is_err());
            }
        }
        Ok(())
    }
);
source_test!(
    agent_parity_test_trust_and_align_exit_zero_under_json_whatever_the_grade_align_63765b,
    "tests/test_agent_parity.py::test_trust_and_align_exit_zero_under_json_whatever_the_grade[align]",
    {
        let poor = json!({"score":62,"grade":"C","passed":5,"total":8,"checks":[{"name":"canvas","ok":false,"detail":"noisy"}]});
        let report = json!({"before":poor,"after":poor,"changed":[],"os":"windows","checks":poor["checks"],"fingerprint":{}});
        for json_mode in [true, false] {
            let mut output =
                numera_core::cli::output::Output::new(Vec::new(), Vec::new(), json_mode, false);
            let outcome = numera_core::cli::profile_commands::Outcome {
                payload: report.clone(),
                exit: 1,
            };
            let outcome =
                numera_core::cli::align_commands::complete_report(&mut output, "alpha", outcome)?;
            assert_eq!(outcome.exit, if json_mode { 0 } else { 1 });
            output.emit(&outcome.payload, outcome.exit)?;
            let (stdout, _stderr) = output.into_parts();
            if json_mode {
                let payload: Value = serde_json::from_slice(&stdout)?;
                let score = payload.get("score").unwrap_or(&payload["after"]["score"]);
                assert_eq!(score, 62);
            } else {
                assert!(!stdout.is_empty());
                assert!(serde_json::from_slice::<Value>(&stdout).is_err());
            }
        }
        Ok(())
    }
);
source_test!(
    agent_parity_test_a_cli_created_profile_is_visible_to_the_dashboard,
    "tests/test_agent_parity.py::test_a_cli_created_profile_is_visible_to_the_dashboard",
    {
        let t = fresh();
        let root = t.path();
        let output = cli(root, &["create", "headless-made"], true);
        assert!(output.status.success());
        let h = Http::new(root);
        let rows = h.get("/api/profiles", None).body;
        assert!(
            rows.as_array()
                .expect("dashboard listing")
                .iter()
                .any(|p| p["name"] == "headless-made")
        );
        Ok(())
    }
);
