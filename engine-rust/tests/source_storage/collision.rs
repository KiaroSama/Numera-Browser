use super::*;
source_test!(
    unique_profiles_dont_collide,
    "tests/test_collision.py::test_unique_profiles_dont_collide",
    {
        let a = profile("a", 1);
        let mut b = profile("b", 2);
        b.fingerprint = fingerprint::generate(GenerateOptions {
            seed: 2,
            os: Some("macos"),
            ..Default::default()
        })
        .expect("mac");
        assert!(collision_core::find(&[a.clone(), b.clone()]).is_empty());
        assert_eq!(collision_core::summary(&[a, b])["clashing"], 0);
    }
);
source_test!(
    same_device_collides_even_with_different_seed,
    "tests/test_collision.py::test_same_device_collides_even_with_different_seed",
    {
        let a = profile("a", 1);
        let mut b = profile("b", 2);
        b.fingerprint = a.fingerprint.clone();
        b.fingerprint.seed = 99.into();
        let groups = collision_core::find(&[a, b]);
        assert_eq!(groups.len(), 1);
        assert_eq!(
            groups[0]["members"]
                .as_array()
                .expect("members")
                .iter()
                .map(|v| v["name"].as_str().expect("name"))
                .collect::<BTreeSet<_>>(),
            BTreeSet::from(["a", "b"])
        );
        assert_eq!(groups[0]["identical"], false);
    }
);
source_test!(
    identical_when_seed_also_matches,
    "tests/test_collision.py::test_identical_when_seed_also_matches",
    {
        let a = profile("a", 1);
        let mut b = profile("b", 2);
        b.fingerprint = a.fingerprint.clone();
        let groups = collision_core::find(&[a, b]);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0]["identical"], true);
    }
);
source_test!(
    signature_ignores_the_seed,
    "tests/test_collision.py::test_signature_ignores_the_seed",
    {
        let a = profile("a", 1);
        let mut b = profile("b", 2);
        b.fingerprint = a.fingerprint.clone();
        b.fingerprint.seed = 999.into();
        assert_eq!(
            collision_core::signature(&a.fingerprint),
            collision_core::signature(&b.fingerprint)
        );
    }
);
source_test!(
    summary_counts_clashing_profiles_not_groups,
    "tests/test_collision.py::test_summary_counts_clashing_profiles_not_groups",
    {
        let a = profile("a", 1);
        let mut b = profile("b", 2);
        b.fingerprint = a.fingerprint.clone();
        b.fingerprint.seed = 2.into();
        let mut c = profile("c", 3);
        c.fingerprint = fingerprint::generate(GenerateOptions {
            seed: 3,
            os: Some("linux"),
            ..Default::default()
        })
        .expect("linux");
        assert_eq!(
            collision_core::summary(&[a, b, c]),
            json!({"total":3,"groups":1,"clashing":2,"identical":0})
        );
    }
);
source_test!(
    groups_are_most_linked_first,
    "tests/test_collision.py::test_groups_are_most_linked_first",
    {
        let a = profile("a", 1);
        let mut b = profile("b", 2);
        b.fingerprint = a.fingerprint.clone();
        b.fingerprint.seed = 2.into();
        let mut c = a.clone();
        c.id = "w3".into();
        c.name = "w3".into();
        c.fingerprint.seed = 3.into();
        let mut d = profile("m1", 4);
        d.fingerprint = fingerprint::generate(GenerateOptions {
            seed: 1,
            os: Some("macos"),
            ..Default::default()
        })
        .expect("mac");
        let mut e = d.clone();
        e.id = "m2".into();
        e.name = "m2".into();
        e.fingerprint.seed = 2.into();
        assert_eq!(
            collision_core::find(&[a, b, c, d, e])
                .iter()
                .map(|g| g["members"].as_array().expect("members").len())
                .collect::<Vec<_>>(),
            [3, 2]
        );
    }
);
