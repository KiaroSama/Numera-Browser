#![cfg(not(target_arch = "wasm32"))]
//! The 71 original non-DOM cases. The 124 original DOM cases run in Chromium.
//! Each parameterized Vitest row is a separate Rust test, including all 33
//! forbidden status control characters. Browser execution is in browser::tests.
use numera_dashboard::{contracts, model::*, updates};
use serde_json::{Value, json};
use std::collections::BTreeSet;
fn p(id: &str) -> Value {
    let mut p = new_profile("Facebook Ads", "playwright");
    p["id"] = json!(id);
    p["name"] = json!(id);
    p["_new"] = json!(false);
    p["_rev"] = json!(5);
    p
}
fn with_proxy() -> Value {
    let mut p = p("p");
    p["proxy"] = json!({"type":"HTTP","host":"old.host","port":"80","user":"u","pass":"original","country":"US"});
    p
}
fn offered() -> Value {
    json!({"state":"available","message":"A verified update is available.","current":{"version":"0.36.0","sourceSha":"1".repeat(40),"generation":10,"platform":"windows-x64","portable":true},"offer":{"id":"offer-11","version":"0.36.1","sourceSha":"2".repeat(40),"generation":11,"downloadBytes":1048576,"expandedBytes":4194304,"components":{"sdk":{"playwright":"1.62.0"}},"restartRequired":true},"progress":{"receivedBytes":0,"totalBytes":1048576},"canCancel":true})
}

mod api_response_contract {
    use super::*;
    fn health_row() -> Value {
        json!({"id":"fixture","name":"Fixture","engine":"playwright","status":"stopped","coherent":true,"issues":[],"proxy":{"set":false,"country":null,"type":null},"trust":null,"schedule":null})
    }
    #[test]
    fn health_nullable_proxy() {
        let v = json!({"profiles":[health_row()],"summary":{"total":1}});
        assert_eq!(contracts::validate(v.clone(), "health").unwrap(), v);
    }
    #[test]
    fn health_summary_only_schedule() {
        let mut r = health_row();
        r["schedule"] = json!({"enabled":true,"everyHours":24,"preset":"general"});
        let v = json!({"profiles":[r],"summary":{"total":1}});
        assert_eq!(contracts::validate(v.clone(), "health").unwrap(), v);
    }
    #[test]
    fn rotation_refusal() {
        let v = json!({"ok":null,"rotated":false,"error":"no change-IP URL is set for this proxy"});
        assert_eq!(contracts::validate(v.clone(), "rotation").unwrap(), v);
    }
    #[test]
    fn rotation_unknown_previous_ip() {
        let v = json!({"ok":true,"rotated":true,"previousIp":null,"ip":"192.0.2.1","country":null,"city":null});
        assert_eq!(contracts::validate(v.clone(), "rotation").unwrap(), v);
    }
    #[test]
    fn proxy_unknown_location() {
        let v = json!({"ok":true,"ip":"192.0.2.1","country":null,"city":null,"latencyMs":1});
        assert_eq!(contracts::validate(v.clone(), "probe").unwrap(), v);
    }
    #[test]
    fn tls_null_ja3() {
        let v = json!({"ok":true,"checks":[],"ja3Hash":null,"ja4":"fixture-ja4"});
        assert_eq!(contracts::validate(v.clone(), "probe").unwrap(), v);
    }
    #[test]
    fn tls_null_ja4() {
        let v = json!({"ok":true,"checks":[],"ja3Hash":"fixture-ja3","ja4":null});
        assert_eq!(contracts::validate(v.clone(), "probe").unwrap(), v);
    }
    #[test]
    fn tls_both_null() {
        let v = json!({"ok":true,"checks":[],"ja3Hash":null,"ja4":null});
        assert_eq!(contracts::validate(v.clone(), "probe").unwrap(), v);
    }
    #[test]
    fn tls_refuses_numeric_fingerprint() {
        assert!(
            contracts::validate(
                json!({"ok":true,"checks":[],"ja3Hash":12,"ja4":null}),
                "probe"
            )
            .unwrap_err()
            .contains("invalid string")
        );
    }
    #[test]
    fn pool_unknown_country() {
        let v = json!([{"id":"fixture","type":"HTTP","host":"127.0.0.1","port":"8080","lastCheck":{"ok":true,"ip":"192.0.2.1","country":null,"city":null,"latencyMs":1}}]);
        assert_eq!(contracts::validate(v.clone(), "[proxy]").unwrap(), v);
    }
    #[test]
    fn health_refuses_string_boolean() {
        let mut r = health_row();
        r["coherent"] = json!("true");
        assert!(
            contracts::validate(json!({"profiles":[r],"summary":{"total":1}}), "health")
                .unwrap_err()
                .contains("invalid boolean")
        );
    }
}
mod country_picker_offers_each_place_once {
    use super::*;
    #[test]
    fn uk_alias_lookup_preserved() {
        assert_eq!(country_locale("UK"), "en-GB");
        assert_eq!(country_locale("GB"), country_locale("UK"));
    }
    #[test]
    fn countries_offer_unique_locales() {
        assert!(!COUNTRIES.contains(&"UK"));
        assert!(COUNTRIES.contains(&"GB"));
        assert_eq!(
            COUNTRIES
                .map(country_locale)
                .into_iter()
                .collect::<BTreeSet<_>>()
                .len(),
            COUNTRIES.len()
        );
    }
}
mod dev_proxy {
    use super::*;
    #[test]
    fn dev_api_forwards_to_engine() {
        use std::{
            io::{Read, Write},
            net::{TcpListener, TcpStream},
            thread,
        };
        let backend = TcpListener::bind("127.0.0.1:0").unwrap();
        let backend_address = backend.local_addr().unwrap().to_string();
        let received = thread::spawn(move || {
            let (mut connection, _) = backend.accept().unwrap();
            let mut bytes = vec![];
            let mut buffer = [0; 1024];
            loop {
                let n = connection.read(&mut buffer).unwrap();
                bytes.extend_from_slice(&buffer[..n]);
                if bytes.ends_with(b"{\"name\":\"Ads\"}") {
                    break;
                }
            }
            connection.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\nConnection: close\r\n\r\n{\"ok\":true}").unwrap();
            bytes
        });
        let frontend = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = frontend.local_addr().unwrap();
        let proxy = thread::spawn(move || {
            let (client, _) = frontend.accept().unwrap();
            numera_dashboard::development::serve_connection(
                client,
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")),
                &backend_address,
            )
            .unwrap();
        });
        let mut client = TcpStream::connect(address).unwrap();
        client.write_all(b"POST /api/folders HTTP/1.1\r\nHost: localhost:5173\r\nAuthorization: Bearer fixture\r\nContent-Length: 14\r\n\r\n{\"name\":\"Ads\"}").unwrap();
        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();
        proxy.join().unwrap();
        let request = String::from_utf8(received.join().unwrap()).unwrap();
        assert!(request.starts_with("POST /api/folders HTTP/1.1\r\n"));
        assert!(request.contains("Authorization: Bearer fixture"));
        assert!(request.ends_with("{\"name\":\"Ads\"}"));
        assert!(response.ends_with("{\"ok\":true}"));
    }
    #[test]
    fn production_api_uses_own_origin() {
        assert_eq!(
            contracts::response_kind("GET", "/api/profiles"),
            Some("[profile]")
        );
        assert_eq!(contracts::api_url("/api/health"), "/api/health");
        assert_eq!(
            contracts::api_url("/api/profiles?tag=Work"),
            "/api/profiles?tag=Work"
        );
    }
}
mod engine_built_identity_is_not_claimed {
    use super::*;
    #[test]
    fn camoufox_only_builds_identity() {
        assert!(owns_identity("camoufox"));
        for e in ["cloak", "patchright", "playwright", ""] {
            assert!(!owns_identity(e));
        }
    }
    #[test]
    fn only_honored_hardware_is_graded() {
        let mut p = p("p");
        p["webglRenderer"] = json!("ANGLE (Apple M1, OpenGL 4.1)");
        p["screen"] = json!("1x1");
        p["cores"] = json!(999);
        assert_eq!(coherence(&p).len(), 3);
        p["engine"] = json!("camoufox");
        assert!(coherence(&p).is_empty());
    }
    #[test]
    fn camoufox_locale_zone_proxy_still_graded() {
        let mut p = with_proxy();
        p["engine"] = json!("camoufox");
        p["timezone"] = json!("Europe/Berlin");
        p["proxy"]["country"] = json!("DE");
        let issues = coherence(&p);
        assert!(issues.iter().any(|i| i.contains("Timezone")));
        assert!(issues.iter().any(|i| i.contains("proxy country")));
    }
}
mod profile_edits {
    use super::*;
    #[test]
    fn nested_proxy_edit_keeps_remote_password() {
        let base = with_proxy();
        let mut draft = base.clone();
        draft["proxy"]["host"] = json!("new.host");
        let mut latest = base.clone();
        latest["_rev"] = json!(6);
        latest["proxy"]["pass"] = json!("remote");
        let (profile, conflicts) = rebase(&base, &draft, &latest);
        assert!(conflicts.is_empty());
        assert_eq!(profile["proxy"]["host"], "new.host");
        assert_eq!(profile["proxy"]["pass"], "remote");
        let sent = for_write(&profile_write(&latest, &profile));
        assert!(sent["proxy"].get("pass").is_none());
        assert_eq!(sent["_rev"], 6);
    }
    #[test]
    fn retyped_old_password_is_conflict() {
        let base = with_proxy();
        let mut draft = base.clone();
        draft["_passwordSet"] = json!(true);
        let mut latest = base.clone();
        latest["proxy"]["pass"] = json!("remote");
        let (profile, conflicts) = rebase(&base, &draft, &latest);
        assert_eq!(conflicts, vec![vec!["proxy", "pass"]]);
        assert_eq!(
            for_write(&profile_write(&latest, &profile))["proxy"]["pass"],
            "original"
        );
    }
    #[test]
    fn removed_proxy_conflicts_as_whole() {
        let base = with_proxy();
        let mut draft = base.clone();
        draft["proxy"]["host"] = json!("new.host");
        let mut latest = base.clone();
        latest["proxy"] = Value::Null;
        let (profile, conflicts) = rebase(&base, &draft, &latest);
        assert_eq!(conflicts, vec![vec!["proxy"]]);
        assert_eq!(profile["proxy"], draft["proxy"]);
    }
    #[test]
    fn equal_concurrent_edits_need_no_conflict() {
        let base = with_proxy();
        let mut draft = base.clone();
        draft["notes"] = json!("same");
        let mut latest = draft.clone();
        latest["_rev"] = json!(6);
        let (profile, conflicts) = rebase(&base, &draft, &latest);
        assert!(conflicts.is_empty());
        let mut next = latest.clone();
        next["_rev"] = json!(7);
        next["tags"] = json!(["new"]);
        let mut draft = profile;
        draft["notes"] = json!("next edit");
        let (profile, _) = rebase(&latest, &draft, &next);
        assert_eq!(
            for_write(&profile_write(&next, &profile)),
            json!({"id":"p","_rev":7,"notes":"next edit"})
        );
        assert_eq!(base["proxy"]["host"], "old.host");
    }
}
mod proxy_password_intent {
    use super::*;
    #[test]
    fn untouched_password_omitted() {
        let p = with_proxy();
        let sent = for_write(&p);
        assert!(sent["proxy"].get("pass").is_none());
        assert_eq!(sent["proxy"]["host"], "old.host");
        assert_eq!(p["proxy"]["pass"], "original");
    }
    #[test]
    fn chosen_password_transmitted() {
        let mut p = with_proxy();
        p["_passwordSet"] = json!(true);
        p["proxy"]["pass"] = json!("chosen");
        assert_eq!(for_write(&p)["proxy"]["pass"], "chosen");
    }
    #[test]
    fn retyping_same_password_is_decision() {
        let mut p = with_proxy();
        p["_passwordSet"] = json!(true);
        assert_eq!(for_write(&p)["proxy"]["pass"], "original");
    }
    #[test]
    fn explicit_clear_is_empty_string() {
        let mut p = with_proxy();
        p["_passwordSet"] = json!(true);
        p["proxy"]["pass"] = json!("");
        assert_eq!(for_write(&p)["proxy"]["pass"], "");
    }
    #[test]
    fn marker_never_reaches_engine() {
        let mut p = with_proxy();
        p["_passwordSet"] = json!(true);
        assert!(for_write(&p).get("_passwordSet").is_none());
        assert!(without_marker(p).get("_passwordSet").is_none());
    }
    #[test]
    fn no_proxy_passes_unchanged() {
        let p = json!({"id":"p1","name":"One","proxy":null});
        assert_eq!(for_write(&p), p);
    }
    #[test]
    fn proxy_off_is_removal() {
        for marker in [false, true] {
            let mut p = with_proxy();
            p["proxy"] = Value::Null;
            p["_passwordSet"] = json!(marker);
            let sent = for_write(&p);
            assert!(sent.get("proxy").is_some_and(Value::is_null));
            assert!(sent.get("_passwordSet").is_none());
        }
    }
    #[test]
    fn off_on_transmits_new_empty_password() {
        let mut ed = Editor::new(with_proxy());
        ed.draft["proxy"] = json!({"type":"HTTP","host":"","port":"","pass":""});
        ed.draft["_passwordSet"] = json!(true);
        assert_eq!(
            for_write(&profile_write(&ed.base, &ed.draft))["proxy"]["pass"],
            ""
        );
    }
    #[test]
    fn pool_pick_transmits_pool_password() {
        let mut p = with_proxy();
        p["proxy"] =
            json!({"type":"SOCKS5","host":"pool.example","port":"1080","pass":"pool-secret"});
        p["_passwordSet"] = json!(true);
        assert_eq!(for_write(&p)["proxy"]["pass"], "pool-secret");
    }
    #[test]
    fn refetched_profile_has_no_dirty_marker() {
        let mut p = with_proxy();
        p["_passwordSet"] = json!(true);
        let saved = for_write(&p);
        assert!(saved.get("_passwordSet").is_none());
        let later = for_write(&saved);
        assert!(later["proxy"].get("pass").is_none());
    }
}
mod update_consent {
    use super::*;
    fn bad_bridge(endpoint: &str) {
        assert!(updates::bridge(&json!({"endpoint":endpoint,"token":"a".repeat(64)})).is_none());
    }
    #[test]
    fn https_bridge_refused() {
        bad_bridge("https://example.com")
    }
    #[test]
    fn localhost_bridge_refused() {
        bad_bridge("http://localhost:19876")
    }
    #[test]
    fn non_root_bridge_refused() {
        bad_bridge("http://127.0.0.1:19876/other")
    }
    #[test]
    fn zero_port_bridge_refused() {
        bad_bridge("http://127.0.0.1:0")
    }
    #[test]
    fn malformed_status_cannot_enable_install() {
        let mut v = offered();
        v["offer"]["downloadBytes"] = json!(-1);
        assert_eq!(updates::status(v).unwrap_err(), "Invalid update status");
    }
    macro_rules! controls {($($name:ident:$code:expr),* $(,)?)=>{$(#[test]fn $name(){let mut v=offered();v["message"]=json!(format!("Update{}ready",char::from_u32($code).unwrap()));assert_eq!(updates::status(v).unwrap_err(),"Invalid update status");})*};}
    controls! {control_00:0,control_01:1,control_02:2,control_03:3,control_04:4,control_05:5,control_06:6,control_07:7,control_08:8,control_09:9,control_10:10,control_11:11,control_12:12,control_13:13,control_14:14,control_15:15,control_16:16,control_17:17,control_18:18,control_19:19,control_20:20,control_21:21,control_22:22,control_23:23,control_24:24,control_25:25,control_26:26,control_27:27,control_28:28,control_29:29,control_30:30,control_31:31,control_127:127}
    #[test]
    fn printable_unicode_preserved() {
        let mut v = offered();
        v["message"] = json!("Update ready ~ — فارسی 🚀");
        assert_eq!(
            updates::status(v).unwrap()["message"],
            "Update ready ~ — فارسی 🚀"
        );
    }
}
