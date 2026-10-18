use crate::{
    devices,
    fingerprint::{self, GenerateOptions},
    models::Profile,
    proxy::Proxy,
    python_random::PythonRandom,
};
use serde_json::{Value, json};
use std::io;

pub fn select(
    profiles: &[Profile],
    references: &[String],
    tag: Option<&str>,
    everything: bool,
) -> Vec<Profile> {
    if everything {
        return profiles.to_vec();
    }
    let mut chosen = profiles
        .iter()
        .filter(|p| tag.is_some_and(|tag| !tag.is_empty() && p.tags.iter().any(|v| v == tag)))
        .cloned()
        .collect::<Vec<_>>();
    for profile in profiles {
        if references
            .iter()
            .any(|v| v == &profile.id || profile.name == v.as_str())
            && !chosen.iter().any(|p| p.id == profile.id)
        {
            chosen.push(profile.clone());
        }
    }
    chosen
}
pub fn apply(
    profile: &mut Profile,
    patch: &Value,
    regeneration_seed: Option<i64>,
) -> io::Result<Vec<String>> {
    let mut changed = Vec::new();
    for key in ["engine", "notes"] {
        if let Some(value) = patch.get(key).and_then(Value::as_str) {
            let current = if key == "engine" {
                &mut profile.engine
            } else {
                &mut profile.notes
            };
            if value != current.as_str() {
                *current = value.into();
                changed.push(key.into());
            }
        }
    }
    for key in ["startup_urls", "extensions"] {
        if let Some(value) = patch.get(key).filter(|v| !v.is_null()) {
            let rows = value
                .as_array()
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "Bulk paths must be an array")
                })?
                .iter()
                .map(|v| {
                    v.as_str().map(str::to_owned).ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidInput, "Bulk path must be text")
                    })
                })
                .collect::<io::Result<Vec<_>>>()?;
            let current = if key == "extensions" {
                &mut profile.extensions
            } else {
                &mut profile.startup_urls
            };
            if rows != *current {
                *current = rows;
                changed.push(key.into());
            }
        }
    }
    if let Some(value) = patch.get("proxy") {
        let current = profile.proxy.as_ref().map_or(Value::Null, Proxy::to_record);
        if value != &current {
            profile.proxy = if value.is_null() {
                None
            } else {
                Some(Proxy::from_record(value, false).map_err(io::Error::other)?)
            };
            changed.push("proxy".into());
        }
    }
    let tags = |key: &str| -> io::Result<Vec<String>> {
        match patch.get(key).filter(|v| crate::conversion::truth(v)) {
            None => Ok(Vec::new()),
            Some(value) => value
                .as_array()
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::InvalidInput, "Bulk tags must be an array")
                })?
                .iter()
                .map(|v| {
                    v.as_str().map(str::to_owned).ok_or_else(|| {
                        io::Error::new(io::ErrorKind::InvalidInput, "Bulk tag must be text")
                    })
                })
                .collect(),
        }
    };
    let added = tags("add_tags")?
        .into_iter()
        .filter(|v| !profile.tags.contains(v))
        .collect::<Vec<_>>();
    let removed = tags("remove_tags")?
        .into_iter()
        .filter(|v| profile.tags.contains(v))
        .collect::<Vec<_>>();
    if !added.is_empty() || !removed.is_empty() {
        profile.tags.extend(added);
        profile.tags.retain(|v| !removed.contains(v));
        changed.push("tags".into());
    }
    if let Some(locale) = patch
        .get("locale")
        .and_then(Value::as_str)
        .filter(|v| !v.is_empty() && *v != profile.fingerprint.locale)
    {
        let catalog = devices::catalog().map_err(io::Error::other)?;
        if let Some(settings) = catalog["catalog"]["LOCALES"].get(locale) {
            profile.fingerprint.locale = locale.into();
            profile.fingerprint.timezone = settings[0]
                .as_str()
                .ok_or_else(|| io::Error::other("Locale timezone missing"))?
                .into();
            profile.fingerprint.languages = settings[1]
                .as_array()
                .ok_or_else(|| io::Error::other("Locale languages missing"))?
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| io::Error::other("Locale language missing"))
                })
                .collect::<io::Result<_>>()?;
            changed.push("locale".into());
        }
    }
    if let Some(timezone) = patch
        .get("timezone")
        .and_then(Value::as_str)
        .filter(|v| *v != profile.fingerprint.timezone)
    {
        profile.fingerprint.timezone = timezone.into();
        changed.push("timezone".into());
    }
    if patch.get("regen").is_some_and(crate::conversion::truth) {
        profile.fingerprint = fingerprint::generate(GenerateOptions {
            seed: regeneration_seed
                .ok_or_else(|| io::Error::other("Regeneration entropy missing"))?,
            os: Some(&profile.fingerprint.os),
            locale: Some(&profile.fingerprint.locale),
            country: profile.proxy.as_ref().and_then(|p| p.country.as_deref()),
            mobile: false,
        })
        .map_err(io::Error::other)?;
        changed.push("fingerprint".into());
    }
    Ok(changed)
}

pub struct GenerateBatch<'a, S = i64> {
    pub count: usize,
    pub seed: S,
    pub oses: &'a [String],
    pub locales: &'a [String],
    pub chrome_versions: &'a [String],
    pub engine: &'a str,
    pub prefix: &'a str,
    pub start: i64,
    pub pad: usize,
    pub tags: &'a [String],
    pub mobile: bool,
}
pub fn generate<S: Into<serde_json::Number>>(
    options: GenerateBatch<'_, S>,
    mut identity: impl FnMut() -> io::Result<String>,
    time: f64,
) -> io::Result<Vec<Profile>> {
    let defaults = ["windows".to_owned()];
    let oses = if options.oses.is_empty() {
        &defaults[..]
    } else {
        options.oses
    };
    let mut rng = PythonRandom::from_number(&options.seed.into()).ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "Batch seed must be an integer")
    })?;
    let mut profiles = Vec::new();
    for i in 0..options.count {
        let os = oses[i % oses.len()].to_lowercase();
        let locale = if options.locales.is_empty() {
            None
        } else {
            Some(options.locales[(i / oses.len()) % options.locales.len()].as_str())
        };
        let mut fp = fingerprint::generate(GenerateOptions {
            seed: (rng.below(2_147_483_647) + 1) as i64,
            os: Some(&os),
            locale,
            country: None,
            mobile: options.mobile || os == "android",
        })
        .map_err(io::Error::other)?;
        if !options.chrome_versions.is_empty() {
            fp.chrome_version =
                options.chrome_versions[rng.below(options.chrome_versions.len())].clone();
            fp.user_agent = fingerprint::user_agent(
                &fp.os,
                &fp.chrome_version,
                &fp.os_version,
                &fp.device_model,
            )
            .map_err(io::Error::other)?;
        }
        let number = options
            .start
            .checked_add(i64::try_from(i).map_err(io::Error::other)?)
            .ok_or_else(|| io::Error::other("Bulk name index overflow"))?;
        let name = format!("{}-{:0width$}", options.prefix, number, width = options.pad);
        profiles.push(Profile::from_record(&json!({"id":identity()?,"name":name,"fingerprint":fp,"engine":options.engine,"tags":options.tags,"created_at":time,"updated_at":time})).map_err(io::Error::other)?);
    }
    Ok(profiles)
}
