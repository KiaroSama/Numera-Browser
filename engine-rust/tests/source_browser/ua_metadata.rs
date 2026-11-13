use super::*;

source_test!(
    test_a_profile_whose_browser_has_no_client_hints_sends_none,
    "tests/test_ua_metadata.py::test_a_profile_whose_browser_has_no_client_hints_sends_none",
    {
        let f = fp("ios", 1);
        assert!(!numera_core::identity::sends_client_hints(&f));
        assert_eq!(
            numera_core::identity::headers(&f).expect("browser contract"),
            json!({"low":{},"high":{}})
        );
        assert_eq!(
            numera_core::identity::metadata(&f).expect("browser contract")["platform"],
            "iOS"
        );
    }
);
source_test!(
    test_the_header_view_is_the_metadata_view_windows,
    "tests/test_ua_metadata.py::test_the_header_view_is_the_metadata_view[windows]",
    {
        let f = fp("windows", 1);
        let m = numera_core::identity::metadata(&f).expect("browser contract");
        let h = numera_core::identity::headers(&f).expect("browser contract");
        for (section, key, source) in [
            ("low", "sec-ch-ua-platform", "platform"),
            ("high", "sec-ch-ua-arch", "architecture"),
            ("high", "sec-ch-ua-model", "model"),
            ("high", "sec-ch-ua-platform-version", "platformVersion"),
            ("high", "sec-ch-ua-bitness", "bitness"),
        ] {
            assert_eq!(
                h[section][key],
                json!(format!(
                    "\"{}\"",
                    m[source].as_str().expect("browser contract")
                ))
            );
        }
        assert_eq!(
            h["low"]["sec-ch-ua-mobile"],
            if m["mobile"] == true { "?1" } else { "?0" }
        );
        assert_eq!(
            h["high"]["sec-ch-ua-wow64"],
            if m["wow64"] == true { "?1" } else { "?0" }
        );
    }
);
source_test!(
    test_the_metadata_is_shaped_the_way_cdp_wants_it_windows,
    "tests/test_ua_metadata.py::test_the_metadata_is_shaped_the_way_cdp_wants_it[windows]",
    {
        let m = numera_core::identity::metadata(&fp("windows", 1)).expect("browser contract");
        assert_eq!(
            m.as_object()
                .expect("browser contract")
                .keys()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>(),
            [
                "brands",
                "fullVersionList",
                "platform",
                "platformVersion",
                "architecture",
                "model",
                "mobile",
                "bitness",
                "wow64"
            ]
            .into_iter()
            .map(str::to_owned)
            .collect()
        );
        assert!(m["mobile"].is_boolean() && m["wow64"].is_boolean());
        for k in [
            "platform",
            "platformVersion",
            "architecture",
            "model",
            "bitness",
        ] {
            assert!(m[k].is_string());
        }
        for key in ["brands", "fullVersionList"] {
            for b in m[key].as_array().expect("browser contract") {
                assert_eq!(b.as_object().expect("browser contract").len(), 2);
                assert!(b["brand"].is_string() && b["version"].is_string());
            }
        }
        for k in ["platform", "platformVersion", "architecture"] {
            assert!(!m[k].as_str().expect("browser contract").contains('"'));
        }
    }
);
source_test!(
    test_the_header_view_is_the_metadata_view_macos,
    "tests/test_ua_metadata.py::test_the_header_view_is_the_metadata_view[macos]",
    {
        let f = fp("macos", 1);
        let m = numera_core::identity::metadata(&f).expect("browser contract");
        let h = numera_core::identity::headers(&f).expect("browser contract");
        for (section, key, source) in [
            ("low", "sec-ch-ua-platform", "platform"),
            ("high", "sec-ch-ua-arch", "architecture"),
            ("high", "sec-ch-ua-model", "model"),
            ("high", "sec-ch-ua-platform-version", "platformVersion"),
            ("high", "sec-ch-ua-bitness", "bitness"),
        ] {
            assert_eq!(
                h[section][key],
                json!(format!(
                    "\"{}\"",
                    m[source].as_str().expect("browser contract")
                ))
            );
        }
        assert_eq!(
            h["low"]["sec-ch-ua-mobile"],
            if m["mobile"] == true { "?1" } else { "?0" }
        );
        assert_eq!(
            h["high"]["sec-ch-ua-wow64"],
            if m["wow64"] == true { "?1" } else { "?0" }
        );
    }
);
source_test!(
    test_the_metadata_is_shaped_the_way_cdp_wants_it_macos,
    "tests/test_ua_metadata.py::test_the_metadata_is_shaped_the_way_cdp_wants_it[macos]",
    {
        let m = numera_core::identity::metadata(&fp("macos", 1)).expect("browser contract");
        assert_eq!(
            m.as_object()
                .expect("browser contract")
                .keys()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>(),
            [
                "brands",
                "fullVersionList",
                "platform",
                "platformVersion",
                "architecture",
                "model",
                "mobile",
                "bitness",
                "wow64"
            ]
            .into_iter()
            .map(str::to_owned)
            .collect()
        );
        assert!(m["mobile"].is_boolean() && m["wow64"].is_boolean());
        for k in [
            "platform",
            "platformVersion",
            "architecture",
            "model",
            "bitness",
        ] {
            assert!(m[k].is_string());
        }
        for key in ["brands", "fullVersionList"] {
            for b in m[key].as_array().expect("browser contract") {
                assert_eq!(b.as_object().expect("browser contract").len(), 2);
                assert!(b["brand"].is_string() && b["version"].is_string());
            }
        }
        for k in ["platform", "platformVersion", "architecture"] {
            assert!(!m[k].as_str().expect("browser contract").contains('"'));
        }
    }
);
source_test!(
    test_the_header_view_is_the_metadata_view_linux,
    "tests/test_ua_metadata.py::test_the_header_view_is_the_metadata_view[linux]",
    {
        let f = fp("linux", 1);
        let m = numera_core::identity::metadata(&f).expect("browser contract");
        let h = numera_core::identity::headers(&f).expect("browser contract");
        for (section, key, source) in [
            ("low", "sec-ch-ua-platform", "platform"),
            ("high", "sec-ch-ua-arch", "architecture"),
            ("high", "sec-ch-ua-model", "model"),
            ("high", "sec-ch-ua-platform-version", "platformVersion"),
            ("high", "sec-ch-ua-bitness", "bitness"),
        ] {
            assert_eq!(
                h[section][key],
                json!(format!(
                    "\"{}\"",
                    m[source].as_str().expect("browser contract")
                ))
            );
        }
        assert_eq!(
            h["low"]["sec-ch-ua-mobile"],
            if m["mobile"] == true { "?1" } else { "?0" }
        );
        assert_eq!(
            h["high"]["sec-ch-ua-wow64"],
            if m["wow64"] == true { "?1" } else { "?0" }
        );
    }
);
source_test!(
    test_the_metadata_is_shaped_the_way_cdp_wants_it_linux,
    "tests/test_ua_metadata.py::test_the_metadata_is_shaped_the_way_cdp_wants_it[linux]",
    {
        let m = numera_core::identity::metadata(&fp("linux", 1)).expect("browser contract");
        assert_eq!(
            m.as_object()
                .expect("browser contract")
                .keys()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>(),
            [
                "brands",
                "fullVersionList",
                "platform",
                "platformVersion",
                "architecture",
                "model",
                "mobile",
                "bitness",
                "wow64"
            ]
            .into_iter()
            .map(str::to_owned)
            .collect()
        );
        assert!(m["mobile"].is_boolean() && m["wow64"].is_boolean());
        for k in [
            "platform",
            "platformVersion",
            "architecture",
            "model",
            "bitness",
        ] {
            assert!(m[k].is_string());
        }
        for key in ["brands", "fullVersionList"] {
            for b in m[key].as_array().expect("browser contract") {
                assert_eq!(b.as_object().expect("browser contract").len(), 2);
                assert!(b["brand"].is_string() && b["version"].is_string());
            }
        }
        for k in ["platform", "platformVersion", "architecture"] {
            assert!(!m[k].as_str().expect("browser contract").contains('"'));
        }
    }
);
source_test!(
    test_the_header_view_is_the_metadata_view_android,
    "tests/test_ua_metadata.py::test_the_header_view_is_the_metadata_view[android]",
    {
        let f = fp("android", 1);
        let m = numera_core::identity::metadata(&f).expect("browser contract");
        let h = numera_core::identity::headers(&f).expect("browser contract");
        for (section, key, source) in [
            ("low", "sec-ch-ua-platform", "platform"),
            ("high", "sec-ch-ua-arch", "architecture"),
            ("high", "sec-ch-ua-model", "model"),
            ("high", "sec-ch-ua-platform-version", "platformVersion"),
            ("high", "sec-ch-ua-bitness", "bitness"),
        ] {
            assert_eq!(
                h[section][key],
                json!(format!(
                    "\"{}\"",
                    m[source].as_str().expect("browser contract")
                ))
            );
        }
        assert_eq!(
            h["low"]["sec-ch-ua-mobile"],
            if m["mobile"] == true { "?1" } else { "?0" }
        );
        assert_eq!(
            h["high"]["sec-ch-ua-wow64"],
            if m["wow64"] == true { "?1" } else { "?0" }
        );
    }
);
source_test!(
    test_the_metadata_is_shaped_the_way_cdp_wants_it_android,
    "tests/test_ua_metadata.py::test_the_metadata_is_shaped_the_way_cdp_wants_it[android]",
    {
        let m = numera_core::identity::metadata(&fp("android", 1)).expect("browser contract");
        assert_eq!(
            m.as_object()
                .expect("browser contract")
                .keys()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>(),
            [
                "brands",
                "fullVersionList",
                "platform",
                "platformVersion",
                "architecture",
                "model",
                "mobile",
                "bitness",
                "wow64"
            ]
            .into_iter()
            .map(str::to_owned)
            .collect()
        );
        assert!(m["mobile"].is_boolean() && m["wow64"].is_boolean());
        for k in [
            "platform",
            "platformVersion",
            "architecture",
            "model",
            "bitness",
        ] {
            assert!(m[k].is_string());
        }
        for key in ["brands", "fullVersionList"] {
            for b in m[key].as_array().expect("browser contract") {
                assert_eq!(b.as_object().expect("browser contract").len(), 2);
                assert!(b["brand"].is_string() && b["version"].is_string());
            }
        }
        for k in ["platform", "platformVersion", "architecture"] {
            assert!(!m[k].as_str().expect("browser contract").contains('"'));
        }
    }
);
source_test!(
    test_the_metadata_is_shaped_the_way_cdp_wants_it_ios,
    "tests/test_ua_metadata.py::test_the_metadata_is_shaped_the_way_cdp_wants_it[ios]",
    {
        let m = numera_core::identity::metadata(&fp("ios", 1)).expect("browser contract");
        assert_eq!(
            m.as_object()
                .expect("browser contract")
                .keys()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>(),
            [
                "brands",
                "fullVersionList",
                "platform",
                "platformVersion",
                "architecture",
                "model",
                "mobile",
                "bitness",
                "wow64"
            ]
            .into_iter()
            .map(str::to_owned)
            .collect()
        );
        assert!(m["mobile"].is_boolean() && m["wow64"].is_boolean());
        for k in [
            "platform",
            "platformVersion",
            "architecture",
            "model",
            "bitness",
        ] {
            assert!(m[k].is_string());
        }
        for key in ["brands", "fullVersionList"] {
            for b in m[key].as_array().expect("browser contract") {
                assert_eq!(b.as_object().expect("browser contract").len(), 2);
                assert!(b["brand"].is_string() && b["version"].is_string());
            }
        }
        for k in ["platform", "platformVersion", "architecture"] {
            assert!(!m[k].as_str().expect("browser contract").contains('"'));
        }
    }
);
source_test!(
    test_the_full_version_list_carries_four_part_versions,
    "tests/test_ua_metadata.py::test_the_full_version_list_carries_four_part_versions",
    {
        let m = numera_core::identity::metadata(&fp("windows", 1)).expect("browser contract");
        for b in m["fullVersionList"].as_array().expect("browser contract") {
            assert_eq!(
                b["version"]
                    .as_str()
                    .expect("browser contract")
                    .matches('.')
                    .count(),
                3
            );
        }
        for b in m["brands"].as_array().expect("browser contract") {
            assert!(
                !b["version"]
                    .as_str()
                    .expect("browser contract")
                    .contains('.')
            );
        }
    }
);
source_test!(
    test_apple_silicon_is_arm_and_an_intel_mac_is_not,
    "tests/test_ua_metadata.py::test_apple_silicon_is_arm_and_an_intel_mac_is_not",
    {
        let mut f = fp("macos", 1);
        f.webgl_renderer = "ANGLE (Apple, Apple M3 Pro, OpenGL 4.1)".into();
        assert_eq!(
            numera_core::identity::metadata(&f).expect("browser contract")["architecture"],
            "arm"
        );
        f.webgl_renderer = "ANGLE (Intel, Intel(R) Iris(TM) Plus Graphics, OpenGL 4.1)".into();
        assert_eq!(
            numera_core::identity::metadata(&f).expect("browser contract")["architecture"],
            "x86"
        );
    }
);
source_test!(
    test_the_platform_version_matches_the_shared_encoding,
    "tests/test_ua_metadata.py::test_the_platform_version_matches_the_shared_encoding",
    {
        let mut f = fp("windows", 1);
        f.os_version = "11".into();
        assert_eq!(
            numera_core::identity::metadata(&f).expect("browser contract")["platformVersion"],
            numera_core::identity::platform_version("windows", "11").expect("browser contract")
        );
    }
);
source_test!(
    test_a_phone_reports_its_handset_and_a_desktop_reports_none,
    "tests/test_ua_metadata.py::test_a_phone_reports_its_handset_and_a_desktop_reports_none",
    {
        assert!(
            !numera_core::identity::metadata(&fp("android", 1)).expect("browser contract")["model"]
                .as_str()
                .expect("browser contract")
                .is_empty()
        );
        assert_eq!(
            numera_core::identity::metadata(&fp("windows", 1)).expect("browser contract")["model"],
            ""
        );
    }
);
