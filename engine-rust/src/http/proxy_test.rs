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

struct Selection {
    targets: Vec<Value>,
    withheld: Vec<Value>,
}
fn select(state: &Proxies, body: &Map<String, Value>) -> Result<Selection, RepoError> {
    let rows = state.pool.list()?;
    let ids = match body.get("ids").filter(|value| truth(value)) {
        None => rows.iter().map(|row| row["id"].clone()).collect(),
        Some(Value::Array(values)) => values.clone(),
        Some(Value::String(text)) => text.chars().map(|c| Value::String(c.to_string())).collect(),
        Some(Value::Object(object)) => object.keys().cloned().map(Value::String).collect(),
        _ => {
            return Err(RepoError::InvalidRecord(
                "Proxy selection is not iterable".into(),
            ));
        }
    };
    let by_id: HashMap<_, _> = rows
        .iter()
        .filter_map(|row| row["id"].as_str().map(|id| (id, row)))
        .collect();
    let mut targets = Vec::new();
    let mut withheld = Vec::new();
    for id in ids {
        if id.is_array() || id.is_object() {
            return Err(RepoError::InvalidRecord(
                "Proxy selection contains unhashable IDs".into(),
            ));
        }
        let Some(proxy) = id.as_str().and_then(|id| by_id.get(id)) else {
            continue;
        };
        if state.pool.secret_withheld(id.as_str().unwrap_or(""))? {
            withheld.push((**proxy).clone());
        } else {
            targets.push((**proxy).clone());
        }
    }
    Ok(Selection { targets, withheld })
}
async fn execute(targets: Vec<Value>) -> std::io::Result<Vec<Value>> {
    execute_with(targets, |proxy| async move {
        crate::network::proxy_check::check(&proxy, Duration::from_secs(12)).await
    })
    .await
}
async fn execute_with<F, Fut>(targets: Vec<Value>, check: F) -> std::io::Result<Vec<Value>>
where
    F: Fn(Value) -> Fut + Clone + Send + 'static,
    Fut: std::future::Future<Output = Value> + Send + 'static,
{
    let count = targets.len();
    let mut pending = targets.into_iter().enumerate();
    let mut tasks = tokio::task::JoinSet::new();
    let mut results = Vec::with_capacity(count);
    loop {
        while tasks.len() < 8 {
            let Some((index, proxy)) = pending.next() else {
                break;
            };
            let check = check.clone();
            tasks.spawn(super::admission::owned(async move {
                let id = proxy["id"].clone();
                let mut result = check(proxy).await;
                result["status"] = if truth(&result["ok"]) {
                    "ok"
                } else if result["ok"].is_null() {
                    "skipped"
                } else {
                    "failed"
                }
                .into();
                result["id"] = id;
                (index, result)
            }));
        }
        let Some(joined) = tasks.join_next().await else {
            break;
        };
        match joined {
            Ok(result) => results.push(result),
            Err(_) => {
                tasks.shutdown().await;
                return Err(std::io::Error::other("Native proxy check task failed"));
            }
        }
    }
    results.sort_by_key(|(index, _)| *index);
    Ok(results.into_iter().map(|(_, value)| value).collect())
}
fn commit(
    state: &Proxies,
    mut results: Vec<Value>,
    withheld: Vec<Value>,
) -> Result<Value, RepoError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(std::io::Error::other)?
        .as_secs_f64();
    let mut patches = Map::new();
    for result in &results {
        let id = result["id"]
            .as_str()
            .ok_or_else(|| RepoError::InvalidRecord("Proxy result ID is not text".into()))?;
        let mut check = result
            .as_object()
            .ok_or_else(|| RepoError::InvalidRecord("Proxy result is not an object".into()))?
            .clone();
        check.remove("id");
        check.remove("status");
        patches.insert(
            id.into(),
            json!({"status":result["status"],"lastCheck":check,"checkedAt":now}),
        );
    }
    state.pool.update_many(patches)?;
    let skipped = withheld.len();
    for proxy in withheld {
        results.push(json!({"id":proxy["id"],"status":"skipped","error":"password locked; unlock to test this proxy"}));
    }
    Ok(
        json!({"tested":results.len(),"ok":results.iter().filter(|result| result["status"] == "ok").count(),"skipped":skipped,"results":results}),
    )
}
pub(super) async fn test(
    State(state): State<Arc<Proxies>>,
    ObjectBody(body): ObjectBody,
) -> Response {
    let selection = {
        let state = state.clone();
        match super::admission::blocking(move || select(&state, &body)).await {
            Ok(Ok(selection)) => selection,
            Ok(Err(error)) => return profile_errors::storage(error),
            Err(_) => return super::proxy_routes::failure(),
        }
    };
    if selection.targets.is_empty() && selection.withheld.is_empty() {
        return profile_errors::detail(StatusCode::BAD_REQUEST, "no proxies to test");
    }
    let results = match execute(selection.targets).await {
        Ok(results) => results,
        Err(_) => return super::proxy_routes::failure(),
    };
    match super::admission::blocking(move || commit(&state, results, selection.withheld)).await {
        Ok(Ok(result)) => Json(result).into_response(),
        Ok(Err(error)) => profile_errors::storage(error),
        Err(_) => super::proxy_routes::failure(),
    }
}

pub(super) async fn test_one(
    State(state): State<Arc<Proxies>>,
    Path(id): Path<String>,
) -> Response {
    let selected = {
        let state = state.clone();
        let id = id.clone();
        super::admission::blocking(move || {
            let proxy = state.pool.get(&id)?;
            let withheld = proxy.is_some() && state.pool.secret_withheld(&id)?;
            Ok::<_, RepoError>((proxy, withheld))
        })
        .await
    };
    let (proxy, withheld) = match selected {
        Ok(Ok(selected)) => selected,
        Ok(Err(error)) => return profile_errors::storage(error),
        Err(_) => return super::proxy_routes::failure(),
    };
    let Some(proxy) = proxy else {
        return profile_errors::detail(StatusCode::NOT_FOUND, "proxy not found");
    };
    if withheld {
        return profile_errors::detail(
            StatusCode::CONFLICT,
            format!(
                "this proxy has a password that cannot be read: {}",
                crate::vault::policy::LOCKED_MESSAGE
            ),
        );
    }
    let result = crate::network::proxy_check::check(&proxy, Duration::from_secs(12)).await;
    let status = if truth(&result["ok"]) {
        "ok"
    } else if result["ok"].is_null() {
        "skipped"
    } else {
        "failed"
    };
    match super::admission::blocking(move || {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(std::io::Error::other)?
            .as_secs_f64();
        state.pool.update(
            &id,
            Map::from_iter([
                ("status".into(), status.into()),
                ("lastCheck".into(), result.clone()),
                ("checkedAt".into(), now.into()),
            ]),
        )?;
        let mut result = result;
        result["status"] = status.into();
        Ok::<_, RepoError>(result)
    })
    .await
    {
        Ok(Ok(result)) => Json(result).into_response(),
        Ok(Err(error)) => profile_errors::storage(error),
        Err(_) => super::proxy_routes::failure(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };
    use tokio::sync::{Barrier, oneshot};

    struct Active(Arc<AtomicUsize>);
    impl Drop for Active {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::SeqCst);
        }
    }
    #[test]
    fn network_workers_preserve_order_ceiling_and_cancel_owned_tasks_on_failure()
    -> std::io::Result<()> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()?;
        runtime.block_on(async {
            let active = Arc::new(AtomicUsize::new(0));
            let peak = Arc::new(AtomicUsize::new(0));
            let barrier = Arc::new(Barrier::new(8));
            let completion = Arc::new(Mutex::new(Vec::new()));
            let (senders, receivers): (Vec<_>, Vec<_>) =
                (0..7).map(|_| oneshot::channel::<()>()).unzip();
            let gates = Arc::new(Mutex::new((
                senders.into_iter().map(Some).collect::<Vec<_>>(),
                receivers.into_iter().map(Some).collect::<Vec<_>>(),
            )));
            let rows = (0..20).map(|id| json!({"id":id.to_string()})).collect();
            let workers = active.clone();
            let max = peak.clone();
            let observed = completion.clone();
            let result = tokio::time::timeout(
                Duration::from_secs(2),
                execute_with(rows, move |proxy| {
                    let workers = workers.clone();
                    let max = max.clone();
                    let barrier = barrier.clone();
                    let gates = gates.clone();
                    let observed = observed.clone();
                    async move {
                        let count = workers.fetch_add(1, Ordering::SeqCst) + 1;
                        let _owner = Active(workers);
                        max.fetch_max(count, Ordering::SeqCst);
                        let index = proxy["id"]
                            .as_str()
                            .expect("Fixture ID")
                            .parse::<usize>()
                            .expect("Fixture index");
                        if index < 8 {
                            let (send, receive) = {
                                let mut gates = gates.lock().expect("Fixture gates");
                                (
                                    index.checked_sub(1).and_then(|i| gates.0[i].take()),
                                    gates.1.get_mut(index).and_then(Option::take),
                                )
                            };
                            barrier.wait().await;
                            if let Some(receive) = receive {
                                receive.await.expect("Fixture predecessor");
                            }
                            observed.lock().expect("Fixture completions").push(index);
                            if let Some(send) = send {
                                send.send(()).expect("Fixture successor");
                            }
                        }
                        json!({"ok":true,"ip":proxy["id"]})
                    }
                }),
            )
            .await
            .map_err(|_| std::io::Error::other("Worker fixture timed out"))??;
            assert_eq!(peak.load(Ordering::SeqCst), 8);
            assert_eq!(active.load(Ordering::SeqCst), 0);
            assert_eq!(
                *completion.lock().expect("Fixture completions"),
                (0..8).rev().collect::<Vec<_>>()
            );
            for (index, result) in result.iter().enumerate() {
                assert_eq!(result["id"], index.to_string());
                assert_eq!(result["ip"], result["id"]);
                assert_eq!(result["status"], "ok");
            }
            assert_eq!(result.len(), 20);

            let (started, waiting) = oneshot::channel::<()>();
            let started = Arc::new(Mutex::new(Some(started)));
            let waiting = Arc::new(Mutex::new(Some(waiting)));
            let admitted = Arc::new(AtomicUsize::new(0));
            let seen = admitted.clone();
            let workers = active.clone();
            let error = tokio::time::timeout(
                Duration::from_secs(2),
                execute_with(
                    vec![json!({"id":"panic"}), json!({"id":"wait"})],
                    move |proxy| {
                        let workers = workers.clone();
                        let seen = seen.clone();
                        let started = started.clone();
                        let waiting = waiting.clone();
                        async move {
                            workers.fetch_add(1, Ordering::SeqCst);
                            seen.fetch_add(1, Ordering::SeqCst);
                            let _owner = Active(workers);
                            if proxy["id"] == "panic" {
                                let waiting = waiting
                                    .lock()
                                    .expect("Fixture receiver")
                                    .take()
                                    .expect("Single receiver");
                                waiting.await.expect("Pending worker never admitted");
                                panic!("Fixture failure");
                            }
                            started
                                .lock()
                                .expect("Fixture sender")
                                .take()
                                .expect("Single sender")
                                .send(())
                                .expect("Panic worker gone");
                            std::future::pending::<Value>().await
                        }
                    },
                ),
            )
            .await
            .map_err(|_| std::io::Error::other("Failed worker cleanup timed out"))?
            .expect_err("Worker panic ignored");
            assert!(error.to_string().contains("task failed"));
            assert_eq!(admitted.load(Ordering::SeqCst), 2);
            assert_eq!(
                active.load(Ordering::SeqCst),
                0,
                "Aborted worker owner survived"
            );
            Ok(())
        })
    }
}
