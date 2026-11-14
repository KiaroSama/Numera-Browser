use super::*;
source_test!(
    c000_id_is_found_in_every_shape_of_link,
    "tests/test_webstore.py::test_id_is_found_in_every_shape_of_link[ddkjiahejlhfcafbddmgiahcphecmpfh]",
    {
        assert_eq!(
            extension_catalog::extension_id("ddkjiahejlhfcafbddmgiahcphecmpfh")
                .expect("extension ID"),
            "ddkjiahejlhfcafbddmgiahcphecmpfh"
        );
    }
);
source_test!(
    c001_id_is_found_in_every_shape_of_link,
    "tests/test_webstore.py::test_id_is_found_in_every_shape_of_link[https://chromewebstore.google.com/detail/ublock-origin-lite/ddkjiahejlhfcafbddmgiahcphecmpfh]",
    {
        assert_eq!(extension_catalog::extension_id("https://chromewebstore.google.com/detail/ublock-origin-lite/ddkjiahejlhfcafbddmgiahcphecmpfh").expect("extension ID"),"ddkjiahejlhfcafbddmgiahcphecmpfh");
    }
);
source_test!(
    c002_id_is_found_in_every_shape_of_link,
    "tests/test_webstore.py::test_id_is_found_in_every_shape_of_link[https://chrome.google.com/webstore/detail/x/ddkjiahejlhfcafbddmgiahcphecmpfh?hl=en]",
    {
        assert_eq!(
            extension_catalog::extension_id(
                "https://chrome.google.com/webstore/detail/x/ddkjiahejlhfcafbddmgiahcphecmpfh?hl=en"
            )
            .expect("extension ID"),
            "ddkjiahejlhfcafbddmgiahcphecmpfh"
        );
    }
);
source_test!(
    c003_id_is_found_in_every_shape_of_link,
    "tests/test_webstore.py::test_id_is_found_in_every_shape_of_link[  https://chromewebstore.google.com/detail/ublock/ddkjiahejlhfcafbddmgiahcphecmpfh/reviews  ]",
    {
        assert_eq!(extension_catalog::extension_id("  https://chromewebstore.google.com/detail/ublock/ddkjiahejlhfcafbddmgiahcphecmpfh/reviews  ").expect("extension ID"),"ddkjiahejlhfcafbddmgiahcphecmpfh");
    }
);
source_test!(
    c000_download_refuses_invalid_ids_before_constructing_a_request,
    "tests/test_webstore.py::test_download_refuses_invalid_ids_before_constructing_a_request[../outside]",
    {
        let error = network::extension_download::url("../outside")
            .expect_err("invalid ID before request construction");
        assert!(
            error.to_string().contains("invalid Chrome extension id"),
            "{error}"
        );
    }
);
source_test!(
    c001_download_refuses_invalid_ids_before_constructing_a_request,
    "tests/test_webstore.py::test_download_refuses_invalid_ids_before_constructing_a_request[aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa]",
    {
        let error = network::extension_download::url("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa")
            .expect_err("invalid ID before request construction");
        assert!(
            error.to_string().contains("invalid Chrome extension id"),
            "{error}"
        );
    }
);
source_test!(
    c002_download_refuses_invalid_ids_before_constructing_a_request,
    "tests/test_webstore.py::test_download_refuses_invalid_ids_before_constructing_a_request[aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\\n]",
    {
        let error = network::extension_download::url("aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n")
            .expect_err("invalid ID before request construction");
        assert!(
            error.to_string().contains("invalid Chrome extension id"),
            "{error}"
        );
    }
);
source_test!(
    c003_download_refuses_invalid_ids_before_constructing_a_request,
    "tests/test_webstore.py::test_download_refuses_invalid_ids_before_constructing_a_request[AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA]",
    {
        let error = network::extension_download::url("AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA")
            .expect_err("invalid ID before request construction");
        assert!(
            error.to_string().contains("invalid Chrome extension id"),
            "{error}"
        );
    }
);
source_test!(
    c000_install_refuses_linked_extension_before_network_or_replacement,
    "tests/test_webstore.py::test_install_refuses_linked_extension_before_network_or_replacement",
    {
        let tmp = common::TempDirectory::new("source-net-linked-install").expect("root");
        let outside = tmp.path().join("outside");
        fs::create_dir_all(&outside).expect("outside");
        let original = br#"{"name":"Preserve"}"#;
        fs::write(outside.join("manifest.json"), original).expect("preserved manifest");
        let id = "a".repeat(32);
        let dest = tmp.path().join("extensions").join(&id);
        fs::create_dir_all(dest.parent().expect("parent")).expect("extensions");
        link_directory(&outside, &dest);
        let error = extension_install::install(tmp.path(), &id, true, |_| {
            panic!("linked installation reached download")
        })
        .err()
        .expect("linked target refused");
        assert!(error.to_string().contains("linked"), "{error}");
        assert_eq!(
            fs::read(outside.join("manifest.json")).expect("original bytes"),
            original
        );
    }
);
source_test!(
    c000_a_truncated_id_says_so,
    "tests/test_webstore.py::test_a_truncated_id_says_so",
    {
        let error = extension_catalog::extension_id(
            "https://chromewebstore.google.com/detail/x/ddkjiahejlhfcafbddmgiahcphecmpf",
        )
        .expect_err("truncated ID");
        assert!(error.to_string().contains("31 characters"));
    }
);
source_test!(
    c000_a_letter_past_p_says_which_one,
    "tests/test_webstore.py::test_a_letter_past_p_says_which_one",
    {
        let error = extension_catalog::extension_id(
            "https://chromewebstore.google.com/detail/buster/mpbjkejclgfqadimmefqebjfooflfhl",
        )
        .expect_err("invalid ID alphabet");
        assert!(error.to_string().contains("contains q"));
    }
);
source_test!(
    c000_empty_input,
    "tests/test_webstore.py::test_empty_input",
    {
        assert!(extension_catalog::extension_id("").is_err());
    }
);
source_test!(
    c000_unpack_strips_the_header,
    "tests/test_webstore.py::test_unpack_strips_the_header",
    {
        let tmp = common::TempDirectory::new("source-net-unpack").expect("root");
        let package = crx3(&zip_bytes(&[(
            "manifest.json",
            br#"{"name":"Test","version":"1"}"#,
        )]));
        extension_package::unpack(&package, tmp.path(), true).expect("CRX extraction");
        assert!(tmp.path().join("manifest.json").is_file());
        tmp.cleanup().expect("owned extraction cleanup");
    }
);
source_test!(
    c000_unpack_refuses_to_escape_the_folder,
    "tests/test_webstore.py::test_unpack_refuses_to_escape_the_folder",
    {
        let tmp = common::TempDirectory::new("source-net-zip-escape").expect("root");
        let package = crx3(&zip_bytes(&[
            ("../escaped.js", b"x"),
            ("manifest.json", b"{}"),
        ]));
        let error = extension_package::unpack(&package, &tmp.path().join("ext"), true)
            .expect_err("traversal refused");
        assert!(error.to_string().contains("outside the folder"), "{error}");
        assert!(!tmp.path().join("escaped.js").exists());
        assert!(!tmp.path().join("ext/manifest.json").exists());
    }
);
source_test!(
    c000_unpack_rejects_an_unknown_crx_version,
    "tests/test_webstore.py::test_unpack_rejects_an_unknown_crx_version",
    {
        let tmp = common::TempDirectory::new("source-net-crx-version").expect("root");
        let mut bytes = b"Cr24".to_vec();
        bytes.extend_from_slice(&9u32.to_le_bytes());
        bytes.extend_from_slice(&[0; 8]);
        assert!(
            extension_package::unpack(&bytes, tmp.path(), true)
                .expect_err("version refused")
                .to_string()
                .contains("unsupported CRX version")
        );
    }
);
source_test!(
    c000_name_falls_back_to_the_locale_file,
    "tests/test_webstore.py::test_name_falls_back_to_the_locale_file[english-first]",
    {
        locale_name(&["en", "en_US", "fr"], "English");
    }
);
source_test!(
    c001_name_falls_back_to_the_locale_file,
    "tests/test_webstore.py::test_name_falls_back_to_the_locale_file[regional-fallback]",
    {
        locale_name(&["en_US", "fr"], "Regional");
    }
);
source_test!(
    c002_name_falls_back_to_the_locale_file,
    "tests/test_webstore.py::test_name_falls_back_to_the_locale_file[default-fallback]",
    {
        locale_name(&["fr"], "Default");
    }
);
source_test!(
    c000_description_refuses_locale_paths_outside_the_extension,
    "tests/test_webstore.py::test_description_refuses_locale_paths_outside_the_extension[traversal]",
    {
        unsafe_locale(json!("traversal"));
    }
);
source_test!(
    c001_description_refuses_locale_paths_outside_the_extension,
    "tests/test_webstore.py::test_description_refuses_locale_paths_outside_the_extension[absolute]",
    {
        unsafe_locale(json!("absolute"));
    }
);
source_test!(
    c002_description_refuses_locale_paths_outside_the_extension,
    "tests/test_webstore.py::test_description_refuses_locale_paths_outside_the_extension[..]",
    {
        unsafe_locale(json!(".."));
    }
);
source_test!(
    c003_description_refuses_locale_paths_outside_the_extension,
    "tests/test_webstore.py::test_description_refuses_locale_paths_outside_the_extension[.]",
    {
        unsafe_locale(json!("."));
    }
);
source_test!(
    c004_description_refuses_locale_paths_outside_the_extension,
    "tests/test_webstore.py::test_description_refuses_locale_paths_outside_the_extension[]",
    {
        unsafe_locale(json!(""));
    }
);
source_test!(
    c005_description_refuses_locale_paths_outside_the_extension,
    "tests/test_webstore.py::test_description_refuses_locale_paths_outside_the_extension[en/fr]",
    {
        unsafe_locale(json!("en/fr"));
    }
);
source_test!(
    c006_description_refuses_locale_paths_outside_the_extension,
    "tests/test_webstore.py::test_description_refuses_locale_paths_outside_the_extension[en\\\\fr]",
    {
        unsafe_locale(json!("en\\fr"));
    }
);
source_test!(
    c007_description_refuses_locale_paths_outside_the_extension,
    "tests/test_webstore.py::test_description_refuses_locale_paths_outside_the_extension[C:outside]",
    {
        unsafe_locale(json!("C:outside"));
    }
);
source_test!(
    c008_description_refuses_locale_paths_outside_the_extension,
    "tests/test_webstore.py::test_description_refuses_locale_paths_outside_the_extension[None]",
    {
        unsafe_locale(Value::Null);
    }
);
source_test!(
    c009_description_refuses_locale_paths_outside_the_extension,
    "tests/test_webstore.py::test_description_refuses_locale_paths_outside_the_extension[42]",
    {
        unsafe_locale(json!(42));
    }
);
source_test!(
    c000_description_refuses_a_locale_catalog_linked_outside_the_extension,
    "tests/test_webstore.py::test_description_refuses_a_locale_catalog_linked_outside_the_extension",
    {
        use numera_core::vault::testing::{Action, with_observer};
        let tmp = common::TempDirectory::new("source-net-linked-catalog").expect("root");
        let ext = tmp.path().join("ext");
        write_ext(
            &ext,
            json!({"name":"__MSG_extName__","default_locale":"en"}),
            &[],
        );
        fs::create_dir_all(ext.join("_locales")).expect("locales");
        let outside = tmp.path().join("outside");
        fs::create_dir_all(&outside).expect("outside");
        let payload = br#"{"extName":{"message":"Outside catalog must not be returned"}}"#;
        fs::write(outside.join("messages.json"), payload).expect("catalog bytes");
        link_directory(&outside, &ext.join("_locales").join("en"));
        let description = with_observer(
            |e| {
                if e.operation == "before-extension-json-read"
                    && e.path.file_name().is_some_and(|n| n == "messages.json")
                {
                    return Err(io::Error::other("linked outside catalog reached reading"));
                }
                Ok(Action::Continue)
            },
            || extension_catalog::describe(&ext),
        )
        .expect("outside catalog refused before read");
        assert_eq!(description.name, "ext");
        assert_eq!(
            fs::read(outside.join("messages.json")).expect("outside bytes"),
            payload
        );
    }
);
source_test!(
    c000_name_survives_a_broken_manifest,
    "tests/test_webstore.py::test_name_survives_a_broken_manifest",
    {
        let tmp = common::TempDirectory::new("source-net-broken-manifest").expect("root");
        let ext = tmp.path().join("abcd");
        fs::create_dir(&ext).expect("extension");
        fs::write(ext.join("manifest.json"), b"{not json").expect("broken bytes");
        assert_eq!(
            extension_catalog::describe(&ext)
                .expect("description fallback")
                .name,
            "abcd"
        );
    }
);
source_test!(
    c000_installed_lists_only_real_extensions,
    "tests/test_webstore.py::test_installed_lists_only_real_extensions",
    {
        let tmp = common::TempDirectory::new("source-net-installed").expect("root");
        let ext = tmp.path().join("extensions").join("a".repeat(32));
        write_ext(&ext, json!({"name":"Good"}), &[]);
        fs::create_dir_all(tmp.path().join("extensions/leftover")).expect("leftover");
        let installed = extension_catalog::installed(tmp.path()).expect("installed");
        assert_eq!(installed.len(), 1);
        assert_eq!(installed[0].description.name, "Good");
    }
);
source_test!(
    c000_uninstall_removes_the_folder,
    "tests/test_webstore.py::test_uninstall_removes_the_folder",
    {
        let tmp = common::TempDirectory::new("source-net-uninstall").expect("root");
        let id = "a".repeat(32);
        let ext = tmp.path().join("extensions").join(&id);
        write_ext(&ext, json!({}), &[]);
        assert!(extension_install::uninstall(tmp.path(), &id).expect("uninstall"));
        assert!(!ext.exists());
        assert!(!extension_install::uninstall(tmp.path(), &id).expect("idempotent uninstall"));
    }
);
source_test!(
    c000_icon_picks_the_largest_declared_size,
    "tests/test_webstore.py::test_icon_picks_the_largest_declared_size",
    {
        let tmp = common::TempDirectory::new("source-net-icon").expect("root");
        let ext = tmp.path().join("e");
        write_ext(&ext,serde_json::from_str("{\"name\": \"X\", \"icons\": {\"16\": \"s.png\", \"128\": \"l.png\", \"48\": \"m.png\"}}").expect("manifest"),&[("s.png",b"s"),("m.png",b"m"),("l.png",b"l")]);
        assert_eq!(
            extension_catalog::describe(&ext)
                .expect("description")
                .icon
                .expect("selected icon")
                .file_name()
                .expect("filename"),
            "l.png"
        );
    }
);
source_test!(
    c000_icon_skips_a_size_whose_file_is_missing,
    "tests/test_webstore.py::test_icon_skips_a_size_whose_file_is_missing",
    {
        let tmp = common::TempDirectory::new("source-net-icon").expect("root");
        let ext = tmp.path().join("e");
        write_ext(
            &ext,
            serde_json::from_str(
                "{\"name\": \"X\", \"icons\": {\"128\": \"gone.png\", \"48\": \"here.png\"}}",
            )
            .expect("manifest"),
            &[("here.png", b"h")],
        );
        assert_eq!(
            extension_catalog::describe(&ext)
                .expect("description")
                .icon
                .expect("selected icon")
                .file_name()
                .expect("filename"),
            "here.png"
        );
    }
);
source_test!(
    c000_icon_falls_back_to_the_action_icon,
    "tests/test_webstore.py::test_icon_falls_back_to_the_action_icon",
    {
        let tmp = common::TempDirectory::new("source-net-icon").expect("root");
        let ext = tmp.path().join("e");
        write_ext(
            &ext,
            serde_json::from_str(
                "{\"name\": \"X\", \"action\": {\"default_icon\": {\"32\": \"a.png\"}}}",
            )
            .expect("manifest"),
            &[("a.png", b"a")],
        );
        assert_eq!(
            extension_catalog::describe(&ext)
                .expect("description")
                .icon
                .expect("selected icon")
                .file_name()
                .expect("filename"),
            "a.png"
        );
    }
);
source_test!(
    c000_icon_accepts_a_plain_string_action_icon,
    "tests/test_webstore.py::test_icon_accepts_a_plain_string_action_icon",
    {
        let tmp = common::TempDirectory::new("source-net-icon").expect("root");
        let ext = tmp.path().join("e");
        write_ext(
            &ext,
            serde_json::from_str(
                "{\"name\": \"X\", \"browser_action\": {\"default_icon\": \"only.png\"}}",
            )
            .expect("manifest"),
            &[("only.png", b"o")],
        );
        assert_eq!(
            extension_catalog::describe(&ext)
                .expect("description")
                .icon
                .expect("selected icon")
                .file_name()
                .expect("filename"),
            "only.png"
        );
    }
);
source_test!(
    c000_icon_refuses_a_path_that_escapes_the_folder,
    "tests/test_webstore.py::test_icon_refuses_a_path_that_escapes_the_folder",
    {
        let tmp = common::TempDirectory::new("source-net-icon-escape").expect("root");
        fs::write(tmp.path().join("secret.png"), b"nope").expect("outside icon");
        let ext = tmp.path().join("e");
        write_ext(
            &ext,
            json!({"name":"X","icons":{"128":"../secret.png"}}),
            &[],
        );
        assert!(
            extension_catalog::describe(&ext)
                .expect("description")
                .icon
                .is_none()
        );
        assert_eq!(
            fs::read(tmp.path().join("secret.png")).expect("protected icon"),
            b"nope"
        );
    }
);
source_test!(
    c000_icon_is_none_when_nothing_is_declared,
    "tests/test_webstore.py::test_icon_is_none_when_nothing_is_declared",
    {
        let tmp = common::TempDirectory::new("source-net-no-icon").expect("root");
        let ext = tmp.path().join("e");
        write_ext(&ext, json!({"name":"X"}), &[]);
        assert!(
            extension_catalog::describe(&ext)
                .expect("description")
                .icon
                .is_none()
        );
    }
);
source_test!(
    c000_describe_carries_name_and_icon,
    "tests/test_webstore.py::test_describe_carries_name_and_icon",
    {
        let tmp = common::TempDirectory::new("source-net-description").expect("root");
        let ext = tmp.path().join("e");
        write_ext(
            &ext,
            json!({"name":"Real Name","icons":{"48":"i.png"}}),
            &[("i.png", b"i")],
        );
        let d = extension_catalog::describe(&ext).expect("description");
        assert_eq!(d.name, "Real Name");
        assert!(d.icon.expect("icon").ends_with("i.png"));
        assert_eq!(d.path, ext);
    }
);
