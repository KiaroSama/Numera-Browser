//! Shared JSON and Netscape cookie exports for the CLI and HTTP.
use crate::{
    browser::{adapter::Adapter, cookie_format, cookie_normalize, cookies},
    cli::{cookie_commands::Cookies, profile_commands::Outcome},
    conversion::value_text,
    models::Profile,
    storage::repo::RepoError,
};
use serde_json::{Value, json};
use std::{io, path::Path};

pub(crate) fn run(
    service: &Cookies<'_>,
    reference: &str,
    file: Option<&Path>,
    format: Option<&str>,
    engine: Option<&str>,
    time: f64,
    start: impl FnOnce(&Profile) -> io::Result<()>,
) -> Result<Outcome, RepoError> {
    let format = CookieFormat::parse(format, file)?;
    let Some(profile) = service.repo.resolve(reference, time)? else {
        return Ok(missing(reference));
    };
    start(&profile)?;
    let export = with_profile_browser(service, &profile, engine, time, |adapter| {
        CookieExport::read(adapter, format)
    })?;
    let Some(file) = file else {
        return Ok(Outcome::success(export.response(&profile.id, format)));
    };
    let content = export.content;
    // Retain Windows text-mode output for files without changing the API payload.
    #[cfg(windows)]
    let content = content.replace('\n', "\r\n");
    crate::storage::atomic::write_bytes(file, content.as_bytes())?;
    Ok(Outcome::success(
        json!({"ok":true,"count":export.count,"file":file,"format":format.label()}),
    ))
}

pub(crate) fn with_profile_browser<T>(
    service: &Cookies<'_>,
    profile: &Profile,
    engine: Option<&str>,
    time: f64,
    operation: impl FnOnce(&mut Adapter) -> io::Result<T>,
) -> Result<T, RepoError> {
    service
        .sessions
        .with_finite_browser(
            &profile.id,
            &value_text(&profile.name),
            |session, mut current| {
                current.startup_urls.clear();
                if let Some(engine) = engine.filter(|v| !v.is_empty()) {
                    current.engine = engine.into();
                }
                service.runtime.open(session, current, true, time)
            },
            operation,
        )
        .map_err(RepoError::from)
}

pub(crate) fn missing(reference: &str) -> Outcome {
    Outcome::failed(format!("no profile matching '{reference}'"))
}

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
    pub(crate) fn read(adapter: &mut Adapter, format: CookieFormat) -> io::Result<Self> {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_export_keeps_profile_and_format() -> io::Result<()> {
        let rows = [
            json!({"domain":".example.test","name":"qa","value":"é","path":"/","httpOnly":true,"secure":true,"expires":-1}),
        ];
        let json_response = CookieExport::from_rows(&rows, CookieFormat::Json)?
            .response("fixture-a", CookieFormat::Json);
        assert_eq!(json_response["profile_id"], "fixture-a");
        assert_eq!(json_response["filename"], "fixture-a-cookies.json");
        let content = json_response["content"]
            .as_str()
            .ok_or_else(|| io::Error::other("Expected cookie JSON"))?;
        assert_eq!(
            serde_json::from_str::<Value>(content).map_err(io::Error::other)?["cookies"],
            json!(rows)
        );
        let response = CookieExport::from_rows(&rows, CookieFormat::Netscape)?
            .response("fixture-a", CookieFormat::Netscape);
        let content = response["content"]
            .as_str()
            .ok_or_else(|| io::Error::other("Expected Netscape text"))?;
        let parsed = cookie_format::parse_netscape(content)?;
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0]["value"], "é");
        assert_eq!(parsed[0]["httpOnly"], true);
        assert_eq!(response["count"], 1);
        Ok(())
    }
}
