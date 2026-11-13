use super::*;
fn catalog() -> Value {
    serde_json::from_str::<Value>(include_str!("../../src/device_catalog.json"))
        .expect("browser contract")["catalog"]
        .clone()
}
fn round_trip(f: &numera_core::models::Fingerprint) -> numera_core::models::Fingerprint {
    let mut p = profile();
    p.id = "rt0000000000".into();
    p.fingerprint = f.clone();
    conversion::to_engine(&conversion::to_dashboard(&p), 1000.0)
        .expect("browser contract")
        .fingerprint
}
source_test!(
    test_every_os_target_generates_coherently_windows,
    "tests/test_fingerprint_parity.py::test_every_os_target_generates_coherently[windows]",
    {
        for seed in 0..60 {
            let fp = fp("windows", seed);
            assert!(
                numera_core::identity::validate(&fp)
                    .expect("browser contract")
                    .is_empty(),
                "{seed}"
            );
        }
    }
);
source_test!(
    test_every_os_target_has_a_full_set_of_device_tables_windows,
    "tests/test_fingerprint_parity.py::test_every_os_target_has_a_full_set_of_device_tables[windows]",
    {
        for key in [
            "WEBGL",
            "SCREENS",
            "HARDWARE",
            "OS_UA_TOKEN",
            "NAV_PLATFORM",
            "CH_PLATFORM",
            "FONTS",
            "OS_VERSIONS",
            "TOUCH_POINTS",
            "PIXEL_RATIOS",
        ] {
            assert!(!catalog()[key]["windows"].is_null(), "{key}");
        }
    }
);
source_test!(
    test_no_parity_field_is_dropped_in_conversion_windows,
    "tests/test_fingerprint_parity.py::test_no_parity_field_is_dropped_in_conversion[windows]",
    {
        let f = fp("windows", 13);
        let back = round_trip(&f);
        let a = serde_json::to_value(&f).expect("browser contract");
        let b = serde_json::to_value(back).expect("browser contract");
        for key in [
            "client_rects",
            "webgl_image",
            "media_mode",
            "speakers",
            "os_version",
            "device_model",
            "max_touch_points",
            "device_pixel_ratio",
            "seed",
            "canvas",
            "audio",
            "font_mode",
            "geolocation",
            "do_not_track",
            "webrtc",
            "fonts",
            "cameras",
            "microphones",
            "device_memory",
        ] {
            assert_eq!(a[key], b[key], "{key}");
        }
    }
);
source_test!(
    test_every_os_target_generates_coherently_macos,
    "tests/test_fingerprint_parity.py::test_every_os_target_generates_coherently[macos]",
    {
        for seed in 0..60 {
            let fp = fp("macos", seed);
            assert!(
                numera_core::identity::validate(&fp)
                    .expect("browser contract")
                    .is_empty(),
                "{seed}"
            );
        }
    }
);
source_test!(
    test_every_os_target_has_a_full_set_of_device_tables_macos,
    "tests/test_fingerprint_parity.py::test_every_os_target_has_a_full_set_of_device_tables[macos]",
    {
        for key in [
            "WEBGL",
            "SCREENS",
            "HARDWARE",
            "OS_UA_TOKEN",
            "NAV_PLATFORM",
            "CH_PLATFORM",
            "FONTS",
            "OS_VERSIONS",
            "TOUCH_POINTS",
            "PIXEL_RATIOS",
        ] {
            assert!(!catalog()[key]["macos"].is_null(), "{key}");
        }
    }
);
source_test!(
    test_no_parity_field_is_dropped_in_conversion_macos,
    "tests/test_fingerprint_parity.py::test_no_parity_field_is_dropped_in_conversion[macos]",
    {
        let f = fp("macos", 13);
        let back = round_trip(&f);
        let a = serde_json::to_value(&f).expect("browser contract");
        let b = serde_json::to_value(back).expect("browser contract");
        for key in [
            "client_rects",
            "webgl_image",
            "media_mode",
            "speakers",
            "os_version",
            "device_model",
            "max_touch_points",
            "device_pixel_ratio",
            "seed",
            "canvas",
            "audio",
            "font_mode",
            "geolocation",
            "do_not_track",
            "webrtc",
            "fonts",
            "cameras",
            "microphones",
            "device_memory",
        ] {
            assert_eq!(a[key], b[key], "{key}");
        }
    }
);
source_test!(
    test_every_os_target_generates_coherently_linux,
    "tests/test_fingerprint_parity.py::test_every_os_target_generates_coherently[linux]",
    {
        for seed in 0..60 {
            let fp = fp("linux", seed);
            assert!(
                numera_core::identity::validate(&fp)
                    .expect("browser contract")
                    .is_empty(),
                "{seed}"
            );
        }
    }
);
source_test!(
    test_every_os_target_has_a_full_set_of_device_tables_linux,
    "tests/test_fingerprint_parity.py::test_every_os_target_has_a_full_set_of_device_tables[linux]",
    {
        for key in [
            "WEBGL",
            "SCREENS",
            "HARDWARE",
            "OS_UA_TOKEN",
            "NAV_PLATFORM",
            "CH_PLATFORM",
            "FONTS",
            "OS_VERSIONS",
            "TOUCH_POINTS",
            "PIXEL_RATIOS",
        ] {
            assert!(!catalog()[key]["linux"].is_null(), "{key}");
        }
    }
);
source_test!(
    test_no_parity_field_is_dropped_in_conversion_linux,
    "tests/test_fingerprint_parity.py::test_no_parity_field_is_dropped_in_conversion[linux]",
    {
        let f = fp("linux", 13);
        let back = round_trip(&f);
        let a = serde_json::to_value(&f).expect("browser contract");
        let b = serde_json::to_value(back).expect("browser contract");
        for key in [
            "client_rects",
            "webgl_image",
            "media_mode",
            "speakers",
            "os_version",
            "device_model",
            "max_touch_points",
            "device_pixel_ratio",
            "seed",
            "canvas",
            "audio",
            "font_mode",
            "geolocation",
            "do_not_track",
            "webrtc",
            "fonts",
            "cameras",
            "microphones",
            "device_memory",
        ] {
            assert_eq!(a[key], b[key], "{key}");
        }
    }
);
source_test!(
    test_every_os_target_generates_coherently_android,
    "tests/test_fingerprint_parity.py::test_every_os_target_generates_coherently[android]",
    {
        for seed in 0..60 {
            let fp = fp("android", seed);
            assert!(
                numera_core::identity::validate(&fp)
                    .expect("browser contract")
                    .is_empty(),
                "{seed}"
            );
        }
    }
);
source_test!(
    test_every_os_target_has_a_full_set_of_device_tables_android,
    "tests/test_fingerprint_parity.py::test_every_os_target_has_a_full_set_of_device_tables[android]",
    {
        for key in [
            "WEBGL",
            "SCREENS",
            "HARDWARE",
            "OS_UA_TOKEN",
            "NAV_PLATFORM",
            "CH_PLATFORM",
            "FONTS",
            "OS_VERSIONS",
            "TOUCH_POINTS",
            "PIXEL_RATIOS",
        ] {
            assert!(!catalog()[key]["android"].is_null(), "{key}");
        }
    }
);
source_test!(
    test_no_parity_field_is_dropped_in_conversion_android,
    "tests/test_fingerprint_parity.py::test_no_parity_field_is_dropped_in_conversion[android]",
    {
        let f = fp("android", 13);
        let back = round_trip(&f);
        let a = serde_json::to_value(&f).expect("browser contract");
        let b = serde_json::to_value(back).expect("browser contract");
        for key in [
            "client_rects",
            "webgl_image",
            "media_mode",
            "speakers",
            "os_version",
            "device_model",
            "max_touch_points",
            "device_pixel_ratio",
            "seed",
            "canvas",
            "audio",
            "font_mode",
            "geolocation",
            "do_not_track",
            "webrtc",
            "fonts",
            "cameras",
            "microphones",
            "device_memory",
        ] {
            assert_eq!(a[key], b[key], "{key}");
        }
    }
);
source_test!(
    test_every_os_target_generates_coherently_ios,
    "tests/test_fingerprint_parity.py::test_every_os_target_generates_coherently[ios]",
    {
        for seed in 0..60 {
            let fp = fp("ios", seed);
            assert!(
                numera_core::identity::validate(&fp)
                    .expect("browser contract")
                    .is_empty(),
                "{seed}"
            );
        }
    }
);
source_test!(
    test_every_os_target_has_a_full_set_of_device_tables_ios,
    "tests/test_fingerprint_parity.py::test_every_os_target_has_a_full_set_of_device_tables[ios]",
    {
        for key in [
            "WEBGL",
            "SCREENS",
            "HARDWARE",
            "OS_UA_TOKEN",
            "NAV_PLATFORM",
            "CH_PLATFORM",
            "FONTS",
            "OS_VERSIONS",
            "TOUCH_POINTS",
            "PIXEL_RATIOS",
        ] {
            assert!(!catalog()[key]["ios"].is_null(), "{key}");
        }
    }
);
source_test!(
    test_no_parity_field_is_dropped_in_conversion_ios,
    "tests/test_fingerprint_parity.py::test_no_parity_field_is_dropped_in_conversion[ios]",
    {
        let f = fp("ios", 13);
        let back = round_trip(&f);
        let a = serde_json::to_value(&f).expect("browser contract");
        let b = serde_json::to_value(back).expect("browser contract");
        for key in [
            "client_rects",
            "webgl_image",
            "media_mode",
            "speakers",
            "os_version",
            "device_model",
            "max_touch_points",
            "device_pixel_ratio",
            "seed",
            "canvas",
            "audio",
            "font_mode",
            "geolocation",
            "do_not_track",
            "webrtc",
            "fonts",
            "cameras",
            "microphones",
            "device_memory",
        ] {
            assert_eq!(a[key], b[key], "{key}");
        }
    }
);
source_test!(
    test_mobile_targets_are_flagged_mobile_and_touch_capable,
    "tests/test_fingerprint_parity.py::test_mobile_targets_are_flagged_mobile_and_touch_capable",
    {
        for os in ["android", "ios"] {
            let f = fp(os, 3);
            assert!(f.is_mobile);
            assert!(f.max_touch_points >= 1);
            assert!(f.device_pixel_ratio.as_f64().expect("browser contract") >= 2.0);
        }
    }
);
source_test!(
    test_ios_claims_the_webkit_shaped_surface,
    "tests/test_fingerprint_parity.py::test_ios_claims_the_webkit_shaped_surface",
    {
        let mut f = fp("ios", 3);
        assert!(f.user_agent.contains("CriOS/"));
        assert!(f.user_agent.contains("iPhone OS"));
        assert!(!f.user_agent.contains("Chrome/"));
        assert_eq!(f.device_memory, json!(0));
        f.device_memory = json!(8);
        assert!(
            !numera_core::identity::validate(&f)
                .expect("browser contract")
                .is_empty()
        );
    }
);
source_test!(
    test_desktop_targets_are_not_touch_phones,
    "tests/test_fingerprint_parity.py::test_desktop_targets_are_not_touch_phones",
    {
        for os in ["windows", "macos", "linux"] {
            let f = fp(os, 2);
            assert!(!f.is_mobile);
            assert!(f.device_memory.as_f64().expect("browser contract") > 0.0);
        }
    }
);
source_test!(
    test_weight_tables_stay_aligned_with_the_pools_they_weight,
    "tests/test_fingerprint_parity.py::test_weight_tables_stay_aligned_with_the_pools_they_weight",
    {
        let d = catalog();
        let w = &d["WEIGHTS"];
        for os in ["windows", "macos", "linux", "android", "ios"] {
            for (name, pool) in [
                ("webgl", &d["WEBGL"][os]),
                ("screens", &d["SCREENS"][os]),
                ("cores", &d["HARDWARE"][os]["cores"]),
                ("memory", &d["HARDWARE"][os]["memory"]),
                ("os_versions", &d["OS_VERSIONS"][os]),
                ("touch", &d["TOUCH_POINTS"][os]),
                ("pixel_ratios", &d["PIXEL_RATIOS"][os]),
            ] {
                assert_eq!(
                    w[name][os].as_array().expect("browser contract").len(),
                    pool.as_array().expect("browser contract").len(),
                    "{os}/{name}"
                );
            }
        }
        assert_eq!(
            w["chrome"].as_array().expect("browser contract").len(),
            d["CHROME_VERSIONS"]
                .as_array()
                .expect("browser contract")
                .len()
        );
        assert_eq!(
            w["android_models"]
                .as_array()
                .expect("browser contract")
                .len(),
            d["ANDROID_MODELS"]
                .as_array()
                .expect("browser contract")
                .len()
        );
        assert_eq!(
            w["os"]
                .as_object()
                .expect("browser contract")
                .keys()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>(),
            std::collections::BTreeSet::from(["windows".into(), "macos".into(), "linux".into()])
        );
    }
);
source_test!(
    test_os_choice_follows_desktop_market_share_not_a_coin_flip,
    "tests/test_fingerprint_parity.py::test_os_choice_follows_desktop_market_share_not_a_coin_flip",
    {
        let mut n = std::collections::BTreeMap::<String, usize>::new();
        for seed in 0..1500 {
            let f = numera_core::fingerprint::generate(numera_core::fingerprint::GenerateOptions {
                seed,
                ..Default::default()
            })
            .expect("browser contract");
            *n.entry(f.os).or_default() += 1;
        }
        assert!(n["windows"] > n["macos"] && n["macos"] > n["linux"]);
        assert!(n["windows"] > 900);
    }
);
source_test!(
    test_gpus_are_drawn_from_a_realistic_mix_not_uniformly,
    "tests/test_fingerprint_parity.py::test_gpus_are_drawn_from_a_realistic_mix_not_uniformly",
    {
        let mut n = std::collections::BTreeMap::<String, usize>::new();
        for seed in 0..1500 {
            *n.entry(fp("windows", seed).webgl_renderer).or_default() += 1;
        }
        assert!(
            n.iter()
                .max_by_key(|(_, n)| *n)
                .expect("browser contract")
                .0
                .contains("Intel")
        );
        assert!(*n.values().min().expect("browser contract") < 225);
    }
);
source_test!(
    test_phones_do_not_have_a_randomly_trimmed_font_list,
    "tests/test_fingerprint_parity.py::test_phones_do_not_have_a_randomly_trimmed_font_list",
    {
        for os in ["android", "ios"] {
            let mut expected = catalog()["FONTS"][os]
                .as_array()
                .expect("browser contract")
                .iter()
                .map(|v| v.as_str().expect("browser contract").to_owned())
                .collect::<Vec<_>>();
            expected.sort();
            for seed in [1, 2, 3] {
                assert_eq!(fp(os, seed).fonts, expected);
            }
        }
    }
);
source_test!(
    test_desktop_font_lists_do_vary_between_profiles,
    "tests/test_fingerprint_parity.py::test_desktop_font_lists_do_vary_between_profiles",
    {
        assert!(
            (0..20)
                .map(|seed| fp("windows", seed).fonts)
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                > 1
        );
    }
);
source_test!(
    test_the_header_and_the_brand_list_cannot_drift_apart,
    "tests/test_fingerprint_parity.py::test_the_header_and_the_brand_list_cannot_drift_apart",
    {
        let f = fp("windows", 4);
        let h = numera_core::identity::sec_ch_ua(&f);
        for b in numera_core::identity::ua_brands(&f)
            .as_array()
            .expect("browser contract")
        {
            assert!(h.contains(&format!(
                "\"{}\";v=\"{}\"",
                b["brand"].as_str().expect("browser contract"),
                b["version"].as_str().expect("browser contract")
            )));
        }
    }
);
source_test!(
    test_a_trimmed_font_list_survives_conversion,
    "tests/test_fingerprint_parity.py::test_a_trimmed_font_list_survives_conversion",
    {
        let mut f = fp("windows", 13);
        f.fonts = vec!["Arial".into(), "Georgia".into()];
        assert_eq!(round_trip(&f).fonts, f.fonts);
    }
);
source_test!(
    test_a_seedless_dashboard_profile_gets_the_same_seed_every_time,
    "tests/test_fingerprint_parity.py::test_a_seedless_dashboard_profile_gets_the_same_seed_every_time",
    {
        let v = json!({"id":"abc123def456","name":"p","os":"Windows"});
        let seeds = (0..5)
            .map(|_| {
                conversion::to_engine(&v, 1000.0)
                    .expect("browser contract")
                    .fingerprint
                    .seed
                    .to_string()
            })
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(seeds.len(), 1);
        assert!(!seeds.contains("0"));
    }
);
source_test!(
    test_two_seedless_profiles_do_not_share_a_seed,
    "tests/test_fingerprint_parity.py::test_two_seedless_profiles_do_not_share_a_seed",
    {
        let a = conversion::to_engine(
            &json!({"id":"aaaaaaaaaaaa","name":"a","os":"Windows"}),
            1000.0,
        )
        .expect("browser contract");
        let b = conversion::to_engine(
            &json!({"id":"bbbbbbbbbbbb","name":"b","os":"Windows"}),
            1000.0,
        )
        .expect("browser contract");
        assert_ne!(a.fingerprint.seed, b.fingerprint.seed);
    }
);
source_test!(
    test_an_explicit_seed_still_wins,
    "tests/test_fingerprint_parity.py::test_an_explicit_seed_still_wins",
    {
        assert_eq!(
            conversion::to_engine(
                &json!({"id":"abc123def456","name":"p","os":"Windows","seed":4242}),
                1000.0
            )
            .expect("browser contract")
            .fingerprint
            .seed,
            json!(4242).as_number().expect("browser contract").clone()
        );
    }
);
source_test!(
    test_ios_dashboard_profiles_convert_and_stay_coherent,
    "tests/test_fingerprint_parity.py::test_ios_dashboard_profiles_convert_and_stay_coherent",
    {
        let f=conversion::to_engine(&json!({"id":"iosiosiosios","name":"phone","os":"iOS","screen":"393x852","memory":8,"cores":4,"webglVendor":"Apple Inc.","webglRenderer":"Apple GPU","locale":"en-US","browserVersion":"128.0.0.0"}),1000.0).expect("browser contract").fingerprint;
        assert_eq!(f.os, "ios");
        assert!(f.is_mobile);
        assert_eq!(f.device_memory, json!(0));
        assert!(
            numera_core::identity::validate(&f)
                .expect("browser contract")
                .is_empty()
        );
    }
);
source_test!(
    test_unknown_dashboard_os_still_falls_back_to_windows,
    "tests/test_fingerprint_parity.py::test_unknown_dashboard_os_still_falls_back_to_windows",
    {
        assert_eq!(
            conversion::to_engine(&json!({"id":"x","name":"p","os":"Symbian"}), 1000.0)
                .expect("browser contract")
                .fingerprint
                .os,
            "windows"
        );
    }
);
