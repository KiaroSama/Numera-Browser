use super::*;
source_test!(
    c000_default_mode_is_altered,
    "tests/test_webrtc.py::test_default_mode_is_altered",
    {
        assert_eq!(generated().webrtc, "altered");
    }
);
source_test!(
    c000_profiles_saved_before_the_field_existed_still_load,
    "tests/test_webrtc.py::test_profiles_saved_before_the_field_existed_still_load",
    {
        let mut old = serde_json::to_value(generated()).expect("fingerprint record");
        old.as_object_mut().expect("record").remove("webrtc");
        assert_eq!(
            Fingerprint::from_record(&old)
                .expect("old fingerprint")
                .webrtc,
            "altered"
        );
    }
);
source_test!(
    c000_mode_reaches_the_init_script,
    "tests/test_webrtc.py::test_mode_reaches_the_init_script",
    {
        for mode in ["altered", "disabled", "real"] {
            let mut fp = generated();
            fp.webrtc = mode.into();
            let script = identity_script::build(&fp).expect("identity script");
            assert!(script.contains(&format!("\"webrtc\": \"{mode}\"")));
        }
    }
);
source_test!(
    c000_real_mode_leaves_webrtc_alone,
    "tests/test_webrtc.py::test_real_mode_leaves_webrtc_alone",
    {
        let mut fp = generated();
        fp.webrtc = "real".into();
        let script = identity_script::build(&fp).expect("identity script");
        assert!(script.contains("cfg.webrtc === 'disabled'"));
        assert!(script.contains("cfg.webrtc !== 'real'"));
    }
);
source_test!(
    c000_candidate_filter_keeps_private_and_mdns,
    "tests/test_webrtc.py::test_candidate_filter_keeps_private_and_mdns",
    {
        let script = identity_script::build(&generated()).expect("identity script");
        assert!(script.contains(".local"));
        assert!(script.contains(r"192\.168\."));
        assert!(script.contains("a=candidate:"));
    }
);
source_test!(
    c000_dashboard_choice_survives_the_round_trip,
    "tests/test_webrtc.py::test_dashboard_choice_survives_the_round_trip",
    {
        for (shown, stored) in [
            ("Altered", "altered"),
            ("Disabled", "disabled"),
            ("Real", "real"),
        ] {
            let p=conversion::to_engine(&json!({"id":"x","name":"n","os":"Windows","screen":"1920x1080","cores":8,"memory":16,"locale":"en-US","timezone":"America/New_York","webrtc":shown,"webglRenderer":"ANGLE (NVIDIA GeForce RTX 3060 Direct3D11)"}),1000.0).expect("dashboard profile");
            assert_eq!(p.fingerprint.webrtc, stored);
            assert_eq!(conversion::to_dashboard(&p)["webrtc"], shown);
        }
    }
);
