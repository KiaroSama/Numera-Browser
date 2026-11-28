#![cfg(not(target_arch = "wasm32"))]

use numera_dashboard::contracts;
use serde_json::json;

#[test]
fn proxy_pool_accepts_the_cleared_check_returned_after_connection_edits() {
    let rows = json!([{
        "id": "edited-proxy",
        "type": "HTTP",
        "host": "proxy.example",
        "port": 8080,
        "status": "untested",
        "lastCheck": null,
        "checkedAt": null,
        "profiles": 1
    }]);
    assert_eq!(contracts::validate(rows.clone(), "[proxy]"), Ok(rows));
}

#[test]
fn proxy_pool_still_rejects_a_malformed_nonnull_check() {
    for check in [json!(true), json!("unchecked"), json!(42), json!([])] {
        let rows = json!([{
            "id": "malformed-proxy",
            "type": "HTTP",
            "host": "proxy.example",
            "port": 8080,
            "lastCheck": check
        }]);
        assert!(contracts::validate(rows, "[proxy]").is_err());
    }
}
