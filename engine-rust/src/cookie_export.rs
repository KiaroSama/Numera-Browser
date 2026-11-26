//! Shared JSON and Netscape cookie exports for the CLI and HTTP.
use crate::browser::{cookie_format, cookie_normalize, cookies};
use serde_json::{Value, json};
use std::{io, path::Path};

#[derive(Clone, Copy)]
pub(crate) enum CookieFormat {
    Json,
    Netscape,
}

impl CookieFormat {
    pub(crate) fn parse(format: Option<&str>, path: Option<&Path>) -> io::Result<Self> {
        match format {
            Some("json") => Ok(Self::Json),
            Some("netscape") => Ok(Self::Netscape),
            None if path
                .and_then(Path::extension)
                .is_some_and(|ext| ext.eq_ignore_ascii_case("txt")) =>
            {
                Ok(Self::Netscape)
            }
            None => Ok(Self::Json),
            Some(_) => Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Cookie export format must be json or netscape",
            )),
        }
    }
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Json => "json",
            Self::Netscape => "netscape",
        }
    }
}

pub(crate) struct CookieExport {
    pub(crate) count: usize,
    pub(crate) content: String,
}
impl CookieExport {
    pub(crate) fn read(
        adapter: &mut crate::browser::adapter::Adapter,
        format: CookieFormat,
    ) -> io::Result<Self> {
        let rows = cookie_normalize::normalize_many(&cookies::read(adapter)?)?;
        Self::from_rows(&rows, format)
    }
    pub(crate) fn from_rows(rows: &[Value], format: CookieFormat) -> io::Result<Self> {
        Ok(Self {
            count: rows.len(),
            content: cookie_format::dumps(rows, format.label())?,
        })
    }
    pub(crate) fn response(self, profile_id: &str, format: CookieFormat) -> Value {
        let ext = match format {
            CookieFormat::Json => "json",
            CookieFormat::Netscape => "txt",
        };
        json!({"profile_id":profile_id,"format":format.label(),"count":self.count,
            "filename":format!("{profile_id}-cookies.{ext}"),"content":self.content})
    }
}
