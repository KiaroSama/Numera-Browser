use super::*;
fn jar() -> Value {
    json!([{"domain":".facebook.com","expirationDate":1893456000.5,"hostOnly":false,"httpOnly":true,"name":"c_user","path":"/","sameSite":"no_restriction","secure":true,"session":false,"storeId":"0","value":"1000123"},{"domain":"www.facebook.com","name":"sb","path":"/","sameSite":"lax","secure":false,"session":true,"value":"abc"}])
}
fn norm(v: &Value) -> Value {
    browser::cookie_normalize::normalize(v)
        .expect("browser contract")
        .expect("browser contract")
}
fn parse(text: &str) -> Vec<Value> {
    browser::cookie_normalize::parse(text).expect("browser contract")
}
source_test!(
    test_normalize_extension_cookie,
    "tests/test_cookies.py::test_normalize_extension_cookie",
    {
        assert_eq!(
            norm(&jar()[0]),
            json!({"name":"c_user","value":"1000123","domain":".facebook.com","path":"/","expires":1893456000.5,"httpOnly":true,"secure":true,"sameSite":"None"})
        );
    }
);
source_test!(
    test_session_cookie_gets_minus_one,
    "tests/test_cookies.py::test_session_cookie_gets_minus_one",
    {
        assert_eq!(norm(&jar()[1])["expires"].as_f64(), Some(-1.0));
    }
);
source_test!(
    test_samesite_none_requires_secure,
    "tests/test_cookies.py::test_samesite_none_requires_secure",
    {
        assert_eq!(
            norm(
                &json!({"name":"a","value":"1","domain":"x.com","sameSite":"no_restriction","secure":false})
            )["sameSite"],
            "Lax"
        );
    }
);
source_test!(
    test_incomplete_cookies_are_dropped,
    "tests/test_cookies.py::test_incomplete_cookies_are_dropped",
    {
        for v in [
            json!({"value":"no name","domain":"x.com"}),
            json!({"name":"no domain"}),
            json!("not a dict"),
        ] {
            assert!(
                browser::cookie_normalize::normalize(&v)
                    .expect("browser contract")
                    .is_none()
            );
        }
    }
);
source_test!(
    test_parse_extension_json_list,
    "tests/test_cookies.py::test_parse_extension_json_list",
    {
        let out = parse(&jar().to_string());
        assert_eq!(
            out.iter().map(|v| v["name"].clone()).collect::<Vec<_>>(),
            vec![json!("c_user"), json!("sb")]
        );
    }
);
source_test!(
    test_parse_playwright_storage_state,
    "tests/test_cookies.py::test_parse_playwright_storage_state",
    {
        assert_eq!(
            parse(&json!({"cookies":jar(),"origins":[]}).to_string()).len(),
            2
        );
    }
);
source_test!(
    test_parse_single_cookie_object,
    "tests/test_cookies.py::test_parse_single_cookie_object",
    {
        assert_eq!(parse(&jar()[0].to_string())[0]["name"], "c_user");
    }
);
source_test!(
    test_parse_empty_input,
    "tests/test_cookies.py::test_parse_empty_input",
    {
        assert!(parse("").is_empty());
        assert!(parse("   \n ").is_empty());
    }
);
source_test!(
    test_parse_netscape,
    "tests/test_cookies.py::test_parse_netscape",
    {
        let out = parse(
            "# Netscape HTTP Cookie File\n\n.facebook.com\tTRUE\t/\tTRUE\t1893456000\tc_user\t1000123\n#HttpOnly_.facebook.com\tTRUE\t/\tTRUE\t1893456000\txs\tsecret\n# a real comment\n",
        );
        assert_eq!(
            out.iter().map(|v| v["name"].clone()).collect::<Vec<_>>(),
            vec![json!("c_user"), json!("xs")]
        );
        assert_eq!(out[0]["httpOnly"], false);
        assert_eq!(out[1]["httpOnly"], true);
        assert_eq!(out[1]["domain"], ".facebook.com");
    }
);
source_test!(
    test_netscape_round_trip,
    "tests/test_cookies.py::test_netscape_round_trip",
    {
        let out = parse(&jar().to_string());
        let text = browser::cookie_format::to_netscape(&out).expect("browser contract");
        let again = parse(&text);
        for (a, b) in out.iter().zip(&again) {
            for k in ["name", "value", "domain"] {
                assert_eq!(a[k], b[k]);
            }
        }
    }
);
source_test!(
    test_dumps_json_is_storage_state,
    "tests/test_cookies.py::test_dumps_json_is_storage_state",
    {
        let out = parse(&jar().to_string());
        let text = browser::cookie_format::dumps(&out, "json").expect("browser contract");
        let value: Value = serde_json::from_str(&text).expect("browser contract");
        assert_eq!(
            value
                .as_object()
                .expect("browser contract")
                .keys()
                .cloned()
                .collect::<std::collections::BTreeSet<_>>(),
            std::collections::BTreeSet::from(["cookies".to_owned(), "origins".to_owned()])
        );
        assert_eq!(value["cookies"][0]["name"], "c_user");
    }
);
source_test!(test_read_file, "tests/test_cookies.py::test_read_file", {
    let dir = fresh();
    let p = dir.path().join("cookies.json");
    std::fs::write(&p, jar().to_string()).expect("browser contract");
    assert_eq!(
        browser::cookie_normalize::read_file(&p)
            .expect("browser contract")
            .len(),
        2
    );
    dir.cleanup().expect("remove owned cookie fixture");
});
source_test!(
    test_write_nothing_is_a_noop,
    "tests/test_cookies.py::test_write_nothing_is_a_noop",
    {
        assert_eq!(
            browser::cookies::write(&[], false, || panic!("empty import activated browser"))
                .expect("browser contract"),
            0
        );
    }
);
source_test!(
    test_a_malformed_expiry_does_not_abort_the_whole_import,
    "tests/test_cookies.py::test_a_malformed_expiry_does_not_abort_the_whole_import",
    {
        let out=parse(&json!([{ "name":"good1","value":"1","domain":".x.test","path":"/","expires":1789000000},{"name":"bad","value":"2","domain":".x.test","path":"/","expires":"whenever"},{"name":"good2","value":"3","domain":".x.test","path":"/","expirationDate":"2027-01-01T00:00:00Z"}]).to_string());
        assert_eq!(
            out.iter().map(|v| v["name"].clone()).collect::<Vec<_>>(),
            vec![json!("good1"), json!("bad"), json!("good2")]
        );
        assert_eq!(out[1]["expires"].as_f64(), Some(-1.0));
        assert_eq!(out[0]["expires"].as_f64(), Some(1789000000.0));
        assert!(out[2]["expires"].as_f64().expect("browser contract") > 0.0);
    }
);
source_test!(
    test_an_iso_expiry_is_understood,
    "tests/test_cookies.py::test_an_iso_expiry_is_understood",
    {
        assert!(norm(&json!({"name":"iso","domain":"x.test","value":"v","expires":"2027-01-01T00:00:00Z"}))["expires"].as_f64().expect("browser contract")>1_700_000_000.0);
    }
);
