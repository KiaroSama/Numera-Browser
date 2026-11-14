use super::*;
source_test!(
    an_explicit_clear_removes_the_password_and_keeps_the_proxy,
    "tests/test_pooled_proxy_secret_intent.py::test_an_explicit_clear_removes_the_password_and_keeps_the_proxy",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let id = added_http(root);
        let (status, response) = http(
            root,
            None,
            "PUT",
            &format!("/api/proxies/{id}"),
            json!({"pass":""}),
        );
        assert_eq!(status, 200, "{response}");
        let entry = pool(root, None).get(&id).expect("pool get").expect("proxy");
        assert!(entry["pass"].as_str().unwrap_or("").is_empty());
        assert_eq!(
            (
                entry["host"].as_str(),
                entry["port"].as_str(),
                entry["type"].as_str(),
                entry["user"].as_str()
            ),
            (Some("h.example"), Some("8080"), Some("HTTP"), Some("u"))
        );
    }
);
source_test!(
    an_omitted_password_is_left_alone,
    "tests/test_pooled_proxy_secret_intent.py::test_an_omitted_password_is_left_alone",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let id = added_http(root);
        let (status, response) = http(
            root,
            None,
            "PUT",
            &format!("/api/proxies/{id}"),
            json!({"label":"renamed"}),
        );
        assert_eq!(status, 200, "{response}");
        let entry = pool(root, None).get(&id).expect("get").expect("proxy");
        assert_eq!(entry["pass"], "secret");
        assert_eq!(entry["label"], "renamed");
    }
);
source_test!(
    a_new_password_replaces_the_old_one,
    "tests/test_pooled_proxy_secret_intent.py::test_a_new_password_replaces_the_old_one",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let id = added_http(root);
        let (status, response) = http(
            root,
            None,
            "PUT",
            &format!("/api/proxies/{id}"),
            json!({"pass":"other"}),
        );
        assert_eq!(status, 200, "{response}");
        assert_eq!(
            pool(root, None).get(&id).expect("get").expect("proxy")["pass"],
            "other"
        );
    }
);
source_test!(
    an_unlocked_clear_removes_the_ciphertext,
    "tests/test_pooled_proxy_secret_intent.py::test_an_unlocked_clear_removes_the_ciphertext",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let id = added_http(root);
        enable(root, MASTER);
        assert_ne!(read(root, "proxies.json")[0]["pass"], "secret");
        let (status, response) = http(
            root,
            Some(MASTER),
            "PUT",
            &format!("/api/proxies/{id}"),
            json!({"pass":""}),
        );
        assert_eq!(status, 200, "{response}");
        assert!(
            read(root, "proxies.json")[0]["pass"]
                .as_str()
                .unwrap_or("")
                .is_empty()
        );
        assert_eq!(read(root, "proxies.json")[0]["host"], "h.example");
    }
);
source_test!(
    a_locked_clear_is_refused_rather_than_guessed,
    "tests/test_pooled_proxy_secret_intent.py::test_a_locked_clear_is_refused_rather_than_guessed",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let id = added_http(root);
        enable(root, MASTER);
        let before = read(root, "proxies.json");
        let (status, response) = http(
            root,
            None,
            "PUT",
            &format!("/api/proxies/{id}"),
            json!({"pass":""}),
        );
        assert_eq!(status, 423, "{response}");
        assert_eq!(read(root, "proxies.json"), before);
    }
);
source_test!(
    an_edit_refuses_a_password_that_is_not_text_case_005,
    "tests/test_pooled_proxy_secret_intent.py::test_an_edit_refuses_a_password_that_is_not_text[a boolean]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let id = added_http(root);
        let before = read(root, "proxies.json");
        let (status, response) = http(
            root,
            None,
            "PUT",
            &format!("/api/proxies/{id}"),
            json!({"pass":json!(true)}),
        );
        assert_eq!(status, 400, "{response}");
        assert!(
            response["detail"]
                .as_str()
                .expect("detail")
                .contains("password must be text")
        );
        assert_eq!(read(root, "proxies.json"), before);
    }
);
source_test!(
    an_edit_refuses_a_password_that_is_not_text_case_006,
    "tests/test_pooled_proxy_secret_intent.py::test_an_edit_refuses_a_password_that_is_not_text[a float]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let id = added_http(root);
        let before = read(root, "proxies.json");
        let (status, response) = http(
            root,
            None,
            "PUT",
            &format!("/api/proxies/{id}"),
            json!({"pass":json!(1.5)}),
        );
        assert_eq!(status, 400, "{response}");
        assert!(
            response["detail"]
                .as_str()
                .expect("detail")
                .contains("password must be text")
        );
        assert_eq!(read(root, "proxies.json"), before);
    }
);
source_test!(
    an_edit_refuses_a_password_that_is_not_text_case_007,
    "tests/test_pooled_proxy_secret_intent.py::test_an_edit_refuses_a_password_that_is_not_text[a list]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let id = added_http(root);
        let before = read(root, "proxies.json");
        let (status, response) = http(
            root,
            None,
            "PUT",
            &format!("/api/proxies/{id}"),
            json!({"pass":json!(["x"])}),
        );
        assert_eq!(status, 400, "{response}");
        assert!(
            response["detail"]
                .as_str()
                .expect("detail")
                .contains("password must be text")
        );
        assert_eq!(read(root, "proxies.json"), before);
    }
);
source_test!(
    an_edit_refuses_a_password_that_is_not_text_case_008,
    "tests/test_pooled_proxy_secret_intent.py::test_an_edit_refuses_a_password_that_is_not_text[a number]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let id = added_http(root);
        let before = read(root, "proxies.json");
        let (status, response) = http(
            root,
            None,
            "PUT",
            &format!("/api/proxies/{id}"),
            json!({"pass":json!(123)}),
        );
        assert_eq!(status, 400, "{response}");
        assert!(
            response["detail"]
                .as_str()
                .expect("detail")
                .contains("password must be text")
        );
        assert_eq!(read(root, "proxies.json"), before);
    }
);
source_test!(
    an_edit_refuses_a_password_that_is_not_text_case_009,
    "tests/test_pooled_proxy_secret_intent.py::test_an_edit_refuses_a_password_that_is_not_text[an object]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let id = added_http(root);
        let before = read(root, "proxies.json");
        let (status, response) = http(
            root,
            None,
            "PUT",
            &format!("/api/proxies/{id}"),
            json!({"pass":json!({"a":1})}),
        );
        assert_eq!(status, 400, "{response}");
        assert!(
            response["detail"]
                .as_str()
                .expect("detail")
                .contains("password must be text")
        );
        assert_eq!(read(root, "proxies.json"), before);
    }
);
source_test!(
    create_refuses_the_same_shapes_case_010,
    "tests/test_pooled_proxy_secret_intent.py::test_create_refuses_the_same_shapes[a boolean]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let (status, response) = http(
            root,
            None,
            "POST",
            "/api/proxies",
            json!({"proxies":[pool_entry("h.example",json!(true))]}),
        );
        assert_eq!(status, 400, "{response}");
        assert!(!root.join("proxies.json").exists() || read(root, "proxies.json") == json!([]));
    }
);
source_test!(
    create_refuses_the_same_shapes_case_011,
    "tests/test_pooled_proxy_secret_intent.py::test_create_refuses_the_same_shapes[a float]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let (status, response) = http(
            root,
            None,
            "POST",
            "/api/proxies",
            json!({"proxies":[pool_entry("h.example",json!(1.5))]}),
        );
        assert_eq!(status, 400, "{response}");
        assert!(!root.join("proxies.json").exists() || read(root, "proxies.json") == json!([]));
    }
);
source_test!(
    create_refuses_the_same_shapes_case_012,
    "tests/test_pooled_proxy_secret_intent.py::test_create_refuses_the_same_shapes[a list]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let (status, response) = http(
            root,
            None,
            "POST",
            "/api/proxies",
            json!({"proxies":[pool_entry("h.example",json!(["x"]))]}),
        );
        assert_eq!(status, 400, "{response}");
        assert!(!root.join("proxies.json").exists() || read(root, "proxies.json") == json!([]));
    }
);
source_test!(
    create_refuses_the_same_shapes_case_013,
    "tests/test_pooled_proxy_secret_intent.py::test_create_refuses_the_same_shapes[a number]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let (status, response) = http(
            root,
            None,
            "POST",
            "/api/proxies",
            json!({"proxies":[pool_entry("h.example",json!(123))]}),
        );
        assert_eq!(status, 400, "{response}");
        assert!(!root.join("proxies.json").exists() || read(root, "proxies.json") == json!([]));
    }
);
source_test!(
    create_refuses_the_same_shapes_case_014,
    "tests/test_pooled_proxy_secret_intent.py::test_create_refuses_the_same_shapes[an object]",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let (status, response) = http(
            root,
            None,
            "POST",
            "/api/proxies",
            json!({"proxies":[pool_entry("h.example",json!({"a":1}))]}),
        );
        assert_eq!(status, 400, "{response}");
        assert!(!root.join("proxies.json").exists() || read(root, "proxies.json") == json!([]));
    }
);
source_test!(
    the_shapes_that_are_meant_to_work_still_do,
    "tests/test_pooled_proxy_secret_intent.py::test_the_shapes_that_are_meant_to_work_still_do",
    {
        let temp = common::TempDirectory::new("source-vault").expect("temporary root");
        let root = temp.path();
        let id = added_http(root);
        for (body, expected) in [
            (json!({"pass":"other"}), json!("other")),
            (json!({"label":"renamed"}), json!("other")),
            (json!({"pass":""}), json!("")),
        ] {
            let (status, response) = http(root, None, "PUT", &format!("/api/proxies/{id}"), body);
            assert_eq!(status, 200, "{response}");
            let (status, rows) = http(root, None, "GET", "/api/proxies", Value::Null);
            assert_eq!(status, 200);
            assert_eq!(
                rows[0]["pass"].as_str().unwrap_or(""),
                expected.as_str().expect("expected")
            );
        }
    }
);
