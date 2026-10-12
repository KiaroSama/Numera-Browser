use super::{adapter::Adapter, network_grades, probes};
use serde_json::{Value, json};
use std::io;

/// Observe all network signals in the one admitted browser context.
pub fn read(adapter: &mut Adapter, country: Option<&str>) -> io::Result<Value> {
    let Some(info) = probes::observation(
        adapter,
        include_str!("ip_probe.js"),
        json!([
            "https://ipapi.co/json/",
            "https://ipwho.is/",
            "https://api.ipify.org/?format=json"
        ]),
    )?
    else {
        return Ok(json!({"error":"Browser IP evaluation failed"}));
    };
    let ice = probes::evaluate(adapter, include_str!("webrtc_probe.js"), Value::Null)?;
    let leaks = network_grades::leaks(&ice, info["ip"].as_str())?;
    let dns = probes::dns(adapter, country)?;
    let tls = probes::tls(adapter)?;
    Ok(json!({"info":info,"leaks":leaks,"dns":dns,"tls":tls}))
}
