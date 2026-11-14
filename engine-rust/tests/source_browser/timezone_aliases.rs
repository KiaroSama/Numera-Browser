use super::*;
fn same(a: Option<&str>, b: Option<&str>) -> bool {
    let mut f = fp("windows", 1);
    f.timezone = b.unwrap_or("").into();
    let seen = json!({"webdriver":false,"webdriverOwn":false,"hasChrome":true,"plugins":5,"userAgent":f.user_agent,"platform":f.platform,"languages":f.languages,"cores":f.hardware_concurrency,"memory":f.device_memory,"timezone":a,"toStringNative":true,"permissionsAgree":true,"canvasStable":true,"webglRenderer":f.webgl_renderer});
    let report = browser::trust_grades::grade(&f, &seen, "playwright").expect("browser contract");
    report["checks"]
        .as_array()
        .expect("browser contract")
        .iter()
        .find(|v| v["name"] == "Timezone matches the locale")
        .expect("browser contract")["ok"]
        == true
}
source_test!(
    test_two_names_for_one_zone_match_asia_kolkata_asia_calcutta,
    "tests/test_timezone_aliases.py::test_two_names_for_one_zone_match[Asia/Kolkata-Asia/Calcutta]",
    {
        assert!(same(Some("Asia/Kolkata"), Some("Asia/Calcutta")));
    }
);
source_test!(
    test_two_names_for_one_zone_match_asia_calcutta_asia_kolkata,
    "tests/test_timezone_aliases.py::test_two_names_for_one_zone_match[Asia/Calcutta-Asia/Kolkata]",
    {
        assert!(same(Some("Asia/Calcutta"), Some("Asia/Kolkata")));
    }
);
source_test!(
    test_two_names_for_one_zone_match_europe_kyiv_europe_kiev,
    "tests/test_timezone_aliases.py::test_two_names_for_one_zone_match[Europe/Kyiv-Europe/Kiev]",
    {
        assert!(same(Some("Europe/Kyiv"), Some("Europe/Kiev")));
    }
);
source_test!(
    test_two_names_for_one_zone_match_america_nuuk_america_godthab,
    "tests/test_timezone_aliases.py::test_two_names_for_one_zone_match[America/Nuuk-America/Godthab]",
    {
        assert!(same(Some("America/Nuuk"), Some("America/Godthab")));
    }
);
source_test!(
    test_two_names_for_one_zone_match_asia_ho_chi_minh_asia_saigon,
    "tests/test_timezone_aliases.py::test_two_names_for_one_zone_match[Asia/Ho_Chi_Minh-Asia/Saigon]",
    {
        assert!(same(Some("Asia/Ho_Chi_Minh"), Some("Asia/Saigon")));
    }
);
source_test!(
    test_two_names_for_one_zone_match_pacific_chuuk_pacific_truk,
    "tests/test_timezone_aliases.py::test_two_names_for_one_zone_match[Pacific/Chuuk-Pacific/Truk]",
    {
        assert!(same(Some("Pacific/Chuuk"), Some("Pacific/Truk")));
    }
);
source_test!(
    test_two_names_for_one_zone_match_america_argentina_buenos_aires_america_buenos_aires,
    "tests/test_timezone_aliases.py::test_two_names_for_one_zone_match[America/Argentina/Buenos_Aires-America/Buenos_Aires]",
    {
        assert!(same(
            Some("America/Argentina/Buenos_Aires"),
            Some("America/Buenos_Aires")
        ));
    }
);
source_test!(
    test_the_same_name_matches_itself,
    "tests/test_timezone_aliases.py::test_the_same_name_matches_itself",
    {
        assert!(same(Some("Europe/Berlin"), Some("Europe/Berlin")));
    }
);
source_test!(
    test_case_does_not_decide_it,
    "tests/test_timezone_aliases.py::test_case_does_not_decide_it",
    {
        assert!(same(Some("asia/kolkata"), Some("Asia/Calcutta")));
        assert!(same(Some("  Asia/Kolkata  "), Some("Asia/Calcutta")));
    }
);
source_test!(
    test_different_zones_do_not_match_europe_london_europe_lisbon,
    "tests/test_timezone_aliases.py::test_different_zones_do_not_match[Europe/London-Europe/Lisbon]",
    {
        assert!(!same(Some("Europe/London"), Some("Europe/Lisbon")));
    }
);
source_test!(
    test_different_zones_do_not_match_europe_berlin_europe_paris,
    "tests/test_timezone_aliases.py::test_different_zones_do_not_match[Europe/Berlin-Europe/Paris]",
    {
        assert!(!same(Some("Europe/Berlin"), Some("Europe/Paris")));
    }
);
source_test!(
    test_different_zones_do_not_match_asia_kolkata_asia_colombo,
    "tests/test_timezone_aliases.py::test_different_zones_do_not_match[Asia/Kolkata-Asia/Colombo]",
    {
        assert!(!same(Some("Asia/Kolkata"), Some("Asia/Colombo")));
    }
);
source_test!(
    test_different_zones_do_not_match_europe_london_asia_kolkata,
    "tests/test_timezone_aliases.py::test_different_zones_do_not_match[Europe/London-Asia/Kolkata]",
    {
        assert!(!same(Some("Europe/London"), Some("Asia/Kolkata")));
    }
);
source_test!(
    test_a_missing_name_is_never_a_match_empty_first,
    "tests/test_timezone_aliases.py::test_a_missing_name_is_never_a_match[-Asia/Kolkata]",
    {
        assert!(!same(Some(""), Some("Asia/Kolkata")));
    }
);
source_test!(
    test_a_missing_name_is_never_a_match_empty_second,
    "tests/test_timezone_aliases.py::test_a_missing_name_is_never_a_match[Asia/Kolkata-]",
    {
        assert!(!same(Some("Asia/Kolkata"), Some("")));
    }
);
source_test!(
    test_a_missing_name_is_never_a_match_none_asia_kolkata,
    "tests/test_timezone_aliases.py::test_a_missing_name_is_never_a_match[None-Asia/Kolkata]",
    {
        assert!(!same(None, Some("Asia/Kolkata")));
    }
);
source_test!(
    test_a_missing_name_is_never_a_match_asia_kolkata_none,
    "tests/test_timezone_aliases.py::test_a_missing_name_is_never_a_match[Asia/Kolkata-None]",
    {
        assert!(!same(Some("Asia/Kolkata"), None));
    }
);
source_test!(
    test_a_missing_name_is_never_a_match_none_none,
    "tests/test_timezone_aliases.py::test_a_missing_name_is_never_a_match[None-None]",
    {
        assert!(!same(None, None));
    }
);
source_test!(
    test_the_table_is_symmetric,
    "tests/test_timezone_aliases.py::test_the_table_is_symmetric",
    {
        let d: Value = serde_json::from_str(include_str!("../../src/device_catalog.json"))
            .expect("browser contract");
        let aliases = d["catalog"]["TZ_ALIASES"]
            .as_object()
            .expect("browser contract");
        assert!(!aliases.is_empty());
        for (a, b) in aliases {
            let b = b.as_str().expect("browser contract");
            assert_eq!(
                aliases[&b.to_lowercase()]
                    .as_str()
                    .expect("browser contract")
                    .to_lowercase(),
                *a
            );
            assert!(same(Some(a), Some(b)));
        }
    }
);
source_test!(
    test_the_probe_compares_with_the_alias_rule,
    "tests/test_timezone_aliases.py::test_the_probe_compares_with_the_alias_rule",
    {
        assert!(same(Some("Asia/Calcutta"), Some("Asia/Kolkata")));
        let mut p = profile();
        p.fingerprint.timezone = "Asia/Kolkata".into();
        let empty = json!({"checks":[]});
        let r = browser::network_grades::proxy(
            &p,
            &json!({"ip":"8.8.8.8","country":"IN","timezone":"Asia/Calcutta"}),
            &empty,
            &empty,
            &empty,
            0,
        )
        .expect("browser contract");
        assert!(
            r["checks"]
                .as_array()
                .expect("browser contract")
                .iter()
                .any(|c| c["name"] == "Timezone matches the exit location" && c["ok"] == true)
        );
    }
);
