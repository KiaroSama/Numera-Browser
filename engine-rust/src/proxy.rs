use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

/// A proxy's secret value and its write intent are separate facts.
#[derive(Clone, Serialize, Deserialize)]
pub struct Proxy {
    pub server: String,
    pub username: Option<String>,
    password: Option<String>,
    pub country: Option<String>,
    #[serde(skip, default = "chosen")]
    password_chosen: bool,
}

fn chosen() -> bool {
    true
}

impl Proxy {
    pub fn from_record(record: &Value, stored: bool) -> Result<Self, serde_json::Error> {
        let mut proxy: Self = serde_json::from_value(record.clone())?;
        proxy.password_chosen = !stored;
        Ok(proxy)
    }

    pub fn set_password(&mut self, password: Option<String>) {
        self.password = password;
        self.password_chosen = true;
    }

    pub fn password_intent(&self) -> &'static str {
        if !self.password_chosen || self.password.is_none() {
            "preserve"
        } else if self.password.as_deref() == Some("") {
            "clear"
        } else {
            "replace"
        }
    }

    pub fn to_record(&self) -> Value {
        let mut record = Map::new();
        record.insert("server".into(), self.server.clone().into());
        for (name, value) in [
            ("username", &self.username),
            ("password", &self.password),
            ("country", &self.country),
        ] {
            record.insert(
                name.into(),
                value
                    .as_ref()
                    .map_or(Value::Null, |v| Value::String(v.clone())),
            );
        }
        Value::Object(record)
    }

    pub fn to_playwright(&self) -> Value {
        let mut record = Map::new();
        record.insert("server".into(), self.server.clone().into());
        for (name, value) in [("username", &self.username), ("password", &self.password)] {
            if let Some(value) = value.as_ref().filter(|value| !value.is_empty()) {
                record.insert(name.into(), Value::String(value.clone()));
            }
        }
        Value::Object(record)
    }
}
