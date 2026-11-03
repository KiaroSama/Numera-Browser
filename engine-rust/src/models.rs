use crate::proxy::Proxy;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::time::{SystemTime, UNIX_EPOCH};

fn one() -> i64 {
    1
}
fn zero_seed() -> serde_json::Number {
    0.into()
}
fn one_float() -> serde_json::Number {
    serde_json::Number::from_f64(1.0).expect("Finite default pixel ratio")
}
fn altered() -> String {
    "altered".into()
}
fn noise() -> String {
    "noise".into()
}
fn masked() -> String {
    "masked".into()
}
fn prompt() -> String {
    "prompt".into()
}
fn playwright() -> String {
    "playwright".into()
}
fn now() -> f64 {
    match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(time) => time.as_secs_f64(),
        Err(error) => -error.duration().as_secs_f64(),
    }
}

/// Persisted identity. Unknown storage fields are ignored, not rejected as writes.
#[derive(Clone, Serialize, Deserialize)]
pub struct Fingerprint {
    pub os: String,
    pub user_agent: String,
    pub platform: String,
    pub ch_platform: String,
    pub chrome_version: String,
    pub screen_width: i64,
    pub screen_height: i64,
    pub viewport_width: i64,
    pub viewport_height: i64,
    pub hardware_concurrency: i64,
    pub device_memory: Value,
    pub webgl_vendor: String,
    pub webgl_renderer: String,
    pub locale: String,
    pub timezone: String,
    pub languages: Vec<String>,
    #[serde(default)]
    pub is_mobile: bool,
    #[serde(default = "zero_seed")]
    pub seed: serde_json::Number,
    #[serde(default)]
    pub fonts: Vec<String>,
    #[serde(default = "one")]
    pub cameras: i64,
    #[serde(default = "one")]
    pub microphones: i64,
    #[serde(default = "altered")]
    pub webrtc: String,
    #[serde(default = "noise")]
    pub canvas: String,
    #[serde(default = "noise")]
    pub audio: String,
    #[serde(default = "masked")]
    pub font_mode: String,
    #[serde(default = "prompt")]
    pub geolocation: String,
    #[serde(default)]
    pub do_not_track: bool,
    #[serde(default = "noise")]
    pub client_rects: String,
    #[serde(default = "noise")]
    pub webgl_image: String,
    #[serde(default = "masked")]
    pub media_mode: String,
    #[serde(default = "one")]
    pub speakers: i64,
    #[serde(default)]
    pub os_version: String,
    #[serde(default)]
    pub device_model: String,
    #[serde(default)]
    pub max_touch_points: i64,
    #[serde(default = "one_float")]
    pub device_pixel_ratio: serde_json::Number,
}

impl Fingerprint {
    pub fn from_record(record: &Value) -> Result<Self, serde_json::Error> {
        serde_json::from_value(record.clone())
    }
}

#[derive(Clone, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: Value,
    #[serde(default)]
    pub notes: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default = "playwright")]
    pub engine: String,
    #[serde(default)]
    pub startup_urls: Vec<String>,
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub launch_args: Vec<String>,
    #[serde(default)]
    pub bookmarks: Vec<Value>,
    #[serde(default)]
    pub dns: String,
    #[serde(default)]
    pub ip_based: Vec<String>,
    #[serde(default)]
    pub clear_on_start: Vec<String>,
    #[serde(default)]
    pub switched_off: Vec<String>,
    #[serde(default = "now")]
    pub created_at: f64,
    #[serde(default = "now")]
    pub updated_at: f64,
    pub fingerprint: Fingerprint,
    #[serde(default)]
    pub proxy: Option<Proxy>,
}

impl Profile {
    pub fn from_record(record: &Value) -> Result<Self, serde_json::Error> {
        let mut stored = record.clone();
        // Source storage treats every false JSON value as no proxy. Normalize
        // only this optional read view; truthy malformed proxies still fail.
        if stored
            .get("proxy")
            .is_some_and(|v| !crate::conversion::truth(v))
        {
            stored["proxy"] = Value::Null;
        }
        let mut profile: Self = serde_json::from_value(stored.clone())?;
        profile.proxy = match stored.get("proxy").filter(|v| !v.is_null()) {
            Some(value) => Some(Proxy::from_record(value, true)?),
            None => None,
        };
        Ok(profile)
    }
}
