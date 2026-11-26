use super::*;
source_test!(
    c000_the_first_connection_uses_the_validated_address,
    "tests/test_rotate_transport.py::test_the_first_connection_uses_the_validated_address",
    {
        let global = rotation::testing::public_destination(
            "http://host-a.example/rotate",
            &["8.8.8.8".parse().expect("public IP")],
        )
        .expect("public destination");
        assert_eq!(global.1.to_string(), "8.8.8.8");
        let site = Site::body(Some(b"first-pin".to_vec()));
        let url = format!(
            "http://host-a.example:{}/rotate",
            site.server.address.port()
        );
        let (url, pin) = rotation::testing::local_destination(&url, &[site.server.address.ip()])
            .expect("validated fixture pin");
        assert_eq!(pin, site.server.address.ip());
        let body = runtime(rotation::testing::control_at(
            url,
            pin,
            Duration::from_secs(2),
            |_| async { Err(io::Error::other("unexpected redirect")) },
        ))
        .expect("request to validated local pin");
        assert_eq!(body, "first-pin");
        assert_eq!(site.seen.lock().expect("actual first server").len(), 1);
    }
);
source_test!(
    c000_a_redirect_connects_to_the_hop_it_validated,
    "tests/test_rotate_transport.py::test_a_redirect_connects_to_the_hop_it_validated",
    {
        redirect_pins(false);
    }
);
source_test!(
    c000_a_redirect_to_a_private_address_is_refused,
    "tests/test_rotate_transport.py::test_a_redirect_to_a_private_address_is_refused",
    {
        redirect_pins(true);
    }
);
source_test!(
    c000_a_cgnat_address_is_still_refused,
    "tests/test_rotate_transport.py::test_a_cgnat_address_is_still_refused",
    {
        assert!(
            rotation::testing::public_destination(
                "http://host-a.example/rotate",
                &["100.64.1.1".parse().expect("CGNAT")]
            )
            .is_err()
        );
    }
);
source_test!(
    c000_the_environment_proxy_is_not_used,
    "tests/test_rotate_transport.py::test_the_environment_proxy_is_not_used",
    {
        environment_proxy_isolation();
    }
);
