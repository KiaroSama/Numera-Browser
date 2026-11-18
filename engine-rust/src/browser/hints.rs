use crate::{identity, models::Fingerprint};
use serde_json::{Value, json};

/// Application identity and header policy are supplied to the SDK adapter;
/// the adapter only creates CDP sessions and executes these commands.
pub fn policy(fp: &Fingerprint) -> Result<Value, serde_json::Error> {
    let mut commands = vec![
        json!({"method":"Emulation.setUserAgentOverride", "payload":{
            "userAgent":fp.user_agent,
            "acceptLanguage":fp.languages.join(","),
            "platform":fp.platform,
            "userAgentMetadata":identity::metadata(fp)?
        }}),
    ];
    let prefixes = if !identity::sends_client_hints(fp) {
        commands.push(json!({"method":"Fetch.enable", "payload":{
            "patterns":[{"urlPattern":"*", "requestStage":"Request"}]
        }}));
        vec!["sec-ch-ua"]
    } else {
        Vec::new()
    };
    Ok(json!({"commands":commands, "strip_headers_prefixes":prefixes}))
}
