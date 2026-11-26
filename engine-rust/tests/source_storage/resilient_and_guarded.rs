use super::*;
source_test!(
    a_profile_encrypted_under_another_key_does_not_hide_the_rest,
    "tests/test_resilient_and_guarded.py::test_a_profile_encrypted_under_another_key_does_not_hide_the_rest",
    {
        let temp = temp();
        let root = temp.path();
        let good = saved(root, "good", 1);
        let other = secret_profile(root, "other", 2, Some("SECRET"));
        store(root).enable_vault("master").expect("vault");
        let mut data = raw(root, &format!("profiles/{}.json", other.id));
        data["proxy"]["password"] = numera_core::vault::encrypt("different", &new_salt(), "SECRET")
            .expect("foreign envelope")
            .into();
        write(root, &format!("profiles/{}.json", other.id), data);
        let unlocked = ProfileRepo::new(root.to_owned(), Some("master".into()));
        assert!(
            unlocked
                .list(1001.0)
                .expect("list")
                .iter()
                .any(|p| p.name == "good")
        );
        assert!(unlocked.get(&good.id).expect("get good").is_some());
    }
);
source_test!(
    an_unreadable_secret_reads_as_none_not_ciphertext,
    "tests/test_resilient_and_guarded.py::test_an_unreadable_secret_reads_as_none_not_ciphertext",
    {
        let temp = temp();
        let root = temp.path();
        let other = secret_profile(root, "other", 2, Some("SECRET"));
        store(root).enable_vault("master").expect("vault");
        let mut data = raw(root, &format!("profiles/{}.json", other.id));
        data["proxy"]["password"] = numera_core::vault::encrypt("different", &new_salt(), "SECRET")
            .expect("foreign envelope")
            .into();
        write(root, &format!("profiles/{}.json", other.id), data);
        let unlocked = ProfileRepo::new(root.to_owned(), Some("master".into()));
        let got = unlocked.get(&other.id).expect("get").expect("profile");
        let proxy = got.proxy.expect("proxy").to_record();
        assert!(proxy["password"].is_null());
        assert!(!numera_core::vault::is_envelope(
            proxy["password"].as_str().unwrap_or("")
        ));
    }
);
source_test!(
    a_proxy_with_an_unknown_field_still_loads,
    "tests/test_resilient_and_guarded.py::test_a_proxy_with_an_unknown_field_still_loads",
    {
        let temp = temp();
        let root = temp.path();
        let p = secret_profile(root, "p", 1, Some("SECRET"));
        let mut record = raw(root, &format!("profiles/{}.json", p.id));
        record["proxy"]["somethingNewerAdded"] = "value".into();
        write(root, &format!("profiles/{}.json", p.id), record);
        assert_eq!(
            store(root)
                .get(&p.id)
                .expect("get")
                .expect("profile")
                .proxy
                .expect("proxy")
                .server,
            "http://1.2.3.4:8080"
        );
    }
);
source_test!(
    a_non_numeric_core_count_does_not_raise_case_003,
    "tests/test_resilient_and_guarded.py::test_a_non_numeric_core_count_does_not_raise[8 cores]",
    {
        let (p, _) = numera_core::importers::model(
            &json!({"name":"x","navigator":{"hardwareConcurrency":"8 cores"}}),
            1,
            1000.0,
        )
        .expect("vendor quirk");
        assert!(p.fingerprint.hardware_concurrency > 0);
    }
);
source_test!(
    a_non_numeric_core_count_does_not_raise_case_004,
    "tests/test_resilient_and_guarded.py::test_a_non_numeric_core_count_does_not_raise[N/A]",
    {
        let (p, _) = numera_core::importers::model(
            &json!({"name":"x","navigator":{"hardwareConcurrency":"N/A"}}),
            1,
            1000.0,
        )
        .expect("vendor quirk");
        assert!(p.fingerprint.hardware_concurrency > 0);
    }
);
source_test!(
    a_non_numeric_core_count_does_not_raise_case_005,
    "tests/test_resilient_and_guarded.py::test_a_non_numeric_core_count_does_not_raise[]",
    {
        let (p, _) = numera_core::importers::model(
            &json!({"name":"x","navigator":{"hardwareConcurrency":""}}),
            1,
            1000.0,
        )
        .expect("vendor quirk");
        assert!(p.fingerprint.hardware_concurrency > 0);
    }
);
source_test!(
    a_non_numeric_core_count_does_not_raise_case_006,
    "tests/test_resilient_and_guarded.py::test_a_non_numeric_core_count_does_not_raise[None]",
    {
        let (p, _) = numera_core::importers::model(
            &json!({"name":"x","navigator":{"hardwareConcurrency":null}}),
            1,
            1000.0,
        )
        .expect("vendor quirk");
        assert!(p.fingerprint.hardware_concurrency > 0);
    }
);
source_test!(
    a_non_numeric_core_count_does_not_raise_case_007,
    "tests/test_resilient_and_guarded.py::test_a_non_numeric_core_count_does_not_raise[8.5]",
    {
        let (p, _) = numera_core::importers::model(
            &json!({"name":"x","navigator":{"hardwareConcurrency":"8.5"}}),
            1,
            1000.0,
        )
        .expect("vendor quirk");
        assert!(p.fingerprint.hardware_concurrency > 0);
    }
);
source_test!(
    a_numeric_core_count_is_still_honoured,
    "tests/test_resilient_and_guarded.py::test_a_numeric_core_count_is_still_honoured",
    {
        let (p, _) = numera_core::importers::model(
            &json!({"name":"x","navigator":{"hardwareConcurrency":12}}),
            1,
            1000.0,
        )
        .expect("import");
        assert_eq!(p.fingerprint.hardware_concurrency, 12);
    }
);
source_test!(
    importing_a_file_with_one_bad_entry_is_a_4xx_not_a_500,
    "tests/test_resilient_and_guarded.py::test_importing_a_file_with_one_bad_entry_is_a_4xx_not_a_500",
    {
        let temp = temp();
        let root = temp.path();
        let payload = serde_json::to_string(
            &json!([{"name":"one","navigator":{"hardwareConcurrency":"8 cores"}}]),
        )
        .expect("export");
        let (status, _body) = http(
            root,
            "POST",
            "/api/profiles/import",
            json!({"text":payload}),
        );
        assert!(status < 500, "HTTP response status mismatch");
    }
);
source_test!(
    the_vault_key_is_derived_once_per_password_and_salt,
    "tests/test_resilient_and_guarded.py::test_the_vault_key_is_derived_once_per_password_and_salt",
    {
        let salt = new_salt();
        let calls = Arc::new(Mutex::new(0));
        let seen = calls.clone();
        testing::with_observer(
            move |event| {
                if event.operation == "before-key-derivation" {
                    *seen.lock().expect("count") += 1;
                }
                Ok(Action::Continue)
            },
            || {
                for _ in 0..5 {
                    let token =
                        numera_core::vault::encrypt("pw", &salt, "secret").expect("encrypt");
                    assert_eq!(
                        numera_core::vault::decrypt("pw", &salt, &token).expect("decrypt"),
                        "secret"
                    );
                }
            },
        );
        assert_eq!(*calls.lock().expect("count"), 1);
    }
);
source_test!(
    a_different_password_or_salt_derives_again,
    "tests/test_resilient_and_guarded.py::test_a_different_password_or_salt_derives_again",
    {
        let a = new_salt();
        let b = new_salt();
        let calls = Arc::new(Mutex::new(0));
        let seen = calls.clone();
        testing::with_observer(
            move |event| {
                if event.operation == "before-key-derivation" {
                    *seen.lock().expect("count") += 1;
                }
                Ok(Action::Continue)
            },
            || {
                for (password, salt) in [("pw", &a), ("pw", &b), ("other", &a)] {
                    numera_core::vault::encrypt(password, salt, "x").expect("encrypt");
                }
            },
        );
        assert_eq!(*calls.lock().expect("count"), 3);
    }
);
source_test!(
    the_round_trip_still_works_across_store_instances,
    "tests/test_resilient_and_guarded.py::test_the_round_trip_still_works_across_store_instances",
    {
        let temp = temp();
        let root = temp.path();
        let p = secret_profile(root, "p", 1, Some("SECRET"));
        store(root).enable_vault("master").expect("vault");
        assert_eq!(
            ProfileRepo::new(root.to_owned(), Some("master".into()))
                .resolve(&p.id, 1001.0)
                .expect("resolve")
                .expect("profile")
                .proxy
                .expect("proxy")
                .to_record()["password"],
            "SECRET"
        );
    }
);
source_test!(
    rotate_refuses_a_private_destination_case_013,
    "tests/test_resilient_and_guarded.py::test_rotate_refuses_a_private_destination[http://127.0.0.1/rotate]",
    {
        assert!(public_destination("http://127.0.0.1/rotate").is_err());
        let result = numera_core::network::proxy_rotate::rotate_sync(
            &json!({"rotateUrl":"http://127.0.0.1/rotate","host":"1.2.3.4","port":"8080"}),
            Duration::from_secs(2),
        )
        .expect("rotation result");
        assert_eq!(result["rotated"], false);
        assert!(result["ok"].is_null());
    }
);
source_test!(
    rotate_refuses_a_private_destination_case_014,
    "tests/test_resilient_and_guarded.py::test_rotate_refuses_a_private_destination[http://localhost:8787/rotate]",
    {
        assert!(public_destination("http://localhost:8787/rotate").is_err());
        let result = numera_core::network::proxy_rotate::rotate_sync(
            &json!({"rotateUrl":"http://localhost:8787/rotate","host":"1.2.3.4","port":"8080"}),
            Duration::from_secs(2),
        )
        .expect("rotation result");
        assert_eq!(result["rotated"], false);
        assert!(result["ok"].is_null());
    }
);
source_test!(
    rotate_refuses_a_private_destination_case_015,
    "tests/test_resilient_and_guarded.py::test_rotate_refuses_a_private_destination[http://169.254.169.254/latest/meta-data/]",
    {
        assert!(public_destination("http://169.254.169.254/latest/meta-data/").is_err());
        let result=numera_core::network::proxy_rotate::rotate_sync(&json!({"rotateUrl":"http://169.254.169.254/latest/meta-data/","host":"1.2.3.4","port":"8080"}),Duration::from_secs(2)).expect("rotation result");
        assert_eq!(result["rotated"], false);
        assert!(result["ok"].is_null());
    }
);
source_test!(
    rotate_refuses_a_private_destination_case_016,
    "tests/test_resilient_and_guarded.py::test_rotate_refuses_a_private_destination[http://10.0.0.5/rotate]",
    {
        assert!(public_destination("http://10.0.0.5/rotate").is_err());
        let result = numera_core::network::proxy_rotate::rotate_sync(
            &json!({"rotateUrl":"http://10.0.0.5/rotate","host":"1.2.3.4","port":"8080"}),
            Duration::from_secs(2),
        )
        .expect("rotation result");
        assert_eq!(result["rotated"], false);
        assert!(result["ok"].is_null());
    }
);
source_test!(
    rotate_refuses_a_private_destination_case_017,
    "tests/test_resilient_and_guarded.py::test_rotate_refuses_a_private_destination[http://192.168.1.1/rotate]",
    {
        assert!(public_destination("http://192.168.1.1/rotate").is_err());
        let result = numera_core::network::proxy_rotate::rotate_sync(
            &json!({"rotateUrl":"http://192.168.1.1/rotate","host":"1.2.3.4","port":"8080"}),
            Duration::from_secs(2),
        )
        .expect("rotation result");
        assert_eq!(result["rotated"], false);
        assert!(result["ok"].is_null());
    }
);
source_test!(
    rotate_refuses_a_private_destination_case_018,
    "tests/test_resilient_and_guarded.py::test_rotate_refuses_a_private_destination[http://[::1]/rotate]",
    {
        assert!(public_destination("http://[::1]/rotate").is_err());
        let result = numera_core::network::proxy_rotate::rotate_sync(
            &json!({"rotateUrl":"http://[::1]/rotate","host":"1.2.3.4","port":"8080"}),
            Duration::from_secs(2),
        )
        .expect("rotation result");
        assert_eq!(result["rotated"], false);
        assert!(result["ok"].is_null());
    }
);
source_test!(
    rotate_still_allows_a_real_provider_url,
    "tests/test_resilient_and_guarded.py::test_rotate_still_allows_a_real_provider_url",
    {
        let (url, pinned) =
            public_destination("https://example.com/api/rotate").expect("public provider");
        assert_eq!(url.host_str(), Some("example.com"));
        assert_eq!(
            pinned,
            "93.184.216.34"
                .parse::<std::net::IpAddr>()
                .expect("fixture public address")
        );
    }
);
source_test!(
    rotate_still_refuses_a_non_http_scheme_case_020,
    "tests/test_resilient_and_guarded.py::test_rotate_still_refuses_a_non_http_scheme[ftp://example.com/x]",
    {
        assert!(public_destination("ftp://example.com/x").is_err());
    }
);
source_test!(
    rotate_still_refuses_a_non_http_scheme_case_021,
    "tests/test_resilient_and_guarded.py::test_rotate_still_refuses_a_non_http_scheme[file:///etc/passwd]",
    {
        assert!(public_destination("file:///etc/passwd").is_err());
    }
);
source_test!(
    rotate_still_refuses_a_non_http_scheme_case_022,
    "tests/test_resilient_and_guarded.py::test_rotate_still_refuses_a_non_http_scheme[gopher://x/]",
    {
        assert!(public_destination("gopher://x/").is_err());
    }
);
source_test!(
    an_empty_host_is_not_treated_as_loopback,
    "tests/test_resilient_and_guarded.py::test_an_empty_host_is_not_treated_as_loopback",
    {
        let temp = temp();
        let root = temp.path();
        let tokens = numera_core::http::tokens::TokenStore::new(root.to_owned());
        assert!(numera_core::http::bind_policy::refusal("", &tokens, "", "").is_some());
    }
);
source_test!(
    real_loopback_still_needs_no_credential_case_024,
    "tests/test_resilient_and_guarded.py::test_real_loopback_still_needs_no_credential[127.0.0.1]",
    {
        let temp = temp();
        let root = temp.path();
        let tokens = numera_core::http::tokens::TokenStore::new(root.to_owned());
        assert!(!numera_core::http::bind_policy::refusal("127.0.0.1", &tokens, "", "").is_some());
    }
);
source_test!(
    real_loopback_still_needs_no_credential_case_025,
    "tests/test_resilient_and_guarded.py::test_real_loopback_still_needs_no_credential[localhost]",
    {
        let temp = temp();
        let root = temp.path();
        let tokens = numera_core::http::tokens::TokenStore::new(root.to_owned());
        assert!(!numera_core::http::bind_policy::refusal("localhost", &tokens, "", "").is_some());
    }
);
source_test!(
    real_loopback_still_needs_no_credential_case_026,
    "tests/test_resilient_and_guarded.py::test_real_loopback_still_needs_no_credential[::1]",
    {
        let temp = temp();
        let root = temp.path();
        let tokens = numera_core::http::tokens::TokenStore::new(root.to_owned());
        assert!(numera_core::http::bind_policy::refusal("::1", &tokens, "", "").is_none());
    }
);
