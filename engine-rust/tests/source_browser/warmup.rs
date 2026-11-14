use super::*;

source_test!(
    test_known_presets_exist,
    "tests/test_warmup.py::test_known_presets_exist",
    {
        for name in ["general", "ads", "ecommerce", "crypto", "social", "news"] {
            assert!(browser::warmup_policy::sites(Some(name)).len() >= 5);
        }
    }
);
source_test!(
    test_preset_sites_resolves_by_name,
    "tests/test_warmup.py::test_preset_sites_resolves_by_name",
    {
        let sites = browser::warmup_policy::sites(Some("crypto"));
        assert!(sites.iter().any(|s| s.contains("coingecko")));
        assert_eq!(
            sites,
            browser::warmup_policy::selected(Some(&json!("crypto")), None)
        );
    }
);
source_test!(
    test_preset_sites_is_case_insensitive,
    "tests/test_warmup.py::test_preset_sites_is_case_insensitive",
    {
        assert_eq!(
            browser::warmup_policy::sites(Some("ADS")),
            browser::warmup_policy::sites(Some("ads"))
        );
    }
);
source_test!(
    test_unknown_preset_falls_back_to_default,
    "tests/test_warmup.py::test_unknown_preset_falls_back_to_default",
    {
        assert_eq!(
            browser::warmup_policy::sites(Some("nonsense")),
            browser::warmup_policy::sites(None)
        );
        assert_eq!(
            browser::warmup_policy::sites(None),
            browser::warmup_policy::sites(Some("general"))
        );
    }
);
source_test!(
    test_preset_sites_returns_a_copy,
    "tests/test_warmup.py::test_preset_sites_returns_a_copy",
    {
        let mut got = browser::warmup_policy::sites(Some("news"));
        got.push("https://example.com/".into());
        assert!(
            !browser::warmup_policy::sites(Some("news")).contains(&"https://example.com/".into())
        );
    }
);
