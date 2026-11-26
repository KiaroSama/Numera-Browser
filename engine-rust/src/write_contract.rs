//! Shape-aware write admission shared by native storage and public callers.
use crate::{models::Profile, validation};
use serde_json::Value;
use std::{error, fmt};

pub const ENGINE_OBJECT: &str = "engine object";
pub const ENGINE_RECORD: &str = "engine record";
pub const DASHBOARD_PATCH: &str = "dashboard patch";

#[derive(Clone, Copy)]
pub enum Input<'a> {
    Json(&'a Value),
    Profile(&'a Profile),
}

#[derive(Debug)]
pub enum Error {
    UnknownShape(String),
    Refused(Vec<String>),
    Policy(serde_json::Error),
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownShape(kind) => write!(
                formatter,
                "unknown profile shape {}",
                validation::repr(&Value::String(kind.clone()))
            ),
            Self::Refused(problems) => formatter.write_str(&problems.join("; ")),
            Self::Policy(_) => formatter.write_str("Field policy is unreadable"),
        }
    }
}
impl error::Error for Error {
    fn source(&self) -> Option<&(dyn error::Error + 'static)> {
        match self {
            Self::Policy(error) => Some(error),
            _ => None,
        }
    }
}
impl From<serde_json::Error> for Error {
    fn from(error: serde_json::Error) -> Self {
        Self::Policy(error)
    }
}

pub fn shape(input: Input<'_>, kind: Option<&str>) -> Result<&'static str, Error> {
    match kind {
        Some(ENGINE_OBJECT) => Ok(ENGINE_OBJECT),
        Some(ENGINE_RECORD) => Ok(ENGINE_RECORD),
        Some(DASHBOARD_PATCH) => Ok(DASHBOARD_PATCH),
        Some(kind) => Err(Error::UnknownShape(kind.into())),
        None => Ok(match input {
            Input::Profile(_) => ENGINE_OBJECT,
            Input::Json(value)
                if value
                    .as_object()
                    .is_some_and(|record| record.contains_key("fingerprint")) =>
            {
                ENGINE_RECORD
            }
            Input::Json(_) => DASHBOARD_PATCH,
        }),
    }
}

fn kind(input: Input<'_>) -> &'static str {
    match input {
        Input::Profile(_) => "Profile",
        Input::Json(value) => validation::kind(value),
    }
}

fn contextual(problems: Vec<String>, context: &str) -> Vec<String> {
    problems
        .into_iter()
        .map(|problem| match problem.strip_prefix("profile:") {
            Some(message) => format!("{context}:{message}"),
            None => problem,
        })
        .collect()
}

pub fn profile_problems(
    input: Input<'_>,
    context: &str,
    declared: Option<&str>,
) -> Result<Vec<String>, Error> {
    let selected = shape(input, declared)?;
    match (selected, input) {
        (ENGINE_OBJECT, Input::Profile(profile)) => {
            let record = serde_json::to_value(profile)?;
            Ok(contextual(
                validation::profile_problems(&record, ENGINE_RECORD)?,
                context,
            ))
        }
        (ENGINE_OBJECT, _) => Ok(vec![format!(
            "{context}: an engine profile object must be a Profile, not {}. Nothing was written.",
            kind(input)
        )]),
        (ENGINE_RECORD, Input::Json(record)) => Ok(contextual(
            validation::profile_problems(record, ENGINE_RECORD)?,
            context,
        )),
        (ENGINE_RECORD, _) => Ok(vec![format!(
            "{context}: an engine profile record must be a mapping, not {}. Nothing was written.",
            kind(input)
        )]),
        (_, Input::Json(record)) => Ok(contextual(
            validation::profile_problems(record, DASHBOARD_PATCH)?,
            context,
        )),
        _ => Ok(vec![format!(
            "{context}: a profile must be a record or a patch, not {}. Nothing was written.",
            kind(input)
        )]),
    }
}

fn require_problems(problems: Vec<String>) -> Result<(), Error> {
    if problems.is_empty() {
        Ok(())
    } else {
        Err(Error::Refused(problems))
    }
}

pub fn require(input: Input<'_>, context: &str, declared: Option<&str>) -> Result<(), Error> {
    require_problems(profile_problems(input, context, declared)?)
}

pub fn require_flat_numbers(input: Input<'_>, context: &str) -> Result<(), Error> {
    let problems = match input {
        Input::Json(record) if record.is_object() => {
            contextual(validation::flat_number_problems(record)?, context)
        }
        _ => vec![format!(
            "{context}: a dashboard profile must be a mapping, not {}. Nothing was written.",
            kind(input)
        )],
    };
    require_problems(problems)
}
