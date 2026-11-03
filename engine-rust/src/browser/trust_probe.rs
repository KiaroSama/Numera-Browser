use super::{adapter::Adapter, probes, trust_grades};
use crate::models::Fingerprint;
use serde_json::{Map, Value, json};
use std::{io, time::Duration};

pub fn observe(adapter: &mut Adapter, fp: &Fingerprint) -> io::Result<Value> {
    observe_fonts(adapter, fp, &[])
}
pub fn observe_fonts(
    adapter: &mut Adapter,
    fp: &Fingerprint,
    candidates: &[String],
) -> io::Result<Value> {
    let route = adapter.call(
        "route",
        Map::from_iter([
            ("pattern".into(), "**/*".into()),
            ("status".into(), 200.into()),
            ("content_type".into(), "text/html".into()),
            (
                "body".into(),
                "<!doctype html><meta charset=utf-8><title>numera probe</title>".into(),
            ),
        ]),
        Duration::from_secs(10),
    )?;
    if route["routed"] != true {
        return Err(io::Error::other("Offline probe route not confirmed"));
    }
    let navigation = adapter.call(
        "navigate",
        Map::from_iter([
            ("url".into(), "https://numera.probe/".into()),
            ("timeout_ms".into(), 15000.into()),
        ]),
        Duration::from_secs(20),
    )?;
    if navigation["navigated"] != true {
        return Err(io::Error::other("Offline probe page not confirmed"));
    }
    let mut seen = probes::observation(adapter,include_str!("trust_probe.js"),json!({
        "knownFont":fp.fonts.first().map_or("Arial",String::as_str),"alienFont":trust_grades::ALIEN_FONT,
    }))?.ok_or_else(|| io::Error::other("Browser trust evaluation failed"))?;
    if !candidates.is_empty() {
        let fonts = probes::observation(adapter, include_str!("font_probe.js"), json!(candidates))?
            .ok_or_else(|| io::Error::other("Browser font evaluation failed"))?;
        seen.as_object_mut()
            .ok_or_else(|| io::Error::other("Trust observation is not an object"))?
            .insert("fontsRendered".into(), fonts);
    }
    Ok(seen)
}
