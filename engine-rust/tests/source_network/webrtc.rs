use super::*;
#[path = "../../../browser-page-rust/src/webrtc_rules.rs"]
mod rules;
fn observations() -> Value {
    use sha2::{Digest, Sha256};
    let raw = include_bytes!("../fixtures/browser-page-webrtc.json");
    assert_eq!(
        Sha256::digest(raw)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>(),
        "7570de3bb939f841b185f72c2e51f8a3b5b1ca4bd41a716e999ec0586936f11c"
    );
    serde_json::from_slice(raw).expect("immutable original WebRTC observations")
}
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
            let encoded = script
                .strip_prefix("(() => {\nconst CFG = ")
                .and_then(|script| script.split_once(";\n(").map(|(encoded, _)| encoded))
                .expect("initialization configuration boundary");
            let delivered: Value = serde_json::from_str(encoded).expect("delivered configuration");
            assert_eq!(delivered["webrtc"], mode);
            assert_eq!(
                rules::mode(delivered["webrtc"].as_str().expect("mode")),
                match mode {
                    "disabled" => rules::Mode::Disabled,
                    "real" => rules::Mode::Real,
                    _ => rules::Mode::Altered,
                }
            );
        }
    }
);
source_test!(
    c000_real_mode_leaves_webrtc_alone,
    "tests/test_webrtc.py::test_real_mode_leaves_webrtc_alone",
    {
        let mut fp = generated();
        fp.webrtc = "real".into();
        assert_eq!(
            identity_script::config(&fp).expect("configuration")["webrtc"],
            "real"
        );
        // The actual browser wrapper branches on this shared pure selector:
        // real returns before replacing listeners or SDP descriptors.
        assert_eq!(rules::mode(&fp.webrtc), rules::Mode::Real);
        assert_ne!(rules::mode("disabled"), rules::Mode::Real);
        assert_eq!(rules::mode("altered"), rules::Mode::Altered);
        assert_eq!(rules::mode(""), rules::Mode::Altered);
        for event in observations()["end_events"].as_array().expect("end events") {
            let line = event["event"]["candidate"]["candidate"].as_str();
            assert_eq!(
                rules::drop_candidate(line),
                event["drop"].as_bool().expect("drop result")
            );
        }
    }
);
source_test!(
    c000_candidate_filter_keeps_private_and_mdns,
    "tests/test_webrtc.py::test_candidate_filter_keeps_private_and_mdns",
    {
        let frozen = observations();
        assert_eq!(
            frozen["source_commit"],
            "13b6d9c6973281fe123353eaec05b29d65781a2d"
        );
        for case in frozen["candidate_cases"]
            .as_array()
            .expect("candidate observations")
        {
            let line = case["line"].as_str().expect("candidate line");
            assert_eq!(
                rules::candidate_address(line),
                case["address"].as_str(),
                "candidate grammar {line:?}"
            );
            assert_eq!(
                rules::safe_candidate(line),
                case["safe"].as_bool().expect("safe result"),
                "candidate policy {line:?}"
            );
            assert_eq!(
                rules::drop_candidate(Some(line)),
                case["drop"].as_bool().expect("drop result"),
                "event policy {line:?}"
            );
        }
        for case in frozen["sdp_cases"].as_array().expect("SDP observations") {
            assert_eq!(
                rules::scrub_sdp(case["input"].as_str().expect("SDP input")),
                case["output"].as_str().expect("SDP result")
            );
        }
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
