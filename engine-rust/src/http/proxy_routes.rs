use super::{
    cors, middleware::protect, request_auth::RequestAuth, request_body::ObjectBody,
    tokens::TokenStore,
};
use crate::{
    conversion::truth,
    storage::{dashboard::DashboardStore, pool::ProxyPool, repo::RepoError},
};
use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post, put},
};
use serde_json::{Map, Value, json};
use std::{collections::HashMap, path::PathBuf, sync::Arc};

pub(super) struct Proxies {
    pub(super) pool: ProxyPool,
    pub(super) dashboard: DashboardStore,
}
pub fn router(root: PathBuf, environment: &str) -> Router {
    let auth = Arc::new(RequestAuth::new(TokenStore::new(root.clone()), environment));
    protect(cors::apply(routes(root, None), ""), auth)
}
pub(crate) fn routes(root: PathBuf, password: Option<String>) -> Router {
    let state = Arc::new(Proxies {
        pool: ProxyPool::new(root.clone(), password.clone()),
        dashboard: DashboardStore::new(root, password),
    });
    Router::new()
        .route(
            "/api/proxies",
            get(list)
                .head(super::application::method_not_allowed)
                .post(add),
        )
        .route("/api/proxies/{id}", put(edit).delete(delete_one))
        .route("/api/proxies/delete", post(delete))
        .route("/api/proxies/export", post(export))
        .route(
            "/api/proxies/archive",
            post(super::proxy_assignment::archive),
        )
        .route("/api/proxies/assign", post(super::proxy_assignment::assign))
        .route("/api/proxies/test", post(super::proxy_test::test))
        .route("/api/proxies/{id}/test", post(super::proxy_test::test_one))
        .route(
            "/api/proxies/{id}/rotate",
            post(super::proxy_rotate::rotate_one),
        )
        .route(
            "/api/proxies/rotate",
            post(super::proxy_rotate::rotate_many),
        )
        .with_state(state)
}
async fn list(State(state): State<Arc<Proxies>>) -> Response {
    match tokio::task::spawn_blocking(move || {
        let mut usage = HashMap::<String, usize>::new();
        for profile in state.dashboard.list()? {
            let proxy = &profile["proxy"];
            if truth(&proxy["host"]) && truth(&proxy["port"]) {
                *usage
                    .entry(crate::pool::identity(proxy).to_string())
                    .or_default() += 1;
            }
        }
        let mut rows = state.pool.list()?;
        for row in &mut rows {
            row["profiles"] = usage
                .get(&crate::pool::identity(row).to_string())
                .copied()
                .unwrap_or(0)
                .into();
        }
        Ok::<_, RepoError>(rows)
    })
    .await
    {
        Ok(Ok(rows)) => Json(rows).into_response(),
        _ => failure(),
    }
}
const ALLOWED: &[&str] = &[
    "archived",
    "country",
    "host",
    "label",
    "pass",
    "port",
    "rotateUrl",
    "type",
    "user",
];
async fn edit(
    State(state): State<Arc<Proxies>>,
    Path(id): Path<String>,
    ObjectBody(body): ObjectBody,
) -> Response {
    match tokio::task::spawn_blocking(move || edit_row(&state, &id, body)).await {
        Ok(Ok(response)) => response,
        Ok(Err(RepoError::VaultLocked(message))) => error(StatusCode::LOCKED, &message),
        _ => failure(),
    }
}
fn edit_row(state: &Proxies, id: &str, body: Map<String, Value>) -> Result<Response, RepoError> {
    let Some(mut current) = state.pool.get(id)? else {
        return Ok(error(StatusCode::NOT_FOUND, "proxy not found"));
    };
    let mut patch: Map<String, Value> = body
        .into_iter()
        .filter(|(key, _)| ALLOWED.contains(&key.as_str()))
        .collect();
    if patch.is_empty() {
        return Ok(error(
            StatusCode::BAD_REQUEST,
            "nothing to change - editable fields are ['archived', 'country', 'host', 'label', 'pass', 'port', 'rotateUrl', 'type', 'user']",
        ));
    }
    current
        .as_object_mut()
        .ok_or_else(|| RepoError::InvalidRecord("Stored proxy is not an object".into()))?
        .extend(patch.clone());
    let clean = match crate::pool::normalize(&current) {
        Ok(clean) => clean,
        Err(problem) => return Ok(error(StatusCode::BAD_REQUEST, &problem.to_string())),
    };
    for field in ["type", "host", "port", "user", "country"] {
        if patch.contains_key(field) {
            patch.insert(field.into(), clean[field].clone());
        }
    }
    if ["host", "port", "user", "pass", "type"]
        .iter()
        .any(|field| patch.contains_key(*field))
    {
        patch.insert("status".into(), "untested".into());
        patch.insert("lastCheck".into(), Value::Null);
        patch.insert("checkedAt".into(), Value::Null);
    }
    Ok(Json(state.pool.update(id, patch)?).into_response())
}
async fn delete(State(state): State<Arc<Proxies>>, ObjectBody(body): ObjectBody) -> Response {
    let Some(ids) = body
        .get("ids")
        .and_then(Value::as_array)
        .filter(|ids| !ids.is_empty())
        .cloned()
    else {
        return error(
            StatusCode::BAD_REQUEST,
            "give a list of proxy ids to delete",
        );
    };
    match tokio::task::spawn_blocking(move || {
        let mut removed = Vec::new();
        for id in ids {
            if let Some(id) = id.as_str()
                && state.pool.delete(id)?
            {
                removed.push(id.to_owned());
            }
        }
        Ok::<_, RepoError>(
            json!({"deleted":removed.len(),"ids":removed,"total":state.pool.list()?.len()}),
        )
    })
    .await
    {
        Ok(Ok(result)) => Json(result).into_response(),
        _ => failure(),
    }
}
async fn delete_one(State(state): State<Arc<Proxies>>, Path(id): Path<String>) -> Response {
    match tokio::task::spawn_blocking(move || state.pool.delete(&id)).await {
        Ok(Ok(true)) => Json(json!({"ok":true})).into_response(),
        Ok(Ok(false)) => error(StatusCode::NOT_FOUND, "proxy not found"),
        Ok(Err(RepoError::VaultLocked(message))) => error(StatusCode::LOCKED, &message),
        _ => failure(),
    }
}
async fn add(State(state): State<Arc<Proxies>>, ObjectBody(body): ObjectBody) -> Response {
    match tokio::task::spawn_blocking(move || {
        let incoming = match body.get("proxies").filter(|value| !value.is_null()) {
            Some(incoming) => incoming.clone(),
            None => match body.get("text").filter(|value| truth(value)) {
                None => Value::Array(Vec::new()),
                Some(Value::String(text)) => Value::Array(crate::pool::parse_many(text)),
                _ => return Err(RepoError::InvalidRecord("Proxy text is not text".into())),
            },
        };
        let parsed = match crate::pool::normalize_many(&incoming) {
            Ok(parsed) => parsed,
            Err(problem) => return Ok(error(StatusCode::BAD_REQUEST, &problem.to_string())),
        };
        if parsed.is_empty() {
            return Ok(error(
                StatusCode::BAD_REQUEST,
                "no proxies could be parsed from that input",
            ));
        }
        let added = state.pool.add_many(parsed)?;
        Ok::<_, RepoError>(
            Json(json!({"added":added.len(),"total":state.pool.list()?.len(),"proxies":added}))
                .into_response(),
        )
    })
    .await
    {
        Ok(Ok(response)) => response,
        Ok(Err(RepoError::VaultLocked(message))) => error(StatusCode::LOCKED, &message),
        _ => failure(),
    }
}
async fn export(State(state): State<Arc<Proxies>>, ObjectBody(body): ObjectBody) -> Response {
    match tokio::task::spawn_blocking(move || {
        let mut rows = state.pool.list()?;
        if let Some(ids) = body.get("ids").filter(|value| truth(value)) {
            let keep = match ids {
                Value::Array(ids) => ids.clone(),
                Value::String(text) => text.chars().map(|c| Value::String(c.to_string())).collect(),
                Value::Object(object) => object.keys().cloned().map(Value::String).collect(),
                _ => {
                    return Err(RepoError::InvalidRecord(
                        "Proxy selection is not iterable".into(),
                    ));
                }
            };
            if keep.iter().any(|id| id.is_array() || id.is_object()) {
                return Err(RepoError::InvalidRecord(
                    "Proxy selection contains unhashable IDs".into(),
                ));
            }
            rows.retain(|row| keep.contains(&row["id"]));
        }
        let mut withheld = 0;
        for row in &rows {
            let id = row["id"]
                .as_str()
                .ok_or_else(|| RepoError::InvalidRecord("Proxy ID is not text".into()))?;
            withheld += usize::from(state.pool.secret_withheld(id)?);
        }
        if withheld > 0 {
            return Ok(error(
                StatusCode::CONFLICT,
                &format!(
                    "{withheld} of these proxies have a password that cannot be read: {}",
                    crate::vault::policy::LOCKED_MESSAGE
                ),
            ));
        }
        let lines = rows
            .iter()
            .map(crate::pool::to_line)
            .collect::<Result<Vec<_>, _>>()?;
        let text = if lines.is_empty() {
            String::new()
        } else {
            format!("{}\n", lines.join("\n"))
        };
        Ok::<_, RepoError>(Json(json!({"count":lines.len(),"text":text})).into_response())
    })
    .await
    {
        Ok(Ok(response)) => response,
        _ => failure(),
    }
}
fn error(status: StatusCode, detail: &str) -> Response {
    (status, Json(json!({"detail":detail}))).into_response()
}
pub(super) fn failure() -> Response {
    (StatusCode::INTERNAL_SERVER_ERROR, "Internal Server Error").into_response()
}
