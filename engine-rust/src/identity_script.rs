use crate::{identity, models::Fingerprint};
use serde::Serialize;
use serde_json::{Map, Value, ser::Formatter};
use std::io::{self, Write};

pub fn config(fp: &Fingerprint) -> Result<Value, serde_json::Error> {
    let metadata = identity::metadata(fp)?;
    let full = if fp.chrome_version.matches('.').count() == 3 {
        fp.chrome_version.clone()
    } else {
        format!(
            "{}.0.0.0",
            fp.chrome_version.split('.').next().unwrap_or("")
        )
    };
    let fields = [
        ("platform", serde_json::to_value(&fp.platform)?),
        ("uaPlatform", fp.ch_platform.trim_matches('"').into()),
        ("uaBrands", metadata["brands"].clone()),
        (
            "platformVersion",
            identity::platform_version(&fp.os, &fp.os_version)?.into(),
        ),
        ("fullVersion", full.into()),
        ("isMobile", fp.is_mobile.into()),
        ("architecture", metadata["architecture"].clone()),
        ("hasUaData", (fp.os != "ios").into()),
        ("deviceModel", serde_json::to_value(&fp.device_model)?),
        ("hardwareConcurrency", fp.hardware_concurrency.into()),
        ("deviceMemory", fp.device_memory.clone()),
        ("maxTouchPoints", fp.max_touch_points.into()),
        (
            "devicePixelRatio",
            Value::Number(fp.device_pixel_ratio.clone()),
        ),
        ("languages", serde_json::to_value(&fp.languages)?),
        ("webglVendor", serde_json::to_value(&fp.webgl_vendor)?),
        ("webglRenderer", serde_json::to_value(&fp.webgl_renderer)?),
        ("webglImage", serde_json::to_value(&fp.webgl_image)?),
        ("seed", fp.seed.into()),
        ("fonts", serde_json::to_value(&fp.fonts)?),
        ("cameras", fp.cameras.into()),
        ("microphones", fp.microphones.into()),
        ("speakers", fp.speakers.into()),
        ("mediaMode", serde_json::to_value(&fp.media_mode)?),
        ("webrtc", serde_json::to_value(&fp.webrtc)?),
        ("canvas", serde_json::to_value(&fp.canvas)?),
        ("audio", serde_json::to_value(&fp.audio)?),
        ("clientRects", serde_json::to_value(&fp.client_rects)?),
        ("fontMode", serde_json::to_value(&fp.font_mode)?),
        ("doNotTrack", fp.do_not_track.into()),
    ];
    Ok(Value::Object(Map::from_iter(
        fields.into_iter().map(|(key, value)| (key.into(), value)),
    )))
}

pub(crate) fn write_ascii_fragment<W: ?Sized + Write>(
    writer: &mut W,
    fragment: &str,
) -> io::Result<()> {
    for character in fragment.chars() {
        if (' '..='~').contains(&character) {
            write!(writer, "{character}")?;
        } else {
            let mut units = [0; 2];
            for unit in character.encode_utf16(&mut units) {
                write!(writer, "\\u{unit:04x}")?;
            }
        }
    }
    Ok(())
}

pub(crate) struct SourceJson;
impl Formatter for SourceJson {
    fn begin_array_value<W: ?Sized + Write>(
        &mut self,
        writer: &mut W,
        first: bool,
    ) -> io::Result<()> {
        if first {
            Ok(())
        } else {
            writer.write_all(b", ")
        }
    }
    fn begin_object_key<W: ?Sized + Write>(
        &mut self,
        writer: &mut W,
        first: bool,
    ) -> io::Result<()> {
        self.begin_array_value(writer, first)
    }
    fn begin_object_value<W: ?Sized + Write>(&mut self, writer: &mut W) -> io::Result<()> {
        writer.write_all(b": ")
    }
    fn write_string_fragment<W: ?Sized + Write>(
        &mut self,
        writer: &mut W,
        fragment: &str,
    ) -> io::Result<()> {
        write_ascii_fragment(writer, fragment)
    }
}

/// Retain the original page script; Rust owns only its identity configuration.
pub fn build(fp: &Fingerprint) -> Result<String, serde_json::Error> {
    let value = config(fp)?;
    let mut bytes = Vec::new();
    value.serialize(&mut serde_json::Serializer::with_formatter(
        &mut bytes, SourceJson,
    ))?;
    let encoded = String::from_utf8(bytes).map_err(|error| {
        serde_json::Error::io(io::Error::new(io::ErrorKind::InvalidData, error))
    })?;
    Ok(include_str!("identity_page.js").replacen("{{CONFIG}}", &encoded, 1))
}
