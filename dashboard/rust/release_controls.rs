use serde_json::Value;
#[cfg(any(target_arch = "wasm32", test))]
use url::Url;

pub(crate) const PREFERENCES: [(&str, &str); 3] = [
    ("app_auto_update", "Automatically update the app"),
    (
        "browser_core_auto_update",
        "Automatically update browser cores",
    ),
    (
        "clean_old_versions_on_update",
        "Clean old versions after updating",
    ),
];

pub(crate) fn preferences(v: &Value) -> bool {
    PREFERENCES.iter().all(|(key, _)| v[key].is_boolean())
}

pub(crate) fn can_apply(status: &Value, channel: &str) -> bool {
    status["channel"] == channel
        && status["offerId"].as_str().is_some_and(|id| {
            id.len() == 64
                && id
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
        && match channel {
            "app" => {
                status["requiresCombined"] == false
                    && status["app"]["verified"] == true
                    && status["app"]["update_available"] == true
            }
            "all" => status["app"]["verified"] == true && status["app"]["update_available"] == true,
            _ => false,
        }
}

#[cfg(any(target_arch = "wasm32", test))]
pub(crate) fn profile_path(id: &str, suffix: &str) -> Option<String> {
    if id.is_empty() || id == "." || id == ".." || id.chars().any(char::is_control) {
        return None;
    }
    let mut url = Url::parse("http://127.0.0.1/api/profiles/").ok()?;
    url.path_segments_mut().ok()?.pop_if_empty().push(id);
    Some(format!("{}{suffix}", url.path()))
}

#[cfg(any(target_arch = "wasm32", test))]
pub(crate) fn bridge_command(origin: &str) -> Option<String> {
    let url = Url::parse(origin).ok()?;
    if url.scheme() != "http"
        || url.host_str() != Some("127.0.0.1")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port() == Some(0)
    {
        return None;
    }
    Some(format!(
        "numera-engine mcp --bridge http://127.0.0.1:{}",
        url.port_or_known_default()?
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn profile_paths_keep_ids_inside_one_segment() {
        assert_eq!(
            profile_path("one/two?x%", "/ip-check").as_deref(),
            Some("/api/profiles/one%2Ftwo%3Fx%25/ip-check")
        );
        assert_eq!(profile_path("..", "/cookies/export"), None);
    }

    #[test]
    fn bridge_command_uses_loopback_port_without_credentials() {
        assert_eq!(
            bridge_command("http://127.0.0.1:17654/").as_deref(),
            Some("numera-engine mcp --bridge http://127.0.0.1:17654")
        );
        assert!(bridge_command("http://example.org:17654").is_none());
        assert!(bridge_command("http://token@127.0.0.1:17654").is_none());
    }

    #[test]
    fn app_apply_requires_verified_offer_for_the_requested_channel() {
        let mut app = json!({"channel":"app","offerId":"1".repeat(64),"requiresCombined":false,"app":{"verified":false,"update_available":true}});
        assert!(!can_apply(&app, "app"));
        app["app"]["verified"] = true.into();
        assert!(can_apply(&app, "app"));
        app["requiresCombined"] = true.into();
        assert!(!can_apply(&app, "app"));
        assert!(!can_apply(&app, "browsers"));
        assert!(!can_apply(&app, "all"));
        app["channel"] = json!("all");
        assert!(can_apply(&app, "all"));
        app["app"]["verified"] = json!(false);
        assert!(!can_apply(&app, "all"));
        assert!(!can_apply(
            &json!({"channel":"browsers","offerId":"1".repeat(64),"browsers":[{"update_available":true}]}),
            "browsers"
        ));
    }

    #[test]
    fn app_apply_requires_the_reviewed_manifest_identity() {
        let mut app = json!({"channel":"app","requiresCombined":false,"app":{"verified":true,"update_available":true}});
        assert!(!can_apply(&app, "app"));
        for id in ["", "not-a-manifest", &"A".repeat(64), &"1".repeat(63)] {
            app["offerId"] = json!(id);
            assert!(!can_apply(&app, "app"));
        }
        app["offerId"] = json!("1".repeat(64));
        assert!(can_apply(&app, "app"));
    }

    #[test]
    fn preferences_accept_booleans_only_and_require_all_fields() {
        let mut state = json!({"app_auto_update": false,"browser_core_auto_update":false,"clean_old_versions_on_update":true});
        assert!(preferences(&state));
        state["app_auto_update"] = json!("false");
        assert!(!preferences(&state));
        assert!(!preferences(&json!({})));
    }
}
