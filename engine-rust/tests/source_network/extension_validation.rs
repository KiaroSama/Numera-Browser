use super::*;
source_test!(
    c000_a_web_store_url_in_the_folder_list_points_at_the_installer,
    "tests/test_extension_validation.py::test_a_web_store_url_in_the_folder_list_points_at_the_installer",
    {
        let tmp = common::TempDirectory::new("source-net-url-default").expect("root");
        let (s, v) = json_http(
            tmp.path(),
            "PUT",
            "/api/config",
            json!({"default_extensions":["https://chromewebstore.google.com/detail/x/abcdefg?hl=en"]}),
        );
        assert_eq!(s, 400);
        let detail = v["detail"].as_str().expect("detail");
        assert!(detail.contains("URL"));
        assert!(detail.contains("ext install") && detail.contains("Extensions page"));
    }
);
source_test!(
    c000_a_path_that_does_not_exist_is_refused,
    "tests/test_extension_validation.py::test_a_path_that_does_not_exist_is_refused",
    {
        let tmp = common::TempDirectory::new("source-net-no-path").expect("root");
        let (s, v) = put_defaults(tmp.path(), &[tmp.path().join("nope")]);
        assert_eq!(s, 400);
        assert!(
            v["detail"]
                .as_str()
                .expect("detail")
                .contains("not a folder")
        );
    }
);
source_test!(
    c000_a_folder_without_a_manifest_is_refused,
    "tests/test_extension_validation.py::test_a_folder_without_a_manifest_is_refused",
    {
        let tmp = common::TempDirectory::new("source-net-no-manifest").expect("root");
        let ext = tmp.path().join("empty");
        fs::create_dir(&ext).expect("empty folder");
        let (s, v) = put_defaults(tmp.path(), &[ext]);
        assert_eq!(s, 400);
        assert!(
            v["detail"]
                .as_str()
                .expect("detail")
                .contains("manifest.json")
        );
    }
);
source_test!(
    c000_a_real_unpacked_extension_is_accepted,
    "tests/test_extension_validation.py::test_a_real_unpacked_extension_is_accepted",
    {
        let tmp = common::TempDirectory::new("source-net-valid-default").expect("root");
        let ext = unpacked(tmp.path(), "probe");
        let (s, v) = put_defaults(tmp.path(), std::slice::from_ref(&ext));
        assert_eq!(s, 200, "{v}");
        assert!(
            v["default_extensions"]
                .as_array()
                .expect("defaults")
                .contains(&json!(ext))
        );
    }
);
source_test!(
    c000_nothing_is_stored_when_one_entry_is_bad,
    "tests/test_extension_validation.py::test_nothing_is_stored_when_one_entry_is_bad",
    {
        let tmp = common::TempDirectory::new("source-net-atomic-default").expect("root");
        let ext = unpacked(tmp.path(), "probe");
        assert_eq!(put_defaults(tmp.path(), std::slice::from_ref(&ext)).0, 200);
        let before = fs::read(tmp.path().join("config.json")).expect("original config");
        assert_eq!(
            put_defaults(tmp.path(), &[ext, tmp.path().join("missing")]).0,
            400
        );
        assert_eq!(
            fs::read(tmp.path().join("config.json")).expect("preserved config"),
            before
        );
    }
);
source_test!(
    c000_the_launcher_would_have_dropped_exactly_what_we_now_refuse,
    "tests/test_extension_validation.py::test_the_launcher_would_have_dropped_exactly_what_we_now_refuse",
    {
        let tmp = common::TempDirectory::new("source-net-dropped-paths").expect("root");
        let bad = vec![
            "https://chromewebstore.google.com/detail/x/y".into(),
            tmp.path().join("missing").to_string_lossy().into_owned(),
        ];
        assert!(ext_flags(bad, tmp.path()).is_empty());
        let ext = unpacked(tmp.path(), "probe");
        assert!(
            ext_flags(vec![ext.to_string_lossy().into_owned()], tmp.path())
                .iter()
                .any(|a| a.contains("--load-extension="))
        );
    }
);
source_test!(
    c000_the_icon_endpoint_serves_the_extension_own_icon,
    "tests/test_extension_validation.py::test_the_icon_endpoint_serves_the_extension_own_icon",
    {
        let tmp = common::TempDirectory::new("source-net-icon-http").expect("root");
        let ext = tmp.path().join("withicon");
        let png = b"\x89PNG\r\n\x1a\n-pretend";
        write_ext(
            &ext,
            json!({"manifest_version":3,"name":"Has Icon","version":"1","icons":{"48":"logo.png"}}),
            &[("logo.png", png)],
        );
        assert_eq!(put_defaults(tmp.path(), std::slice::from_ref(&ext)).0, 200);
        let (s, v) = json_http(tmp.path(), "GET", "/api/extensions/installed", Value::Null);
        assert_eq!(s, 200);
        assert_eq!(v["extensions"][0]["name"], "Has Icon");
        assert!(
            v["extensions"][0]["icon"]
                .as_str()
                .expect("icon path")
                .ends_with("logo.png")
        );
        let response = http(tmp.path(), None, "GET", &icon_url(&ext), Value::Null);
        assert_eq!(response.status, 200);
        assert_eq!(response.body, png);
    }
);
source_test!(
    c000_the_icon_endpoint_refuses_a_folder_it_was_never_told_about,
    "tests/test_extension_validation.py::test_the_icon_endpoint_refuses_a_folder_it_was_never_told_about",
    {
        let tmp = common::TempDirectory::new("source-net-stranger-icon").expect("root");
        let ext = tmp.path().join("elsewhere");
        write_ext(
            &ext,
            json!({"name":"X","icons":{"48":"i.png"}}),
            &[("i.png", b"secret")],
        );
        assert_eq!(
            http(tmp.path(), None, "GET", &icon_url(&ext), Value::Null).status,
            404
        );
    }
);
source_test!(
    c000_an_extension_with_no_icon_404s_rather_than_serving_something_else,
    "tests/test_extension_validation.py::test_an_extension_with_no_icon_404s_rather_than_serving_something_else",
    {
        let tmp = common::TempDirectory::new("source-net-missing-icon").expect("root");
        let ext = unpacked(tmp.path(), "probe");
        assert_eq!(put_defaults(tmp.path(), std::slice::from_ref(&ext)).0, 200);
        let response = http(tmp.path(), None, "GET", &icon_url(&ext), Value::Null);
        assert_eq!(response.status, 404);
        assert!(
            response.json.expect("JSON error")["detail"]
                .as_str()
                .expect("detail")
                .contains("icon")
        );
    }
);
source_test!(
    c000_a_hand_unpacked_folder_is_named_too,
    "tests/test_extension_validation.py::test_a_hand_unpacked_folder_is_named_too",
    {
        let tmp = common::TempDirectory::new("source-net-unpacked-name").expect("root");
        let ext = unpacked(tmp.path(), "probe");
        assert_eq!(put_defaults(tmp.path(), std::slice::from_ref(&ext)).0, 200);
        let (s, v) = json_http(tmp.path(), "GET", "/api/extensions/installed", Value::Null);
        assert_eq!(s, 200);
        assert_eq!(v["extensions"].as_array().expect("extensions").len(), 1);
        assert_eq!(v["extensions"][0]["name"], "probe");
        assert!(v["extensions"][0]["icon"].is_null());
    }
);
source_test!(
    c000_the_cli_config_refuses_what_the_api_refuses,
    "tests/test_extension_validation.py::test_the_cli_config_refuses_what_the_api_refuses[missing]",
    {
        invalid_extension_path("missing", "config");
    }
);
source_test!(
    c001_the_cli_config_refuses_what_the_api_refuses,
    "tests/test_extension_validation.py::test_the_cli_config_refuses_what_the_api_refuses[empty]",
    {
        invalid_extension_path("empty", "config");
    }
);
source_test!(
    c000_ext_default_add_refuses_what_the_api_refuses,
    "tests/test_extension_validation.py::test_ext_default_add_refuses_what_the_api_refuses[missing]",
    {
        invalid_extension_path("missing", "default-add");
    }
);
source_test!(
    c001_ext_default_add_refuses_what_the_api_refuses,
    "tests/test_extension_validation.py::test_ext_default_add_refuses_what_the_api_refuses[empty]",
    {
        invalid_extension_path("empty", "default-add");
    }
);
source_test!(
    c000_a_real_unpacked_extension_is_accepted_by_every_writer,
    "tests/test_extension_validation.py::test_a_real_unpacked_extension_is_accepted_by_every_writer",
    {
        let tmp = common::TempDirectory::new("source-net-every-extension-door").expect("root");
        let ext = unpacked(tmp.path(), "probe");
        let repo = repository(tmp.path());
        let installed = BTreeMap::new();
        let out = numera_core::cli::settings_commands::Settings {
            root: tmp.path(),
            repo: &repo,
            installed: &installed,
            environment_token: false,
        }
        .config(
            json!({"default_extension":[ext]})
                .as_object()
                .expect("arguments"),
            tmp.path(),
        )
        .expect("CLI config");
        assert_eq!(out.exit, 0);
        assert_eq!(stored_defaults(tmp.path()), vec![json!(ext)]);
        let out = numera_core::cli::extension_commands::defaults(
            tmp.path(),
            "default-add",
            Some(ext.to_str().expect("path")),
            tmp.path(),
        )
        .expect("default add");
        assert_eq!(out.exit, 0);
        assert_eq!(stored_defaults(tmp.path()), vec![json!(ext)]);
        assert_eq!(put_defaults(tmp.path(), std::slice::from_ref(&ext)).0, 200);
    }
);
source_test!(
    c000_removing_a_default_still_works_when_its_folder_is_gone,
    "tests/test_extension_validation.py::test_removing_a_default_still_works_when_its_folder_is_gone",
    {
        let tmp = common::TempDirectory::new("source-net-remove-gone-default").expect("root");
        let ext = unpacked(tmp.path(), "probe");
        assert_eq!(
            numera_core::cli::extension_commands::defaults(
                tmp.path(),
                "default-add",
                Some(ext.to_str().expect("path")),
                tmp.path()
            )
            .expect("default add")
            .exit,
            0
        );
        fs::remove_dir_all(&ext).expect("remove extension folder");
        assert_eq!(
            numera_core::cli::extension_commands::defaults(
                tmp.path(),
                "default-remove",
                Some(ext.to_str().expect("path")),
                tmp.path()
            )
            .expect("remove missing default")
            .exit,
            0
        );
        assert!(stored_defaults(tmp.path()).is_empty());
    }
);
