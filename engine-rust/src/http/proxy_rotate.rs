use super::{profile_errors, proxy_routes::Proxies, request_body::ObjectBody};
use crate::{conversion::truth, storage::repo::RepoError};
use axum::{
    Json,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde_json::{Map, Value, json};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

fn timestamp() -> Result<Value, RepoError> {
    Ok(SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(std::io::Error::other)?
        .as_secs_f64()
        .into())
}
fn record(state: &Proxies, id: &str, result: &Value) -> Result<(), RepoError> {
    let mut patch = Map::new();
    if truth(&result["attempted"]) {
        patch.insert("rotateTriedAt".into(), timestamp()?);
    }
    if truth(&result["rotated"]) {
        patch.insert("rotatedAt".into(), timestamp()?);
    }
    if !result["ok"].is_null() {
        patch.insert(
            "status".into(),
            if truth(&result["ok"]) { "ok" } else { "failed" }.into(),
        );
        patch.insert("lastCheck".into(), result.clone());
        patch.insert("checkedAt".into(), timestamp()?);
    }
    if !patch.is_empty() {
        state.pool.update(id, patch)?;
    }
    Ok(())
}
fn locked(count: Option<usize>) -> Response {
    let prefix = count.map_or_else(
        || "this proxy has a password".into(),
        |count| format!("{count} of these proxies have a password"),
    );
    profile_errors::detail(
        StatusCode::CONFLICT,
        format!(
            "{prefix} that cannot be read: {}",
            crate::vault::policy::LOCKED_MESSAGE
        ),
    )
}
fn select(state: &Proxies, ids: &Value) -> Result<(Vec<Value>, usize), RepoError> {
    let values: Vec<Value> = if !truth(ids) {
        Vec::new()
    } else {
        match ids {
            Value::Array(values) => values.clone(),
            Value::String(text) => text.chars().map(|c| c.to_string().into()).collect(),
            Value::Object(values) => values.keys().cloned().map(Value::String).collect(),
            _ => {
                return Err(RepoError::InvalidRecord(
                    "Proxy selection is not iterable".into(),
                ));
            }
        }
    };
    let rows = state.pool.list()?;
    let by_id: HashMap<_, _> = rows
        .iter()
        .filter_map(|row| row["id"].as_str().map(|id| (id, row)))
        .collect();
    let mut targets = Vec::new();
    let mut hidden = 0;
    for id in values {
        if id.is_array() || id.is_object() {
            return Err(RepoError::InvalidRecord(
                "Proxy selection contains unhashable IDs".into(),
            ));
        }
        if let Some(proxy) = id.as_str().and_then(|id| by_id.get(id)) {
            hidden += usize::from(state.pool.secret_withheld(id.as_str().unwrap_or(""))?);
            targets.push((**proxy).clone());
        }
    }
    Ok((targets, hidden))
}

pub(super) async fn rotate_one(
    State(state): State<Arc<Proxies>>,
    Path(id): Path<String>,
) -> Response {
    let selection = {
        let state = state.clone();
        let id = id.clone();
        super::admission::blocking(move || {
            let proxy = state.pool.get(&id)?;
            let hidden = proxy.is_some() && state.pool.secret_withheld(&id)?;
            Ok::<_, RepoError>((proxy, hidden))
        })
        .await
    };
    let (proxy, hidden) = match selection {
        Ok(Ok(selection)) => selection,
        Ok(Err(error)) => return profile_errors::storage(error),
        Err(_) => return super::proxy_routes::failure(),
    };
    let Some(proxy) = proxy else {
        return profile_errors::detail(StatusCode::NOT_FOUND, "proxy not found");
    };
    if hidden {
        return locked(None);
    }
    let result = crate::network::proxy_rotate::rotate(&proxy, Duration::from_secs(20)).await;
    match super::admission::blocking(move || {
        record(&state, &id, &result)?;
        Ok::<_, RepoError>(result)
    })
    .await
    {
        Ok(Ok(result)) => Json(result).into_response(),
        Ok(Err(error)) => profile_errors::storage(error),
        Err(_) => super::proxy_routes::failure(),
    }
}

pub(super) async fn rotate_many(
    State(state): State<Arc<Proxies>>,
    ObjectBody(body): ObjectBody,
) -> Response {
    let selection = {
        let state = state.clone();
        super::admission::blocking(move || select(&state, body.get("ids").unwrap_or(&Value::Null)))
            .await
    };
    let (targets, hidden) = match selection {
        Ok(Ok(selection)) => selection,
        Ok(Err(error)) => return profile_errors::storage(error),
        Err(_) => return super::proxy_routes::failure(),
    };
    if targets.is_empty() {
        return profile_errors::detail(StatusCode::BAD_REQUEST, "no proxies to rotate");
    }
    if hidden > 0 {
        return locked(Some(hidden));
    }
    let mut results = Vec::new();
    for proxy in targets {
        let result = crate::network::proxy_rotate::rotate(&proxy, Duration::from_secs(20)).await;
        let saved = {
            let state = state.clone();
            super::admission::blocking(move || {
                let id = proxy["id"]
                    .as_str()
                    .ok_or_else(|| RepoError::InvalidRecord("Proxy ID is not text".into()))?;
                record(&state, id, &result)?;
                let mut result = result;
                result["id"] = proxy["id"].clone();
                Ok::<_, RepoError>(result)
            })
            .await
        };
        match saved {
            Ok(Ok(result)) => results.push(result),
            Ok(Err(error)) => return profile_errors::storage(error),
            Err(_) => return super::proxy_routes::failure(),
        }
    }
    Json(json!({"rotated":results.iter().filter(|result| truth(&result["rotated"])).count(),"tried":results.len(),"results":results})).into_response()
}
