use super::*;
type WireResult = (Vec<u8>, bool);
type WireWorker = thread::JoinHandle<Result<WireResult>>;
use std::{
    net::{Shutdown, TcpListener},
    thread,
};
fn wire(response: Vec<u8>, abort: bool) -> Result<(u16, WireWorker)> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let port = listener.local_addr()?.port();
    let worker = thread::spawn(move || {
        let (mut socket, _) = listener.accept()?;
        socket.set_read_timeout(Some(Duration::from_secs(3)))?;
        socket.set_write_timeout(Some(Duration::from_secs(3)))?;
        if abort {
            socket.shutdown(Shutdown::Both)?;
            return Ok((Vec::new(), true));
        }
        let mut request = Vec::new();
        let mut byte = [0; 1];
        while !request.ends_with(b"\r\n\r\n") {
            if socket.read(&mut byte)? == 0 {
                return Err("HTTP client closed before request headers".into());
            }
            request.push(byte[0]);
            if request.len() > 8192 {
                return Err("fixture request headers exceeded bound".into());
            }
        }
        let headers = std::str::from_utf8(&request)?;
        let length = headers
            .lines()
            .find_map(|line| {
                line.to_ascii_lowercase()
                    .strip_prefix("content-length:")
                    .and_then(|n| n.trim().parse::<usize>().ok())
            })
            .unwrap_or(0);
        if length > 8192 {
            return Err("fixture request body exceeded bound".into());
        }
        let old = request.len();
        request.resize(old + length, 0);
        socket.read_exact(&mut request[old..])?;
        socket.write_all(&response)?;
        let closed = socket.read(&mut byte)? == 0;
        Ok((request, closed))
    });
    Ok((port, worker))
}
fn join(worker: WireWorker) -> Result<WireResult> {
    worker.join().map_err(|_| "HTTP wire fixture panicked")?
}
fn response(body: &[u8]) -> Vec<u8> {
    let mut v = format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
        body.len()
    )
    .into_bytes();
    v.extend(body);
    v
}

source_test!(
    test_request_retains_auth_headers_and_sends_json_as_utf8_bytes_none_42445b,
    "tests/test_native_http_replay.py::test_request_retains_auth_headers_and_sends_json_as_utf8_bytes[None]",
    {
        let payload = None::<Value>;
        let (port, worker) = wire(response(br#"{"ok":true}"#), false)?;
        let result = request(
            port,
            "PUT",
            "/api/profiles/fixture",
            Some("fixture-token"),
            payload.as_ref(),
        );
        let observed = join(worker)?;
        let result = result?;
        assert_eq!(result.status, 200);
        assert_eq!(
            result.headers.get("content-type"),
            Some(&json!("application/json"))
        );
        assert_eq!(result.body, json!({"ok":true}));
        assert!(observed.1, "HTTP connection remained open");
        let raw = String::from_utf8(observed.0)?;
        let (headers, bytes) = raw
            .split_once("\r\n\r\n")
            .ok_or("request delimiter missing")?;
        assert!(headers.starts_with("PUT /api/profiles/fixture HTTP/1.1\r\n"));
        assert!(
            headers
                .lines()
                .any(|line| line == "Authorization: Bearer fixture-token")
        );
        if let Some(payload) = payload {
            assert!(headers.contains("Content-Type: application/json"));
            assert_eq!(serde_json::from_str::<Value>(bytes)?, payload);
            assert!(bytes.contains("α 日本語"));
        } else {
            assert_eq!(bytes, "");
            assert!(!headers.contains("Content-Type:"));
        }

        Ok(())
    }
);
source_test!(
    test_request_retains_auth_headers_and_sends_json_as_utf8_bytes_payload1_92adae,
    "tests/test_native_http_replay.py::test_request_retains_auth_headers_and_sends_json_as_utf8_bytes[payload1]",
    {
        let payload = Some(json!({"notes":"Fixture α 日本語","_rev":3}));
        let (port, worker) = wire(response(br#"{"ok":true}"#), false)?;
        let result = request(
            port,
            "PUT",
            "/api/profiles/fixture",
            Some("fixture-token"),
            payload.as_ref(),
        );
        let observed = join(worker)?;
        let result = result?;
        assert_eq!(result.status, 200);
        assert_eq!(
            result.headers.get("content-type"),
            Some(&json!("application/json"))
        );
        assert_eq!(result.body, json!({"ok":true}));
        assert!(observed.1, "HTTP connection remained open");
        let raw = String::from_utf8(observed.0)?;
        let (headers, bytes) = raw
            .split_once("\r\n\r\n")
            .ok_or("request delimiter missing")?;
        assert!(headers.starts_with("PUT /api/profiles/fixture HTTP/1.1\r\n"));
        assert!(
            headers
                .lines()
                .any(|line| line == "Authorization: Bearer fixture-token")
        );
        if let Some(payload) = payload {
            assert!(headers.contains("Content-Type: application/json"));
            assert_eq!(serde_json::from_str::<Value>(bytes)?, payload);
            assert!(bytes.contains("α 日本語"));
        } else {
            assert_eq!(bytes, "");
            assert!(!headers.contains("Content-Type:"));
        }

        Ok(())
    }
);
source_test!(
    test_request_failures_close_the_connection_without_accepting_output_send_8595c6,
    "tests/test_native_http_replay.py::test_request_failures_close_the_connection_without_accepting_output[send]",
    {
        let raw = response(b"{");
        let (port, worker) = wire(raw, true)?;
        let result = request(
            port,
            "PUT",
            "/api/profiles/fixture",
            None,
            Some(&json!({"notes":"fixture"})),
        );
        let observed = join(worker)?;
        assert!(result.is_err(), "accepted send output");
        assert!(observed.1, "failed HTTP response leaked connection");

        Ok(())
    }
);
source_test!(
    test_request_failures_close_the_connection_without_accepting_output_oversize_1f4f25,
    "tests/test_native_http_replay.py::test_request_failures_close_the_connection_without_accepting_output[oversize]",
    {
        let raw =
            b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 65537\r\n\r\n"
                .to_vec();
        let (port, worker) = wire(raw, false)?;
        let result = request(
            port,
            "PUT",
            "/api/profiles/fixture",
            None,
            Some(&json!({"notes":"fixture"})),
        );
        let observed = join(worker)?;
        assert!(result.is_err(), "accepted oversize output");
        assert!(observed.1, "failed HTTP response leaked connection");

        Ok(())
    }
);
source_test!(
    test_request_failures_close_the_connection_without_accepting_output_malformed_191d31,
    "tests/test_native_http_replay.py::test_request_failures_close_the_connection_without_accepting_output[malformed]",
    {
        let raw = response(b"{");
        let (port, worker) = wire(raw, false)?;
        let result = request(
            port,
            "PUT",
            "/api/profiles/fixture",
            None,
            Some(&json!({"notes":"fixture"})),
        );
        let observed = join(worker)?;
        assert!(result.is_err(), "accepted malformed output");
        assert!(observed.1, "failed HTTP response leaked connection");

        Ok(())
    }
);
source_test!(
    test_http_profile_comparison_retains_every_stable_field_and_bounds_both_timestamps_none_59cf3a,
    "tests/test_native_http_replay.py::test_http_profile_comparison_retains_every_stable_field_and_bounds_both_timestamps[none]",
    {
        let damage = "none";
        let expected = json!({"id":"fixture","notes":"kept","created_at":1000,"updated_at":1001,"fingerprint":{"seed":42},"proxy":{"password":"synthetic"},"future":[42,true]});
        let before = expected.clone();
        let mut observed = expected.clone();
        observed["created_at"] = json!(1002);
        observed["updated_at"] = json!(1003);
        let result = require_http_profile(&observed, &expected, 1001.0, 1004.0);
        assert_eq!(result.is_ok(), damage == "none", "none");
        assert_eq!(expected, before);

        Ok(())
    }
);
source_test!(test_http_profile_comparison_retains_every_stable_field_and_bounds_both_timestamps_identity_9a1d75,"tests/test_native_http_replay.py::test_http_profile_comparison_retains_every_stable_field_and_bounds_both_timestamps[identity]", {let damage="identity";

let expected=json!({"id":"fixture","notes":"kept","created_at":1000,"updated_at":1001,"fingerprint":{"seed":42},"proxy":{"password":"synthetic"},"future":[42,true]});let before=expected.clone();let mut observed=expected.clone();observed["created_at"]=json!(1002);observed["updated_at"]=json!(1003);observed["id"]=json!("other");
let result=require_http_profile(&observed,&expected,1001.0,1004.0);assert_eq!(result.is_ok(),damage=="none","identity");assert_eq!(expected,before);

Ok(())
});
source_test!(test_http_profile_comparison_retains_every_stable_field_and_bounds_both_timestamps_secret_af9526,"tests/test_native_http_replay.py::test_http_profile_comparison_retains_every_stable_field_and_bounds_both_timestamps[secret]", {let damage="secret";

let expected=json!({"id":"fixture","notes":"kept","created_at":1000,"updated_at":1001,"fingerprint":{"seed":42},"proxy":{"password":"synthetic"},"future":[42,true]});let before=expected.clone();let mut observed=expected.clone();observed["created_at"]=json!(1002);observed["updated_at"]=json!(1003);observed["proxy"]["password"]=Value::Null;
let result=require_http_profile(&observed,&expected,1001.0,1004.0);assert_eq!(result.is_ok(),damage=="none","secret");assert_eq!(expected,before);

Ok(())
});
source_test!(test_http_profile_comparison_retains_every_stable_field_and_bounds_both_timestamps_fingerprint_18556d,"tests/test_native_http_replay.py::test_http_profile_comparison_retains_every_stable_field_and_bounds_both_timestamps[fingerprint]", {let damage="fingerprint";

let expected=json!({"id":"fixture","notes":"kept","created_at":1000,"updated_at":1001,"fingerprint":{"seed":42},"proxy":{"password":"synthetic"},"future":[42,true]});let before=expected.clone();let mut observed=expected.clone();observed["created_at"]=json!(1002);observed["updated_at"]=json!(1003);observed["fingerprint"]["seed"]=json!(43);
let result=require_http_profile(&observed,&expected,1001.0,1004.0);assert_eq!(result.is_ok(),damage=="none","fingerprint");assert_eq!(expected,before);

Ok(())
});
source_test!(test_http_profile_comparison_retains_every_stable_field_and_bounds_both_timestamps_unknown_c9c583,"tests/test_native_http_replay.py::test_http_profile_comparison_retains_every_stable_field_and_bounds_both_timestamps[unknown]", {let damage="unknown";

let expected=json!({"id":"fixture","notes":"kept","created_at":1000,"updated_at":1001,"fingerprint":{"seed":42},"proxy":{"password":"synthetic"},"future":[42,true]});let before=expected.clone();let mut observed=expected.clone();observed["created_at"]=json!(1002);observed["updated_at"]=json!(1003);observed.as_object_mut().ok_or("profile object missing")?.remove("future");
let result=require_http_profile(&observed,&expected,1001.0,1004.0);assert_eq!(result.is_ok(),damage=="none","unknown");assert_eq!(expected,before);

Ok(())
});
source_test!(test_http_profile_comparison_retains_every_stable_field_and_bounds_both_timestamps_created_before_3b44fc,"tests/test_native_http_replay.py::test_http_profile_comparison_retains_every_stable_field_and_bounds_both_timestamps[created-before]", {let damage="created-before";

let expected=json!({"id":"fixture","notes":"kept","created_at":1000,"updated_at":1001,"fingerprint":{"seed":42},"proxy":{"password":"synthetic"},"future":[42,true]});let before=expected.clone();let mut observed=expected.clone();observed["created_at"]=json!(1002);observed["updated_at"]=json!(1003);observed["created_at"]=json!(1000);
let result=require_http_profile(&observed,&expected,1001.0,1004.0);assert_eq!(result.is_ok(),damage=="none","created-before");assert_eq!(expected,before);

Ok(())
});
source_test!(test_http_profile_comparison_retains_every_stable_field_and_bounds_both_timestamps_created_after_9147b1,"tests/test_native_http_replay.py::test_http_profile_comparison_retains_every_stable_field_and_bounds_both_timestamps[created-after]", {let damage="created-after";

let expected=json!({"id":"fixture","notes":"kept","created_at":1000,"updated_at":1001,"fingerprint":{"seed":42},"proxy":{"password":"synthetic"},"future":[42,true]});let before=expected.clone();let mut observed=expected.clone();observed["created_at"]=json!(1002);observed["updated_at"]=json!(1003);observed["created_at"]=json!(1005);
let result=require_http_profile(&observed,&expected,1001.0,1004.0);assert_eq!(result.is_ok(),damage=="none","created-after");assert_eq!(expected,before);

Ok(())
});
source_test!(test_http_profile_comparison_retains_every_stable_field_and_bounds_both_timestamps_updated_bool_f58141,"tests/test_native_http_replay.py::test_http_profile_comparison_retains_every_stable_field_and_bounds_both_timestamps[updated-bool]", {let damage="updated-bool";

let expected=json!({"id":"fixture","notes":"kept","created_at":1000,"updated_at":1001,"fingerprint":{"seed":42},"proxy":{"password":"synthetic"},"future":[42,true]});let before=expected.clone();let mut observed=expected.clone();observed["created_at"]=json!(1002);observed["updated_at"]=json!(1003);observed["updated_at"]=json!(true);
let result=require_http_profile(&observed,&expected,1001.0,1004.0);assert_eq!(result.is_ok(),damage=="none","updated-bool");assert_eq!(expected,before);

Ok(())
});
source_test!(test_http_profile_comparison_retains_every_stable_field_and_bounds_both_timestamps_updated_nonfinite_99c831,"tests/test_native_http_replay.py::test_http_profile_comparison_retains_every_stable_field_and_bounds_both_timestamps[updated-nonfinite]", {let damage="updated-nonfinite";

let expected=json!({"id":"fixture","notes":"kept","created_at":1000,"updated_at":1001,"fingerprint":{"seed":42},"proxy":{"password":"synthetic"},"future":[42,true]});let before=expected.clone();let mut observed=expected.clone();observed["created_at"]=json!(1002);observed["updated_at"]=json!(1003);observed["updated_at"]=Value::Number(serde_json::Number::from_string_unchecked("NaN".into()));
let result=require_http_profile(&observed,&expected,1001.0,1004.0);assert_eq!(result.is_ok(),damage=="none","updated-nonfinite");assert_eq!(expected,before);

Ok(())
});
source_test!(
    test_main_removes_only_successful_fixtures_and_retains_failed_owned_state_false_e1a80b,
    "tests/test_native_http_replay.py::test_main_removes_only_successful_fixtures_and_retains_failed_owned_state[False]",
    {
        let t = Temp::new()?;
        let owned = t.path().join("owned-http");
        let fixture = Fixture::create(&owned)?;
        let marker = owned.join("retained-owner.json");
        fs::write(&marker, b"owned fixture evidence")?;
        let (port, worker) = wire(response(br#"{"ok":true}"#), false)?;
        let result = request(port, "GET", "/api/ready", None, None).map(|r| {
            assert_eq!(r.body, json!({"ok":true}));
        });
        assert!(join(worker)?.1);
        let result = fixture.finish(result);
        if false {
            assert!(result.is_err());
            assert_eq!(fs::read(marker)?, b"owned fixture evidence");
        } else {
            result?;
            assert!(!owned.exists());
        }

        Ok(())
    }
);
source_test!(
    test_main_removes_only_successful_fixtures_and_retains_failed_owned_state_true_7a9c6a,
    "tests/test_native_http_replay.py::test_main_removes_only_successful_fixtures_and_retains_failed_owned_state[True]",
    {
        let t = Temp::new()?;
        let owned = t.path().join("owned-http");
        let fixture = Fixture::create(&owned)?;
        let marker = owned.join("retained-owner.json");
        fs::write(&marker, b"owned fixture evidence")?;
        let (port, worker) = wire(response(b"{"), false)?;
        let result = request(port, "GET", "/api/ready", None, None).map(|r| {
            assert_eq!(r.body, json!({"ok":true}));
        });
        assert!(join(worker)?.1);
        let result = fixture.finish(result);
        if true {
            assert!(result.is_err());
            assert_eq!(fs::read(marker)?, b"owned fixture evidence");
        } else {
            result?;
            assert!(!owned.exists());
        }

        Ok(())
    }
);
