use super::{profile_errors, proxy_routes::Proxies, request_body::ObjectBody};
use crate::{
    conversion::{truth, value_text},
    devices,
    storage::repo::RepoError,
};
use axum::{
    Json,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::{Map, Value, json};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

fn ids(value: Option<&Value>, pool: bool) -> Result<Vec<String>, RepoError> {
    let Some(value) = value.filter(|v| truth(v)) else {
        return Ok(Vec::new());
    };
    let values = match value {
        Value::Array(values) => values.clone(),
        Value::String(text) => text.chars().map(|c| Value::String(c.to_string())).collect(),
        Value::Object(object) => object.keys().cloned().map(Value::String).collect(),
        _ => {
            return Err(RepoError::InvalidRecord(
                "Proxy selection is not iterable".into(),
            ));
        }
    };
    values
        .into_iter()
        .filter(|value| !pool || value.is_string())
        .map(|value| {
            value
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| RepoError::InvalidRecord("Proxy selection ID is not text".into()))
        })
        .collect()
}

pub(super) async fn archive(
    State(state): State<Arc<Proxies>>,
    ObjectBody(body): ObjectBody,
) -> Response {
    let Some(selected) = body
        .get("ids")
        .and_then(Value::as_array)
        .filter(|v| !v.is_empty())
    else {
        return profile_errors::detail(
            StatusCode::BAD_REQUEST,
            "give a list of proxy ids to archive",
        );
    };
    let selected = selected.clone();
    let archived = body.get("archived").is_none_or(truth);
    match super::admission::blocking(move || -> Result<Value, RepoError> {
        let mut changed = Vec::new();
        for id in selected {
            let Some(id) = id.as_str() else {
                continue;
            };
            if state.pool.get(id)?.is_some()
                && let Some(row) = state
                    .pool
                    .update(id, Map::from_iter([("archived".into(), archived.into())]))?
            {
                changed.push(row["id"].clone());
            }
        }
        let mut result = Map::new();
        result.insert(
            if archived { "archived" } else { "restored" }.into(),
            changed.len().into(),
        );
        result.insert("ids".into(), json!(changed));
        Ok(Value::Object(result))
    })
    .await
    {
        Ok(Ok(result)) => Json(result).into_response(),
        Ok(Err(error)) => profile_errors::storage(error),
        Err(_) => super::proxy_routes::failure(),
    }
}

fn assign_sync(state: &Proxies, body: Map<String, Value>) -> Result<Response, RepoError> {
    let explicit = body.get("proxyIds").is_some_and(truth);
    let mut proxies = if explicit {
        ids(body.get("proxyIds"), true)?
            .iter()
            .map(|id| state.pool.get(id))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
    } else {
        state.pool.list()?
    };
    if !explicit {
        proxies.retain(|proxy| !truth(&proxy["archived"]));
    }
    let mut locked = 0;
    for proxy in &proxies {
        let id = proxy["id"]
            .as_str()
            .ok_or_else(|| RepoError::InvalidRecord("Proxy ID is not text".into()))?;
        locked += usize::from(state.pool.secret_withheld(id)?);
    }
    if locked > 0 {
        return Ok(profile_errors::detail(
            StatusCode::CONFLICT,
            format!(
                "{locked} of the selected proxies have a password that cannot be read: {}",
                crate::vault::policy::LOCKED_MESSAGE
            ),
        ));
    }
    if proxies.is_empty() {
        return Ok(profile_errors::detail(
            StatusCode::BAD_REQUEST,
            if state.pool.list()?.is_empty() {
                "the proxy pool is empty"
            } else {
                "every proxy in the pool is archived"
            },
        ));
    }
    let targets = if body.get("profileIds").is_some_and(truth) {
        ids(body.get("profileIds"), false)?
    } else {
        state
            .dashboard
            .list()?
            .iter()
            .map(|profile| {
                profile["id"]
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| RepoError::InvalidRecord("Profile ID is not text".into()))
            })
            .collect::<Result<Vec<_>, _>>()?
    };
    let order: HashMap<_, _> = targets
        .iter()
        .enumerate()
        .map(|(index, id)| (id.as_str(), index))
        .collect();
    let catalog = &devices::catalog().map_err(std::io::Error::other)?["catalog"];
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(std::io::Error::other)?
        .as_secs_f64();
    let saved = state.dashboard.patch_many(&targets, |id, mut current| {
        let index = order.get(id).ok_or_else(|| RepoError::InvalidRecord("Assignment target changed".into()))?;
        let proxy = &proxies[*index % proxies.len()];
        let optional = |field: &str| if truth(&proxy[field]) { proxy[field].clone() } else { "".into() };
        current["proxy"] = json!({"type":proxy.get("type").unwrap_or(&Value::String("HTTP".into())),
            "host":proxy["host"], "port":value_text(&proxy["port"]), "user":optional("user"),
            "pass":optional("pass"), "country":optional("country")});
        let country = proxy["country"].as_str().unwrap_or("").to_uppercase();
        if let Some(locale) = catalog["COUNTRY_TO_LOCALE"][country].as_str() {
            current["locale"] = locale.into();
            current["timezone"] = catalog["LOCALES"][locale][0].clone();
        }
        Ok(Some(current))
    }, Map::new(), None, now)?;
    Ok(Json(json!({"assigned":saved.len(), "pool":proxies.len()})).into_response())
}

pub(super) async fn assign(
    State(state): State<Arc<Proxies>>,
    ObjectBody(body): ObjectBody,
) -> Response {
    match super::admission::blocking(move || assign_sync(&state, body)).await {
        Ok(Ok(response)) => response,
        Ok(Err(error)) => profile_errors::storage(error),
        Err(_) => super::proxy_routes::failure(),
    }
}
