use super::*;
source_test!(
    generated_fingerprint_is_always_coherent,
    "tests/test_fingerprint.py::test_generated_fingerprint_is_always_coherent",
    {
        for seed in 0..200 {
            let fp = generate(seed, None, None);
            assert!(
                identity::validate(&fp).expect("validate").is_empty(),
                "seed {seed}"
            );
        }
    }
);
source_test!(
    seed_is_reproducible,
    "tests/test_fingerprint.py::test_seed_is_reproducible",
    {
        assert_eq!(
            serde_json::to_value(generate(42, None, None)).expect("serialize"),
            serde_json::to_value(generate(42, None, None)).expect("serialize")
        );
    }
);
source_test!(
    different_seeds_differ,
    "tests/test_fingerprint.py::test_different_seeds_differ",
    {
        assert_ne!(
            serde_json::to_value(generate(1, None, None)).expect("serialize"),
            serde_json::to_value(generate(2, None, None)).expect("serialize")
        );
    }
);
fn forced_os(os: &str) {
    let fp = generate(7, Some(os), None);
    let d = catalog();
    assert_eq!(fp.os, os);
    assert_eq!(json!(fp.platform), d["NAV_PLATFORM"][os]);
    assert!(
        d["WEBGL"][os]
            .as_array()
            .expect("GPU pool")
            .contains(&json!([fp.webgl_vendor, fp.webgl_renderer]))
    );
}
source_test!(
    forced_os_windows,
    "tests/test_fingerprint.py::test_forced_os_matches_platform_and_gpu[windows]",
    {
        forced_os("windows");
    }
);
source_test!(
    forced_os_macos,
    "tests/test_fingerprint.py::test_forced_os_matches_platform_and_gpu[macos]",
    {
        forced_os("macos");
    }
);
source_test!(
    forced_os_linux,
    "tests/test_fingerprint.py::test_forced_os_matches_platform_and_gpu[linux]",
    {
        forced_os("linux");
    }
);
source_test!(
    forced_os_android,
    "tests/test_fingerprint.py::test_forced_os_matches_platform_and_gpu[android]",
    {
        forced_os("android");
    }
);
source_test!(
    mobile_flag,
    "tests/test_fingerprint.py::test_mobile_flag",
    {
        let fp = fingerprint::generate(GenerateOptions {
            seed: 3,
            mobile: true,
            ..Default::default()
        })
        .expect("mobile generation");
        assert!(fp.is_mobile);
        assert_eq!(fp.os, "android");
        assert!(fp.user_agent.contains("Mobile"));
    }
);
source_test!(
    proxy_country_steers_locale,
    "tests/test_fingerprint.py::test_proxy_country_steers_locale",
    {
        let fp = fingerprint::generate(GenerateOptions {
            seed: 9,
            country: Some("PK"),
            ..Default::default()
        })
        .expect("country generation");
        assert_eq!(fp.locale, "en-PK");
        assert_eq!(fp.timezone, "Asia/Karachi");
    }
);
source_test!(
    explicit_locale_wins_over_proxy,
    "tests/test_fingerprint.py::test_explicit_locale_wins_over_proxy",
    {
        let fp = fingerprint::generate(GenerateOptions {
            seed: 9,
            country: Some("PK"),
            locale: Some("de-DE"),
            ..Default::default()
        })
        .expect("locale generation");
        assert_eq!(fp.locale, "de-DE");
        assert_eq!(fp.timezone, "Europe/Berlin");
    }
);
source_test!(
    viewport_never_exceeds_screen,
    "tests/test_fingerprint.py::test_viewport_never_exceeds_screen",
    {
        for seed in 0..100 {
            let fp = generate(seed, None, None);
            assert!(fp.viewport_width <= fp.screen_width);
            assert!(fp.viewport_height <= fp.screen_height);
        }
    }
);
source_test!(
    sec_ch_ua_contains_chrome_major,
    "tests/test_fingerprint.py::test_sec_ch_ua_contains_chrome_major",
    {
        let fp = generate(5, Some("windows"), None);
        assert!(identity::sec_ch_ua(&fp).contains(&format!(
            "\"Google Chrome\";v=\"{}\"",
            fp.chrome_version.split('.').next().expect("major")
        )));
    }
);
source_test!(
    validate_catches_tampering,
    "tests/test_fingerprint.py::test_validate_catches_tampering",
    {
        let mut fp = generate(11, Some("windows"), None);
        fp.platform = "MacIntel".into();
        assert!(
            identity::validate(&fp)
                .expect("validate")
                .iter()
                .any(|p| p.contains("platform"))
        );
    }
);
source_test!(
    fonts_come_from_the_os_font_set,
    "tests/test_fingerprint.py::test_fonts_come_from_the_os_font_set",
    {
        let d = catalog();
        for os in ["windows", "macos", "linux", "android"] {
            let fp = generate(3, Some(os), None);
            assert!(!fp.fonts.is_empty());
            assert!(fp.fonts.iter().all(|font| {
                d["FONTS"][os]
                    .as_array()
                    .expect("font pool")
                    .contains(&json!(font))
            }));
        }
    }
);
source_test!(
    validate_catches_a_font_from_another_os,
    "tests/test_fingerprint.py::test_validate_catches_a_font_from_another_os",
    {
        let mut fp = generate(12, Some("windows"), None);
        fp.fonts.push("Helvetica Neue".into());
        assert!(
            identity::validate(&fp)
                .expect("validate")
                .iter()
                .any(|p| p.contains("fonts"))
        );
    }
);
source_test!(
    phones_always_have_a_camera_and_microphone,
    "tests/test_fingerprint.py::test_phones_always_have_a_camera_and_microphone",
    {
        for seed in 0..30 {
            let fp = generate(seed, Some("android"), None);
            assert!(fp.cameras > 0 && fp.microphones > 0);
        }
    }
);
source_test!(
    validate_catches_a_phone_with_no_camera,
    "tests/test_fingerprint.py::test_validate_catches_a_phone_with_no_camera",
    {
        let mut fp = generate(4, Some("android"), None);
        fp.cameras = 0;
        assert!(
            identity::validate(&fp)
                .expect("validate")
                .iter()
                .any(|p| p.contains("camera"))
        );
    }
);
source_test!(
    desktops_may_have_no_webcam,
    "tests/test_fingerprint.py::test_desktops_may_have_no_webcam",
    {
        assert!((0..40).any(|seed| generate(seed, Some("windows"), None).cameras == 0));
    }
);
source_test!(
    secondary_signals_survive_a_round_trip,
    "tests/test_fingerprint.py::test_secondary_signals_survive_a_round_trip",
    {
        let fp = generate(8, Some("macos"), None);
        assert_eq!(
            Fingerprint::from_record(&serde_json::to_value(&fp).expect("serialize"))
                .expect("restore")
                .fonts,
            fp.fonts
        );
    }
);
source_test!(
    fingerprints_saved_before_these_fields_existed_still_load,
    "tests/test_fingerprint.py::test_fingerprints_saved_before_these_fields_existed_still_load",
    {
        let mut record = serde_json::to_value(generate(9, None, None)).expect("serialize");
        for field in ["fonts", "cameras", "microphones"] {
            record.as_object_mut().expect("record").remove(field);
        }
        let restored = Fingerprint::from_record(&record).expect("legacy fingerprint");
        assert!(restored.fonts.is_empty());
        assert_eq!(restored.cameras, 1);
    }
);
source_test!(
    webgl_plausible_accepts_real_host_gpus,
    "tests/test_fingerprint.py::test_webgl_plausible_accepts_real_host_gpus",
    {
        assert!(
            identity::webgl_plausible(
                "windows",
                "Google Inc. (NVIDIA)",
                "ANGLE (NVIDIA, NVIDIA GeForce RTX 3050 Ti Direct3D11 vs_5_0 ps_5_0, D3D11)"
            )
            .expect("GPU validation")
        );
        assert!(
            identity::webgl_plausible(
                "macos",
                "Google Inc. (Apple)",
                "ANGLE (Apple, Apple M3 Pro, OpenGL 4.1)"
            )
            .expect("GPU validation")
        );
    }
);
source_test!(
    webgl_plausible_rejects_wrong_os,
    "tests/test_fingerprint.py::test_webgl_plausible_rejects_wrong_os",
    {
        assert!(
            !identity::webgl_plausible(
                "windows",
                "Google Inc. (Apple)",
                "ANGLE (Apple, Apple M1, OpenGL 4.1)"
            )
            .expect("GPU validation")
        );
        assert!(
            !identity::webgl_plausible(
                "macos",
                "Google Inc. (NVIDIA)",
                "ANGLE (NVIDIA, RTX 3060 Direct3D11, D3D11)"
            )
            .expect("GPU validation")
        );
    }
);
source_test!(
    real_screen_size_is_coherent,
    "tests/test_fingerprint.py::test_real_screen_size_is_coherent",
    {
        let mut fp = generate(1, Some("windows"), None);
        fp.screen_width = 3440;
        fp.screen_height = 1440;
        fp.viewport_width = 3440;
        fp.viewport_height = 1400;
        assert!(
            !identity::validate(&fp)
                .expect("validate")
                .join(" ")
                .contains("screen resolution")
        );
    }
);
source_test!(
    italian_and_belgian_locales_are_coherent,
    "tests/test_fingerprint.py::test_italian_and_belgian_locales_are_coherent",
    {
        let d = catalog();
        for (cc, locale, zone) in [
            ("IT", "it-IT", "Europe/Rome"),
            ("BE", "nl-BE", "Europe/Brussels"),
        ] {
            assert_eq!(d["COUNTRY_TO_LOCALE"][cc], locale);
            assert_eq!(d["LOCALE_TO_COUNTRY"][locale], cc);
            assert!(d["COUNTRY_GEO"].get(cc).is_some());
            let fp = generate(3, None, Some(locale));
            assert_eq!(fp.timezone, zone);
            assert!(identity::validate(&fp).expect("validate").is_empty());
        }
    }
);
source_test!(
    every_locale_has_a_country_and_coordinates,
    "tests/test_fingerprint.py::test_every_locale_has_a_country_and_coordinates",
    {
        let d = catalog();
        for locale in d["LOCALES"].as_object().expect("locale catalog").keys() {
            let cc = d["LOCALE_TO_COUNTRY"][locale]
                .as_str()
                .expect("locale country");
            assert!(!cc.is_empty());
            assert!(d["COUNTRY_GEO"].get(cc).is_some());
            assert_eq!(d["COUNTRY_TO_LOCALE"][cc], locale.as_str());
        }
    }
);
