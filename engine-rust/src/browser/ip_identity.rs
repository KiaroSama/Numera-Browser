use crate::{devices::catalog, models::Profile};
use std::io;

pub fn needs_lookup(profile: &Profile) -> bool {
    !profile.ip_based.is_empty() && profile.proxy.is_some()
}

/// Apply an independently observed exit country only to the marked signals.
/// Network lookup and its bounded fallback belong to the native launch service.
pub fn resolve(profile: &Profile, country: Option<&str>) -> io::Result<Profile> {
    let mut resolved = profile.clone();
    if !needs_lookup(profile) {
        return Ok(resolved);
    }
    let Some(country) = country.filter(|country| !country.is_empty()) else {
        return Ok(resolved);
    };
    let data = &catalog().map_err(io::Error::other)?["catalog"];
    let marked = |signal: &str| profile.ip_based.iter().any(|value| value == signal);
    if let Some(locale) = data["COUNTRY_TO_LOCALE"][country].as_str()
        && let Some(signals) = data["LOCALES"][locale].as_array()
    {
        if marked("timezone") {
            resolved.fingerprint.timezone = signals[0]
                .as_str()
                .ok_or_else(|| io::Error::other("Locale timezone is missing"))?
                .into();
        }
        if marked("languages") {
            resolved.fingerprint.locale = locale.into();
            resolved.fingerprint.languages =
                serde_json::from_value(signals[1].clone()).map_err(io::Error::other)?;
        }
    }
    if marked("geolocation")
        && data["COUNTRY_GEO"].get(country).is_some()
        && let Some(proxy) = resolved.proxy.as_mut()
    {
        proxy.country = Some(country.into());
    }
    Ok(resolved)
}
