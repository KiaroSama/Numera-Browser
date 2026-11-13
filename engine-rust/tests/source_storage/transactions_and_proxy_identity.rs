use super::*;
source_test!(
    a_batch_that_fails_on_the_third_write_stores_none_of_them,
    "tests/test_transactions_and_proxy_identity.py::test_a_batch_that_fails_on_the_third_write_stores_none_of_them",
    {
        let temp = temp();
        let root = temp.path();
        let (status, body) = testing::with_observer(nth_failure(3), || {
            http(
                root,
                "POST",
                "/api/profiles/batch",
                json!({"profiles":[{"name":"a"},{"name":"b"},{"name":"d"}]}),
            )
        });
        assert!(status >= 500, "{body}");
        assert!(json_files(root, "dashboard").is_empty());
        assert!(json_files(root, "profiles").is_empty());
    }
);
source_test!(
    a_successful_batch_still_writes_both_copies,
    "tests/test_transactions_and_proxy_identity.py::test_a_successful_batch_still_writes_both_copies",
    {
        let temp = temp();
        let root = temp.path();
        let (status, body) = http(
            root,
            "POST",
            "/api/profiles/batch",
            json!({"profiles":[{"name":"a"},{"name":"b"}]}),
        );
        assert_eq!(status, 200, "{body}");
        assert_eq!(json_files(root, "dashboard").len(), 2);
        assert_eq!(json_files(root, "profiles").len(), 2);
        assert_eq!(
            http(root, "GET", "/api/profiles", Value::Null)
                .1
                .as_array()
                .expect("rows")
                .iter()
                .map(|p| p["seed"].to_string())
                .collect::<BTreeSet<_>>()
                .len(),
            2
        );
    }
);
source_test!(
    a_session_that_cannot_be_detached_is_reported,
    "tests/test_transactions_and_proxy_identity.py::test_a_session_that_cannot_be_detached_is_reported",
    {
        let temp = temp();
        let root = temp.path();
        let p = ds(root, json!({"name":"held","tags":[]}));
        let id = p["id"].as_str().expect("id");
        let session = sessions(root, id);
        let result = testing::with_observer(error_at("before-session-detach", "", 13), || {
            store(root).delete_session(id, "source-storage-host")
        });
        assert!(result.is_err());
        assert!(session.join("Cookies").exists());
    }
);
source_test!(
    a_removable_session_is_actually_removed,
    "tests/test_transactions_and_proxy_identity.py::test_a_removable_session_is_actually_removed",
    {
        let temp = temp();
        let root = temp.path();
        let p = ds(root, json!({"name":"held","tags":[]}));
        let id = p["id"].as_str().expect("id");
        let session = sessions(root, id);
        store(root)
            .delete_session(id, "source-storage-host")
            .expect("remove session");
        assert!(!session.exists());
        assert!(directories(&root.join("user-data"), ".deleting-").is_empty());
    }
);
source_test!(
    concurrent_folder_creates_both_survive,
    "tests/test_transactions_and_proxy_identity.py::test_concurrent_folder_creates_both_survive",
    {
        let temp = temp();
        let root = temp.path();
        let mut workers = Vec::new();
        for name in ["a", "b", "c"] {
            let root = root.to_owned();
            workers.push(thread::spawn(move || {
                for i in 0..15 {
                    numera_core::storage::folders::Folders::new(root.clone())
                        .create(&format!("{name}{i}"))
                        .expect("create folder");
                }
            }));
        }
        for worker in workers {
            worker.join().expect("folder worker");
        }
        let listed = numera_core::storage::folders::Folders::new(root.to_owned())
            .listing()
            .expect("folders");
        for name in ["a", "b", "c"] {
            assert_eq!(listed.iter().filter(|f| f.starts_with(name)).count(), 15);
        }
    }
);
source_test!(
    a_rename_and_a_create_do_not_lose_each_other,
    "tests/test_transactions_and_proxy_identity.py::test_a_rename_and_a_create_do_not_lose_each_other",
    {
        let temp = temp();
        let root = temp.path();
        let folders = numera_core::storage::folders::Folders::new(root.to_owned());
        folders.create("work").expect("work");
        let one = root.to_owned();
        let two = root.to_owned();
        let barrier = Arc::new(Barrier::new(2));
        let ready = barrier.clone();
        let a = thread::spawn(move || {
            ready.wait();
            numera_core::storage::folders::Folders::new(one)
                .rename("work", "client-a")
                .expect("rename");
        });
        let b = thread::spawn(move || {
            barrier.wait();
            numera_core::storage::folders::Folders::new(two)
                .create("fresh")
                .expect("create");
        });
        a.join().expect("rename writer");
        b.join().expect("create writer");
        let names = folders.listing().expect("folders");
        assert!(names.contains(&"client-a".into()) && names.contains(&"fresh".into()));
    }
);
source_test!(
    protocol_is_part_of_proxy_identity,
    "tests/test_transactions_and_proxy_identity.py::test_protocol_is_part_of_proxy_identity",
    {
        let temp = temp();
        let root = temp.path();
        let mut entry = json!({"type":"HTTP","host":"1.2.3.4","port":"1080","user":"u","pass":"p"});
        pool(root).add_many(vec![entry.clone()]).expect("HTTP");
        entry["type"] = "SOCKS5".into();
        pool(root).add_many(vec![entry]).expect("SOCKS5");
        let rows = pool(root).list().expect("pool");
        assert_eq!(rows.len(), 2);
        assert_eq!(
            rows.iter()
                .map(|p| numera_core::pool::identity(p).to_string())
                .collect::<BTreeSet<_>>()
                .len(),
            2
        );
    }
);
source_test!(
    the_same_proxy_twice_is_still_deduped,
    "tests/test_transactions_and_proxy_identity.py::test_the_same_proxy_twice_is_still_deduped",
    {
        let temp = temp();
        let root = temp.path();
        let entry = json!({"type":"HTTP","host":"1.2.3.4","port":"8080","user":"u","pass":"p"});
        pool(root).add_many(vec![entry.clone()]).expect("first");
        assert!(
            pool(root)
                .add_many(vec![entry])
                .expect("duplicate")
                .is_empty()
        );
        assert_eq!(pool(root).list().expect("pool").len(), 1);
    }
);
source_test!(
    editing_a_proxy_is_validated_like_creating_one_case_008,
    "tests/test_transactions_and_proxy_identity.py::test_editing_a_proxy_is_validated_like_creating_one[patch0-a port outside the range]",
    {
        let temp = temp();
        let root = temp.path();
        let (status, made) = http(
            root,
            "POST",
            "/api/proxies",
            json!({"proxies":[{"type":"HTTP","host":"1.2.3.4","port":"8080"}]}),
        );
        assert_eq!(status, 200, "{made}");
        let uri = format!(
            "/api/proxies/{}",
            made["proxies"][0]["id"].as_str().expect("id")
        );
        let before = http(root, "GET", "/api/proxies", Value::Null).1;
        let (status, body) = http(root, "PUT", &uri, json!({"port": "70000"}));
        assert!([400, 422].contains(&status), "{body}");
        assert_eq!(http(root, "GET", "/api/proxies", Value::Null).1, before);
    }
);
source_test!(
    editing_a_proxy_is_validated_like_creating_one_case_009,
    "tests/test_transactions_and_proxy_identity.py::test_editing_a_proxy_is_validated_like_creating_one[patch1-a port that is not a number]",
    {
        let temp = temp();
        let root = temp.path();
        let (status, made) = http(
            root,
            "POST",
            "/api/proxies",
            json!({"proxies":[{"type":"HTTP","host":"1.2.3.4","port":"8080"}]}),
        );
        assert_eq!(status, 200, "{made}");
        let uri = format!(
            "/api/proxies/{}",
            made["proxies"][0]["id"].as_str().expect("id")
        );
        let before = http(root, "GET", "/api/proxies", Value::Null).1;
        let (status, body) = http(root, "PUT", &uri, json!({"port": "notaport"}));
        assert!([400, 422].contains(&status), "{body}");
        assert_eq!(http(root, "GET", "/api/proxies", Value::Null).1, before);
    }
);
source_test!(
    editing_a_proxy_is_validated_like_creating_one_case_010,
    "tests/test_transactions_and_proxy_identity.py::test_editing_a_proxy_is_validated_like_creating_one[patch2-an unsupported type]",
    {
        let temp = temp();
        let root = temp.path();
        let (status, made) = http(
            root,
            "POST",
            "/api/proxies",
            json!({"proxies":[{"type":"HTTP","host":"1.2.3.4","port":"8080"}]}),
        );
        assert_eq!(status, 200, "{made}");
        let uri = format!(
            "/api/proxies/{}",
            made["proxies"][0]["id"].as_str().expect("id")
        );
        let before = http(root, "GET", "/api/proxies", Value::Null).1;
        let (status, body) = http(root, "PUT", &uri, json!({"type": "GOPHER"}));
        assert!([400, 422].contains(&status), "{body}");
        assert_eq!(http(root, "GET", "/api/proxies", Value::Null).1, before);
    }
);
source_test!(
    editing_a_proxy_is_validated_like_creating_one_case_011,
    "tests/test_transactions_and_proxy_identity.py::test_editing_a_proxy_is_validated_like_creating_one[patch3-an empty host]",
    {
        let temp = temp();
        let root = temp.path();
        let (status, made) = http(
            root,
            "POST",
            "/api/proxies",
            json!({"proxies":[{"type":"HTTP","host":"1.2.3.4","port":"8080"}]}),
        );
        assert_eq!(status, 200, "{made}");
        let uri = format!(
            "/api/proxies/{}",
            made["proxies"][0]["id"].as_str().expect("id")
        );
        let before = http(root, "GET", "/api/proxies", Value::Null).1;
        let (status, body) = http(root, "PUT", &uri, json!({"host": ""}));
        assert!([400, 422].contains(&status), "{body}");
        assert_eq!(http(root, "GET", "/api/proxies", Value::Null).1, before);
    }
);
source_test!(
    editing_a_proxy_is_validated_like_creating_one_case_012,
    "tests/test_transactions_and_proxy_identity.py::test_editing_a_proxy_is_validated_like_creating_one[patch4-a malformed country]",
    {
        let temp = temp();
        let root = temp.path();
        let (status, made) = http(
            root,
            "POST",
            "/api/proxies",
            json!({"proxies":[{"type":"HTTP","host":"1.2.3.4","port":"8080"}]}),
        );
        assert_eq!(status, 200, "{made}");
        let uri = format!(
            "/api/proxies/{}",
            made["proxies"][0]["id"].as_str().expect("id")
        );
        let before = http(root, "GET", "/api/proxies", Value::Null).1;
        let (status, body) = http(root, "PUT", &uri, json!({"country": "xyz"}));
        assert!([400, 422].contains(&status), "{body}");
        assert_eq!(http(root, "GET", "/api/proxies", Value::Null).1, before);
    }
);
source_test!(
    a_valid_edit_still_applies,
    "tests/test_transactions_and_proxy_identity.py::test_a_valid_edit_still_applies",
    {
        let temp = temp();
        let root = temp.path();
        let (status, made) = http(
            root,
            "POST",
            "/api/proxies",
            json!({"proxies":[{"type":"HTTP","host":"1.2.3.4","port":"8080"}]}),
        );
        assert_eq!(status, 200, "{made}");
        let uri = format!(
            "/api/proxies/{}",
            made["proxies"][0]["id"].as_str().expect("id")
        );
        let (status, body) = http(root, "PUT", &uri, json!({"type":"SOCKS5","port":"1080"}));
        assert_eq!(status, 200, "{body}");
        let got = http(root, "GET", "/api/proxies", Value::Null).1;
        assert_eq!(got[0]["type"], "SOCKS5");
        assert_eq!(got[0]["port"], "1080");
    }
);
source_test!(
    an_unsupported_text_scheme_is_refused_not_rewritten,
    "tests/test_transactions_and_proxy_identity.py::test_an_unsupported_text_scheme_is_refused_not_rewritten",
    {
        let temp = temp();
        let root = temp.path();
        let (status, body) = http(
            root,
            "POST",
            "/api/proxies",
            json!({"text":"gopher://1.2.3.4:70"}),
        );
        assert!([400, 422].contains(&status), "{body}");
        assert!(
            http(root, "GET", "/api/proxies", Value::Null)
                .1
                .as_array()
                .expect("pool")
                .is_empty()
        );
    }
);
source_test!(
    a_bare_host_port_line_is_still_http,
    "tests/test_transactions_and_proxy_identity.py::test_a_bare_host_port_line_is_still_http",
    {
        let temp = temp();
        let root = temp.path();
        let (status, body) = http(root, "POST", "/api/proxies", json!({"text":"1.2.3.4:8080"}));
        assert_eq!(status, 200, "{body}");
        assert_eq!(
            http(root, "GET", "/api/proxies", Value::Null).1[0]["type"],
            "HTTP"
        );
    }
);
source_test!(
    every_protocol_survives_the_cli_assignment_case_016,
    "tests/test_transactions_and_proxy_identity.py::test_every_protocol_survives_the_cli_assignment[HTTP-http]",
    {
        assert_eq!(
            numera_core::pool::server_url("HTTP", "1.2.3.4", "8080").expect("URL"),
            "http://1.2.3.4:8080"
        );
    }
);
source_test!(
    every_protocol_survives_the_cli_assignment_case_017,
    "tests/test_transactions_and_proxy_identity.py::test_every_protocol_survives_the_cli_assignment[HTTPS-https]",
    {
        assert_eq!(
            numera_core::pool::server_url("HTTPS", "1.2.3.4", "8080").expect("URL"),
            "https://1.2.3.4:8080"
        );
    }
);
source_test!(
    every_protocol_survives_the_cli_assignment_case_018,
    "tests/test_transactions_and_proxy_identity.py::test_every_protocol_survives_the_cli_assignment[SOCKS4-socks4]",
    {
        assert_eq!(
            numera_core::pool::server_url("SOCKS4", "1.2.3.4", "8080").expect("URL"),
            "socks4://1.2.3.4:8080"
        );
    }
);
source_test!(
    every_protocol_survives_the_cli_assignment_case_019,
    "tests/test_transactions_and_proxy_identity.py::test_every_protocol_survives_the_cli_assignment[SOCKS5-socks5]",
    {
        assert_eq!(
            numera_core::pool::server_url("SOCKS5", "1.2.3.4", "8080").expect("URL"),
            "socks5://1.2.3.4:8080"
        );
    }
);
source_test!(
    an_ipv6_host_stays_bracketed,
    "tests/test_transactions_and_proxy_identity.py::test_an_ipv6_host_stays_bracketed",
    {
        assert_eq!(
            numera_core::pool::server_url("SOCKS5", "2001:db8::1", "1080").expect("IPv6 URL"),
            "socks5://[2001:db8::1]:1080"
        );
    }
);
