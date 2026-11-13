use super::*;
fn gologin() -> Value {
    serde_json::from_str(
        r#"{
  "name": "GL-Account-1",
  "os": "mac",
  "navigator": {
    "userAgent": "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Chrome/128.0.0.0 Safari/537.36",
    "resolution": "1440x900",
    "language": "en-US",
    "hardwareConcurrency": 10,
    "deviceMemory": 16,
    "platform": "MacIntel"
  },
  "timezone": {
    "timezone": "America/New_York"
  },
  "webGLMetadata": {
    "vendor": "Google Inc. (Apple)",
    "renderer": "ANGLE (Apple, Apple M2, OpenGL 4.1)"
  },
  "proxy": {
    "mode": "http",
    "host": "1.2.3.4",
    "port": 8080,
    "username": "u",
    "password": "p"
  }
}"#,
    )
    .expect("original vendor payload")
}
fn adspower() -> Value {
    serde_json::from_str(
        r#"{
  "name": "ADS-Store-7",
  "group_name": "Amazon",
  "user_agent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/127.0.0.0 Safari/537.36",
  "fingerprint_config": {
    "webgl_vendor": "Google Inc. (NVIDIA)",
    "webgl_renderer": "ANGLE (NVIDIA GeForce RTX 3060)"
  },
  "screen_resolution": "1920x1080",
  "timezone": "Europe/Berlin",
  "language": "de-DE",
  "user_proxy_config": {
    "proxy_type": "socks5",
    "proxy_host": "9.9.9.9",
    "proxy_port": "1080",
    "proxy_user": "x",
    "proxy_password": "y"
  }
}"#,
    )
    .expect("original vendor payload")
}
fn multilogin() -> Value {
    serde_json::from_str(
        r#"{
  "name": "ML-QA",
  "os_type": "linux",
  "navigator": {
    "user_agent": "Mozilla/5.0 (X11; Linux x86_64) Chrome/126.0.0.0 Safari/537.36",
    "resolution": "1920x1080",
    "hardware_concurrency": 8,
    "platform": "Linux x86_64"
  },
  "timezone": "Europe/London"
}"#,
    )
    .expect("original vendor payload")
}
fn gl_aliases() -> Value {
    serde_json::from_str(r#"{
  "name": "from gologin",
  "navigator": {
    "userAgent": "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.6099.130 Safari/537.36",
    "resolution": "1920x1080",
    "language": "de-DE",
    "hardwareConcurrency": 12,
    "deviceMemory": 32
  },
  "webGLMetadata": {
    "vendor": "Google Inc. (AMD)",
    "renderer": "ANGLE (AMD, Radeon RX 6700 XT Direct3D11 vs_5_0 ps_5_0, D3D11)"
  },
  "timezone": {
    "timezone": "Europe/Berlin"
  }
}"#).expect("original vendor payload")
}
fn ap_aliases() -> Value {
    serde_json::from_str(r#"{
  "name": "from adspower",
  "user_agent": "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/119.0.0.0 Safari/537.36",
  "webgl": {
    "vendor": "Google Inc. (Apple)",
    "renderer": "ANGLE (Apple, Apple M2, OpenGL 4.1)"
  },
  "screen_resolution": "2560x1600"
}"#).expect("original vendor payload")
}
const IPHONE_UA: &str = "Mozilla/5.0 (iPhone; CPU iPhone OS 17_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) CriOS/126.0.0.0 Mobile/15E148 Safari/604.1";
const IPAD_UA: &str = "Mozilla/5.0 (iPad; CPU OS 17_6 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.6 Mobile/15E148 Safari/604.1";
const ANDROID_UA: &str = "Mozilla/5.0 (Linux; Android 14; SM-S918B) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Mobile Safari/537.36";
fn imported(payload: Value) -> (Profile, Vec<String>) {
    importers::model(&payload, 7, 1000.0).expect("import vendor profile")
}
source_test!(
    gologin_core_fields,
    "tests/test_importers.py::test_gologin_core_fields",
    {
        let (p, _) = imported(gologin());
        assert_eq!(p.name, "GL-Account-1");
        assert_eq!(p.fingerprint.os, "macos");
        assert_eq!(
            (p.fingerprint.screen_width, p.fingerprint.screen_height),
            (1440, 900)
        );
        assert_eq!(p.fingerprint.hardware_concurrency, 10);
        let proxy = p.proxy.expect("imported proxy");
        assert_eq!(proxy.server, "http://1.2.3.4:8080");
        assert_eq!(proxy.to_record()["password"], "p");
    }
);
source_test!(
    adspower_nested_and_group_becomes_tag,
    "tests/test_importers.py::test_adspower_nested_and_group_becomes_tag",
    {
        let (p, _) = imported(adspower());
        assert_eq!(p.fingerprint.os, "windows");
        assert_eq!(p.fingerprint.timezone, "Europe/Berlin");
        assert_eq!(p.proxy.expect("proxy").server, "socks5://9.9.9.9:1080");
        assert!(p.tags.iter().any(|t| t == "Amazon"));
    }
);
source_test!(
    multilogin_without_proxy,
    "tests/test_importers.py::test_multilogin_without_proxy",
    {
        let (p, _) = imported(multilogin());
        assert_eq!(p.fingerprint.os, "linux");
        assert!(p.proxy.is_none());
    }
);
source_test!(
    os_inferred_from_user_agent_when_field_missing,
    "tests/test_importers.py::test_os_inferred_from_user_agent_when_field_missing",
    {
        let (p, _) = imported(
            json!({"name":"x","user_agent":"Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Chrome/128 Safari/537.36"}),
        );
        assert_eq!(p.fingerprint.os, "macos");
    }
);
source_test!(
    missing_fields_fall_back_to_a_coherent_base,
    "tests/test_importers.py::test_missing_fields_fall_back_to_a_coherent_base",
    {
        let (p, _) = imported(json!({"name":"sparse"}));
        assert!(!p.fingerprint.user_agent.is_empty());
        assert!(
            identity::validate(&p.fingerprint)
                .expect("coherence")
                .is_empty()
        );
    }
);
source_test!(
    unknown_locale_is_replaced_not_left_incoherent,
    "tests/test_importers.py::test_unknown_locale_is_replaced_not_left_incoherent",
    {
        let (_, issues) = imported(json!({"name":"x","language":"zz-ZZ"}));
        assert!(!issues.iter().any(|p| p == "languages do not match locale"));
        assert!(!issues.join(" ").contains("unknown locale"));
    }
);
source_test!(
    direct_proxy_mode_means_no_proxy,
    "tests/test_importers.py::test_direct_proxy_mode_means_no_proxy",
    {
        assert!(
            imported(json!({"name":"x","proxy":{"mode":"none","host":"1.1.1.1"}}))
                .0
                .proxy
                .is_none()
        );
    }
);
source_test!(
    unwrap_handles_wrappers,
    "tests/test_importers.py::test_unwrap_handles_wrappers",
    {
        assert_eq!(
            numera_core::import_format::entries(&json!({"data":[{"a":1},{"b":2}]})).len(),
            2
        );
        assert_eq!(
            numera_core::import_format::entries(&json!([{"a":1}])).len(),
            1
        );
        assert_eq!(
            numera_core::import_format::entries(&json!({"name":"single"})).len(),
            1
        );
        assert!(numera_core::import_format::entries(&json!("garbage")).is_empty());
    }
);
fn import_file(root: &std::path::Path, payload: Value) -> Value {
    use numera_core::cli::profile_commands::Profiles;
    let source = root.join("export.json");
    fs::write(&source, serde_json::to_vec(&payload).expect("export JSON")).expect("vendor file");
    let repo = ProfileRepo::new(root.join("store"), None);
    let config = ConfigStore::new(root.join("store"));
    let installed = BTreeMap::new();
    let commands = Profiles {
        repo: &repo,
        config: &config,
        installed: &installed,
        host: "source-domain-host",
    };
    let outcome = commands.import_from(&source, 1000.0).expect("import file");
    assert_eq!(outcome.exit, 0, "{}", outcome.payload);
    outcome.payload
}
source_test!(
    import_file_saves_and_reports,
    "tests/test_importers.py::test_import_file_saves_and_reports",
    {
        let temp = common::TempDirectory::new("source-vendor-file").expect("temporary root");
        let outcome = import_file(temp.path(), json!([gologin(), adspower(), multilogin()]));
        assert_eq!(outcome["imported"], 3);
        assert_eq!(outcome["skipped"], json!([]));
        let repo = ProfileRepo::new(temp.path().join("store"), None);
        assert_eq!(names(&repo), ["ADS-Store-7", "GL-Account-1", "ML-QA"]);
    }
);
source_test!(
    the_chrome_version_in_the_user_agent_is_kept,
    "tests/test_importers.py::test_the_chrome_version_in_the_user_agent_is_kept",
    {
        let fp = imported(gl_aliases()).0.fingerprint;
        assert!(fp.chrome_version.starts_with("120."));
        assert!(fp.user_agent.contains("Chrome/120."));
    }
);
source_test!(
    a_vendor_specific_webgl_path_is_not_dropped,
    "tests/test_importers.py::test_a_vendor_specific_webgl_path_is_not_dropped",
    {
        let fp = imported(gl_aliases()).0.fingerprint;
        assert!(fp.webgl_renderer.contains("AMD"));
        assert_eq!(fp.webgl_vendor, "Google Inc. (AMD)");
        assert!(
            imported(ap_aliases())
                .0
                .fingerprint
                .webgl_renderer
                .contains("Apple M2")
        );
    }
);
source_test!(
    a_snake_case_user_agent_under_navigator_is_found,
    "tests/test_importers.py::test_a_snake_case_user_agent_under_navigator_is_found",
    {
        assert!(imported(json!({"name":"x","navigator":{"user_agent":gl_aliases()["navigator"]["userAgent"]}})).0.fingerprint.user_agent.contains("Chrome/120."));
    }
);
source_test!(
    an_imported_profile_is_coherent,
    "tests/test_importers.py::test_an_imported_profile_is_coherent",
    {
        for payload in [gl_aliases(), ap_aliases()] {
            let fp = imported(payload).0.fingerprint;
            assert!(identity::validate(&fp).expect("coherence").is_empty());
        }
    }
);
source_test!(
    a_mobile_import_does_not_get_desktop_browser_chrome,
    "tests/test_importers.py::test_a_mobile_import_does_not_get_desktop_browser_chrome",
    {
        let fp = imported(
            json!({"name":"m","navigator":{"userAgent":ANDROID_UA,"resolution":"412x915"}}),
        )
        .0
        .fingerprint;
        assert!(fp.is_mobile);
        assert_eq!(fp.viewport_height, 915 - 80);
    }
);
source_test!(
    a_bare_ios_export_is_not_imported_as_windows,
    "tests/test_importers.py::test_a_bare_ios_export_is_not_imported_as_windows",
    {
        let fp = imported(json!({"name":"ip","os":"iOS"})).0.fingerprint;
        assert_eq!(fp.os, "ios");
        assert_eq!(fp.platform, "iPhone");
    }
);
source_test!(
    an_iphone_user_agent_is_not_imported_as_a_mac,
    "tests/test_importers.py::test_an_iphone_user_agent_is_not_imported_as_a_mac",
    {
        let fp = imported(json!({"name":"i","user_agent":IPHONE_UA,"screen_resolution":"390x844"}))
            .0
            .fingerprint;
        assert_eq!(fp.os, "ios");
        assert!(fp.is_mobile);
        assert_eq!(fp.viewport_height, 844 - 113);
    }
);
source_test!(
    an_ipad_export_is_not_imported_as_a_desktop_mac,
    "tests/test_importers.py::test_an_ipad_export_is_not_imported_as_a_desktop_mac",
    {
        assert_eq!(
            imported(json!({"name":"t","user_agent":IPAD_UA}))
                .0
                .fingerprint
                .os,
            "ios"
        );
    }
);
source_test!(
    a_conflicting_os_field_does_not_turn_an_iphone_into_a_desktop,
    "tests/test_importers.py::test_a_conflicting_os_field_does_not_turn_an_iphone_into_a_desktop",
    {
        assert_eq!(
            imported(json!({"name":"c","os":"Windows","user_agent":IPHONE_UA}))
                .0
                .fingerprint
                .os,
            "ios"
        );
        assert_eq!(imported(json!({"name":"c","os":"iOS","user_agent":"Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/120.0.0.0 Safari/537.36"})).0.fingerprint.os,"ios");
    }
);
source_test!(
    the_desktop_and_android_classifications_still_hold,
    "tests/test_importers.py::test_the_desktop_and_android_classifications_still_hold",
    {
        for (mut payload, expected) in [
            (json!({"os":"win"}), "windows"),
            (
                json!({"user_agent":"Mozilla/5.0 (Windows NT 10.0; Win64; x64) Chrome/120 Safari/537.36"}),
                "windows",
            ),
            (json!({"os":"mac"}), "macos"),
            (json!({"os":"Darwin"}), "macos"),
            (
                json!({"user_agent":"Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) Chrome/128 Safari/537.36"}),
                "macos",
            ),
            (json!({"os_type":"linux"}), "linux"),
            (json!({"os":"android"}), "android"),
            (json!({"user_agent":ANDROID_UA}), "android"),
            (
                json!({"navigator":{"platform":"Linux armv8l","userAgent":ANDROID_UA}}),
                "android",
            ),
            (json!({}), "windows"),
        ] {
            payload["name"] = "x".into();
            assert_eq!(imported(payload).0.fingerprint.os, expected);
        }
    }
);
source_test!(
    an_imported_phone_is_a_coherent_phone,
    "tests/test_importers.py::test_an_imported_phone_is_a_coherent_phone",
    {
        for payload in [
            json!({"name":"a","user_agent":ANDROID_UA,"screen_resolution":"412x915"}),
            json!({"name":"i","os":"iOS","screen_resolution":"390x844"}),
        ] {
            let fp = imported(payload).0.fingerprint;
            assert!(fp.is_mobile);
            assert!(fp.max_touch_points >= 1);
            assert!(identity::validate(&fp).expect("coherence").is_empty());
        }
    }
);
source_test!(
    a_non_finite_number_falls_back_instead_of_raising,
    "tests/test_importers.py::test_a_non_finite_number_falls_back_instead_of_raising",
    {
        // Drive the private hardware parser through the public import seam with a
        // generated eight-core base, preserving the original fallback of eight.
        let seed = (0..100)
            .find(|s| generate(*s, Some("windows"), None).hardware_concurrency == 8)
            .expect("eight core base");
        for junk in [
            json!("inf"),
            json!("-inf"),
            json!("Infinity"),
            json!("1e309"),
            json!("nan"),
            json!("9".repeat(400)),
            json!("-5"),
            json!(true),
            json!("8 cores"),
            json!("N/A"),
        ] {
            let fp = importers::profile(&json!({"name":"x","os":"win","cores":junk}), seed, 1000.0)
                .expect("tolerant import")
                .0;
            assert_eq!(fp["cores"], 8, "{junk}");
        }
        for (sent, expected) in [(json!("12"), 12), (json!("8.9"), 8), (json!(0), 0)] {
            assert_eq!(
                importers::profile(&json!({"name":"x","os":"win","cores":sent}), seed, 1000.0)
                    .expect("numeric import")
                    .0["cores"],
                expected
            );
        }
    }
);
source_test!(
    an_infinite_core_count_yields_a_plausible_profile,
    "tests/test_importers.py::test_an_infinite_core_count_yields_a_plausible_profile",
    {
        let fp=imported(json!({"name":"x","os":"win","navigator":{"hardwareConcurrency":"1e309","deviceMemory":"inf"}})).0.fingerprint;
        let d = catalog();
        assert!(
            d["HARDWARE"]["windows"]["cores"]
                .as_array()
                .expect("cores pool")
                .contains(&json!(fp.hardware_concurrency))
        );
        assert!(
            d["HARDWARE"]["windows"]["memory"]
                .as_array()
                .expect("memory pool")
                .contains(&fp.device_memory)
        );
    }
);
source_test!(
    one_impossible_number_does_not_cost_the_whole_file,
    "tests/test_importers.py::test_one_impossible_number_does_not_cost_the_whole_file",
    {
        let temp = common::TempDirectory::new("source-vendor-bad-number").expect("temporary root");
        let outcome = import_file(
            temp.path(),
            json!([gologin(),{"name":"odd","cores":"1e309"},multilogin()]),
        );
        assert_eq!(outcome["imported"], 3);
        assert_eq!(outcome["skipped"], json!([]));
        assert_eq!(
            names(&ProfileRepo::new(temp.path().join("store"), None)).len(),
            3
        );
    }
);
source_test!(
    the_api_import_survives_the_same_number,
    "tests/test_importers.py::test_the_api_import_survives_the_same_number",
    {
        let temp = common::TempDirectory::new("source-http-import-number").expect("temporary root");
        let payload =
            json!([{"name":"first"},{"name":"second","navigator":{"deviceMemory":"1e309"}}]);
        let (status, response) = http(
            temp.path(),
            "POST",
            "/api/profiles/import",
            json!({"text":payload.to_string()}),
        );
        assert_eq!(status, 200, "{response}");
        assert_eq!(response["imported"], 2);
        let (status, rows) = http(temp.path(), "GET", "/api/profiles", Value::Null);
        assert_eq!(status, 200);
        let names = rows
            .as_array()
            .expect("profile list")
            .iter()
            .map(|p| p["name"].as_str().expect("name"))
            .collect::<BTreeSet<_>>();
        assert_eq!(names, BTreeSet::from(["first", "second"]));
    }
);
source_test!(
    a_timezone_nested_under_id_is_read_not_refused,
    "tests/test_importers.py::test_a_timezone_nested_under_id_is_read_not_refused",
    {
        let temp =
            common::TempDirectory::new("source-http-import-timezone").expect("temporary root");
        let payload = json!([{"name":"by-id","navigator":{"language":"de-DE"},"timezone":{"id":"Europe/Berlin"}},
        {"name":"by-nesting","navigator":{"language":"en-US"},"timezone":{"timezone":"America/Chicago"}},
        {"name":"plain-string","navigator":{"language":"en-US"},"timezone":"America/Los_Angeles"}]);
        let (status, response) = http(
            temp.path(),
            "POST",
            "/api/profiles/import",
            json!({"text":payload.to_string()}),
        );
        assert_eq!(status, 200, "{response}");
        assert_eq!(response["imported"], 3);
        let (status, rows) = http(temp.path(), "GET", "/api/profiles", Value::Null);
        assert_eq!(status, 200);
        for (name, zone) in [
            ("by-id", "Europe/Berlin"),
            ("by-nesting", "America/Chicago"),
            ("plain-string", "America/Los_Angeles"),
        ] {
            let row = rows
                .as_array()
                .expect("profiles")
                .iter()
                .find(|p| p["name"] == name)
                .expect("imported name");
            assert_eq!(row["timezone"], zone);
        }
    }
);
source_test!(
    an_object_naming_no_zone_is_still_refused,
    "tests/test_importers.py::test_an_object_naming_no_zone_is_still_refused",
    {
        let temp = common::TempDirectory::new("source-http-import-invalid-timezone")
            .expect("temporary root");
        let payload = json!([{"name":"ok","timezone":{"whatever":true}}]);
        let (status, response) = http(
            temp.path(),
            "POST",
            "/api/profiles/import",
            json!({"text":payload.to_string()}),
        );
        assert!((400..500).contains(&status), "{status}: {response}");
        assert!(response.to_string().contains("timezone"));
        let (status, rows) = http(temp.path(), "GET", "/api/profiles", Value::Null);
        assert_eq!(status, 200);
        assert_eq!(rows, json!([]));
        assert!(json_files(temp.path(), "profiles").is_empty());
    }
);
