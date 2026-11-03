use super::{adapter::Adapter, probe_grades};
use serde_json::{Map, Value, json};
use std::{io, time::Duration};

const TLS_ECHO: [&str; 3] = [
    "https://tls.peet.ws/api/all",
    "https://check.ja3.zone/",
    "https://tools.scrapfly.io/api/fp/ja3",
];
pub(super) fn observation(
    adapter: &mut Adapter,
    expression: &str,
    argument: Value,
) -> io::Result<Option<Value>> {
    let result = adapter.call(
        "evaluate",
        Map::from_iter([
            ("expression".into(), expression.into()),
            ("argument".into(), argument),
            ("tolerate_failure".into(), true.into()),
        ]),
        Duration::from_secs(60),
    )?;
    if !result["evaluated"].is_boolean() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Browser evaluation result missing",
        ));
    }
    if result["evaluated"] != true {
        return Ok(None);
    }
    result.get("value").cloned().map(Some).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Browser evaluation value missing",
        )
    })
}
pub(super) fn evaluate(
    adapter: &mut Adapter,
    expression: &str,
    argument: Value,
) -> io::Result<Value> {
    Ok(observation(adapter, expression, argument)?.unwrap_or(Value::Null))
}
pub fn tls(adapter: &mut Adapter) -> io::Result<Value> {
    for endpoint in TLS_ECHO {
        let navigation = adapter.call(
            "navigate",
            Map::from_iter([
                ("url".into(), endpoint.into()),
                ("new_page".into(), false.into()),
                ("timeout_ms".into(), 20000.into()),
                ("tolerate_failure".into(), true.into()),
            ]),
            Duration::from_secs(25),
        )?;
        if !navigation["navigated"].is_boolean() {
            return Err(io::Error::other("TLS navigation result missing"));
        }
        if navigation["navigated"] != true {
            continue;
        }
        let info = evaluate(adapter, include_str!("tls_probe.js"), Value::Null)?;
        if crate::conversion::truth(&info["ja3Hash"]) || crate::conversion::truth(&info["ja4"]) {
            return probe_grades::tls(&info);
        }
    }
    probe_grades::tls(&Value::Null)
}
pub fn dns(adapter: &mut Adapter, country: Option<&str>) -> io::Result<Value> {
    let result = evaluate(
        adapter,
        include_str!("dns_probe.js"),
        json!({
            "idUrl":"https://bash.ws/id","subTmpl":"https://{i}.{id}.bash.ws/",
            "resultTmpl":"https://bash.ws/dnsleak/test/{id}?json","n":8,
        }),
    )?;
    probe_grades::dns(&result, country)
}
