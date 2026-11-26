use super::*;

pub(super) fn value_of(el: &Element) -> String {
    if let Some(input) = el.dyn_ref::<HtmlInputElement>() {
        input.value()
    } else if let Some(input) = el.dyn_ref::<HtmlSelectElement>() {
        input.value()
    } else if let Some(input) = el.dyn_ref::<HtmlTextAreaElement>() {
        input.value()
    } else {
        String::new()
    }
}
pub fn edit(scope: &str, key: &str, value: &str) -> Result<(), String> {
    change(|a| match scope {
        "app" => match key {
            "query" => {
                a.query = value.into();
                a.page = 0
            }
            "command" => {
                a.command = value.into();
                a.command_index = 0
            }
            _ => {}
        },
        "editor" => {
            if let Some(ed) = a.editor.as_mut() {
                if !ed.editable() {
                    return;
                }
                match key {
                    "os" => change_os(&mut ed.draft, value),
                    "locale" => change_locale(&mut ed.draft, value),
                    "tags" => ed.draft["tags"] = json!(split_tags(value)),
                    "webglRenderer" => {
                        ed.draft[key] = json!(value);
                        ed.draft["webglVendor"] = json!(vendor(value));
                    }
                    _ => {
                        let v = if [
                            "cores",
                            "memory",
                            "mediaVideo",
                            "mediaAudio",
                            "mediaSpeakers",
                        ]
                        .contains(&key)
                        {
                            value
                                .parse::<i64>()
                                .map(|n| json!(n))
                                .unwrap_or_else(|_| json!(value.parse::<f64>().unwrap_or(0.)))
                        } else if key == "dns" && value == "custom" {
                            json!("https://")
                        } else {
                            json!(value)
                        };
                        ed.set(&key.split('.').map(str::to_owned).collect::<Vec<_>>(), v);
                    }
                }
            }
        }
        "form" => {
            a.form[key] = json!(value);
            if key == "poolPick"
                && let Some(p) = a.pool.iter().find(|p| s(p, "id") == value)
            {
                let proxy = json!({"type":p["type"],"host":p["host"],"port":text(&p["port"]),"user":p["user"].as_str().unwrap_or(""),"pass":p["pass"].as_str().unwrap_or(""),"country":p["country"].as_str().unwrap_or("")});
                if let Some(ed) = a.editor.as_mut()
                    && ed.editable()
                {
                    ed.draft["proxy"] = proxy;
                    ed.draft["_passwordSet"] = json!(true);
                }
            }
        }
        "batch" => match key {
            "count" => a.batch.count = value.parse::<usize>().unwrap_or(1).clamp(1, 10000),
            "start" => a.batch.start = value.parse::<usize>().unwrap_or(0),
            "prefix" => a.batch.prefix = value.into(),
            "tags" => a.batch.tags = value.into(),
            "engine" => a.batch.engine = value.into(),
            _ => {}
        },
        "schedule" => {
            let id = s(&a.schedule, "id").to_owned();
            let key = key.to_owned();
            let value = if key == "everyHours" {
                json!(value.parse::<u64>().unwrap_or(1).max(1))
            } else {
                json!(value)
            };
            spawn_local(async move {
                let _ = request(
                    "PUT",
                    &format!("/api/schedules/{id}"),
                    Some(json!({key:value})),
                )
                .await
                .map(|s| change(|a| a.schedule = s));
                draw();
            });
        }
        _ => {}
    });
    Ok(())
}
pub(super) async fn load_view() -> Result<(), ApiError> {
    let a = snapshot();
    if !a.live() {
        return Ok(());
    }
    match a.view.as_str() {
        "proxies" => {
            let p = request("GET", "/api/proxies", None).await?;
            change(|a| {
                a.pool = list(&p);
                a.pool_selected
                    .retain(|id| a.pool.iter().any(|p| s(p, "id") == id));
            });
        }
        "health" => {
            let h = request("GET", "/api/profiles/health", None).await?;
            change(|a| a.health = h);
        }
        "settings" => {
            let t = request("GET", "/api/tokens", None).await?;
            change(|a| a.tokens = t);
            if a.release_preferences.is_null() {
                release::refresh_release_preferences().await;
            }
        }
        "extensions" => load_extensions().await?,
        _ => {}
    }
    Ok(())
}
pub(super) async fn load_extensions() -> Result<(), ApiError> {
    let v = request("GET", "/api/extensions/installed", None).await?;
    let extensions = list(&v["extensions"]);
    change(|a| a.extensions = extensions.clone());
    for meta in extensions {
        let path = s(&meta, "path");
        if s(&meta, "icon").is_empty() || snapshot().icons.contains_key(path) {
            continue;
        }
        let deadline = Deadline::new(10000)?;
        let url = format!(
            "/api/extensions/icon?path={}",
            js_sys::encode_uri_component(path)
        );
        let response = send(
            "GET",
            &contracts::api_url(&url),
            None,
            &deadline,
            &auth(),
            false,
        )
        .await?;
        if response.status() == 401 {
            let err = contracts::error(
                "GET",
                &url,
                401,
                &parse(&response).await.unwrap_or(Value::Null),
            );
            change(|a| {
                a.mode = "locked".into();
                a.auth_error = err.message;
            });
            break;
        }
        if !response.ok() {
            continue;
        }
        let blob = JsFuture::from(response.blob().map_err(network)?)
            .await
            .map_err(network)?
            .dyn_into::<Blob>()
            .map_err(network)?;
        let made = web_sys::Url::create_object_url_with_blob(&blob).map_err(network)?;
        if snapshot().view != "extensions" {
            let _ = web_sys::Url::revoke_object_url(&made);
        } else {
            change(|a| {
                a.icons.insert(path.into(), made);
            });
        }
    }
    Ok(())
}
pub(super) fn release_icons() {
    change(|a| {
        for url in a.icons.values() {
            let _ = web_sys::Url::revoke_object_url(url);
        }
        a.icons.clear();
    });
}
pub(super) async fn open_editor(p: Value) {
    change(|a| a.open_editor(p));
    draw();
    if snapshot().live() {
        if let Ok(pool) = request("GET", "/api/proxies", None).await {
            change(|a| a.pool = list(&pool));
        }
        if let Ok(schedules) = request("GET", "/api/schedules", None).await {
            let id = snapshot()
                .editor
                .as_ref()
                .map(|e| s(&e.draft, "id").to_owned())
                .unwrap_or_default();
            change(|a| {
                a.schedule = list(&schedules)
                    .into_iter()
                    .find(|p| s(p, "profileId") == id)
                    .unwrap_or(Value::Null)
            });
        }
    }
}
pub(super) fn confirm(operation: &str, ids: Vec<String>, title: &str, body: &str, label: &str) {
    change(|a| {
        a.modal = "confirm".into();
        a.form = json!({"operation":operation,"ids":ids,"title":title,"body":body,"label":label});
    });
}
pub(super) fn chosen_profiles(id: &str) -> Vec<Value> {
    let a = snapshot();
    if id.is_empty() {
        a.selected_profiles()
    } else {
        a.profiles
            .into_iter()
            .filter(|p| s(p, "id") == id)
            .collect()
    }
}
pub(super) fn chosen_proxies(id: &str, all: bool) -> Vec<Value> {
    let a = snapshot();
    a.pool
        .into_iter()
        .filter(|p| {
            all || if id.is_empty() {
                a.pool_selected.contains(s(p, "id"))
            } else {
                s(p, "id") == id
            }
        })
        .collect()
}
pub(super) fn preset() -> String {
    let a = snapshot();
    if s(&a.form, "preset").is_empty() {
        "general".into()
    } else {
        s(&a.form, "preset").into()
    }
}
pub(super) async fn save() -> Result<(), ApiError> {
    let a = snapshot();
    let Some(mut ed) = a.editor.clone() else {
        return Ok(());
    };
    let Some(p) = ed.begin_save(a.live()) else {
        return Ok(());
    };
    change(|a| a.editor = Some(ed.clone()));
    draw();
    if !a.live() {
        let id = s(&ed.draft, "id").to_owned();
        let before = a.profiles.iter().find(|p| s(p, "id") == id);
        let mut saved = without_marker(p);
        saved["_new"] = json!(false);
        change(|a| {
            if !s(&saved, "engine").is_empty()
                && before.is_none_or(|b| b["engine"] != saved["engine"])
            {
                a.config["default_engine"] = saved["engine"].clone();
            }
            if let Some(index) = a.profiles.iter().position(|p| s(p, "id") == id) {
                a.profiles[index] = saved;
            } else {
                a.profiles.push(saved);
            }
            a.editor = None;
        });
        return Ok(());
    }
    let existing = ed.draft["_new"] != true && a.profiles.iter().any(|x| x["id"] == ed.draft["id"]);
    let id = s(&ed.draft, "id");
    let result = if existing {
        request("PUT", &format!("/api/profiles/{id}"), Some(for_write(&p))).await
    } else {
        let mut p = without_marker(p);
        p.as_object_mut().map(|p| p.remove("id"));
        request("POST", "/api/profiles", Some(p)).await
    };
    match result {
        Ok(saved) => {
            change(|a| a.commit_profile(saved));
            if let Err(err) = refresh().await {
                change(|a| a.say(format!("Profile saved, but refresh failed: {err}"), true));
            }
            let _ = refresh_config().await;
        }
        Err(err) if err.status == 409 && err.kind == "StaleProfile" => {
            match request("GET", "/api/profiles", None).await {
                Ok(latest) => {
                    if let Some(latest) = list(&latest).into_iter().find(|p| s(p, "id") == id) {
                        change(|a| {
                            if let Some(ed) = a.editor.as_mut() {
                                ed.stale(latest);
                            }
                        });
                    } else {
                        change(|a| {
                            if let Some(ed) = a.editor.as_mut() {
                                ed.saving = false;
                                ed.message="Could not load the latest profile: This profile was removed. Your draft is still open.".into();
                            }
                        });
                    }
                }
                Err(err) => change(|a| {
                    if let Some(ed) = a.editor.as_mut() {
                        ed.saving = false;
                        ed.message = format!(
                            "Could not load the latest profile: {err}. Your draft is preserved; retry when it is available."
                        );
                    }
                }),
            }
        }
        Err(err) => {
            change(|a| {
                if let Some(ed) = a.editor.as_mut() {
                    ed.failed(&err);
                }
                a.say(format!("Save failed: {err}"), true);
            });
            let _ = refresh_config().await;
        }
    }
    Ok(())
}
pub(super) fn local_id() -> String {
    format!(
        "local-{}-{}",
        js_sys::Date::now(),
        (js_sys::Math::random() * u32::MAX as f64) as u32
    )
}
pub(super) async fn profile_session(id: &str, launch: bool) -> Result<(), ApiError> {
    if snapshot().live() {
        request(
            "POST",
            &format!(
                "/api/profiles/{id}/{}",
                if launch { "launch" } else { "stop" }
            ),
            None,
        )
        .await?;
        refresh().await?;
    } else {
        change(|a| {
            if let Some(p) = a.profiles.iter_mut().find(|p| s(p, "id") == id) {
                p["status"] = json!(if launch { "running" } else { "stopped" });
                p["lastActive"] = json!("now");
            }
        });
    }
    Ok(())
}
pub(super) async fn bulk_sessions(warm: bool) -> Result<(), ApiError> {
    let a = snapshot();
    if a.busy {
        return Ok(());
    }
    if warm && !a.live() {
        change(|a| {
            a.say(
                "Warm-up runs against the live engine — start `numera serve` first.",
                false,
            )
        });
        return Ok(());
    }
    let selected = a.selected_profiles();
    let ids = if warm {
        selected
            .iter()
            .map(|p| s(p, "id").to_string())
            .collect::<Vec<_>>()
    } else {
        a.launch_ids()
    };
    if ids.is_empty() {
        change(|a| {
            a.say(
                format!(
                    "All {} selected profiles are already running.",
                    selected.len()
                ),
                false,
            )
        });
        return Ok(());
    }
    change(|a| a.busy = true);
    draw();
    let mut settled = vec![];
    let live = a.live();
    for chunk in ids.chunks(4) {
        let promises = chunk
            .iter()
            .map(|id| {
                let id = id.clone();
                wasm_bindgen_futures::future_to_promise(async move {
                    let result = if warm {
                        request(
                            "POST",
                            &format!("/api/profiles/{id}/warmup"),
                            Some(json!({"minutes":5,"preset":"general"})),
                        )
                        .await
                        .map(|_| ())
                    } else if live {
                        request("POST", &format!("/api/profiles/{id}/launch"), None)
                            .await
                            .map(|_| ())
                    } else {
                        profile_session(&id, true).await
                    };
                    Ok(JsValue::from_str(
                        &json!((id, result.err().map(|e| e.message))).to_string(),
                    ))
                })
            })
            .collect::<Array>();
        let results = JsFuture::from(js_sys::Promise::all(&promises))
            .await
            .map_err(network)?;
        for result in Array::from(&results).iter() {
            if let Some((id, error)) = result
                .as_string()
                .and_then(|s| serde_json::from_str::<(String, Option<String>)>(&s).ok())
            {
                settled.push((id, error.map_or(Ok(()), Err)));
            }
        }
    }
    change(|a| {
        let verb = if warm { "Warmed up" } else { "Launched" };
        let skipped = selected.len().saturating_sub(ids.len());
        a.bulk_result(verb, &settled, skipped);
    });
    if a.live() {
        let _ = refresh().await;
    }
    Ok(())
}

pub(super) async fn save_extensions(paths: Vec<String>) -> Result<(), ApiError> {
    let a = snapshot();
    if a.live() {
        let cfg = request(
            "PUT",
            "/api/config",
            Some(json!({"default_extensions":paths})),
        )
        .await?;
        change(|a| a.config = cfg);
        load_extensions().await?;
    } else {
        change(|a| a.config["default_extensions"] = json!(paths));
    }
    Ok(())
}
pub(super) async fn execute_confirmation() -> Result<(), ApiError> {
    let a = snapshot();
    if a.busy {
        return Ok(());
    }
    let ids = strings(&a.form["ids"]);
    change(|a| a.busy = true);
    draw();
    let result=async{match s(&a.form,"operation"){
"release-apply"=>{release::apply_release(&a,ids.first().map_or("",String::as_str)).await?;},
"delete-profiles"=>{if a.live(){let r=request("POST","/api/profiles/delete",Some(json!({"ids":ids}))).await?;let removed=list(&r["removed"]).len();let missing=list(&r["missing"]).len();let failed=list(&r["failed"]);change(|a|a.say(format!("Deleted {removed} profile(s). {} no longer existed. {} could not be deleted. {}",missing,failed.len(),failed.first().map(|f|s(f,"error")).unwrap_or("")),!failed.is_empty()||missing>0));let _=refresh().await;}else{change(|a|{a.profiles.retain(|p|!ids.contains(&s(p,"id").to_owned()));a.say(format!("Deleted {} profiles.",ids.len()),false);});}change(|a|a.selected.clear());},
"delete-folder"=>{let name=ids.first().map(String::as_str).unwrap_or("");if a.live(){let cfg=request("POST","/api/folders/delete",Some(json!({"name":name}))).await?;change(|a|a.config=cfg);refresh().await?;}else{change(|a|{a.config["folders"]=json!(a.folders().into_iter().filter(|f|f!=name).collect::<Vec<_>>());for p in &mut a.profiles{if s(p,"folder")==name{p["folder"]=json!("Unfiled");}}});}change(|a|{if a.folder==name{a.folder="All profiles".into();}});},"delete-proxies"=>{let r=request("POST","/api/proxies/delete",Some(json!({"ids":ids}))).await?;change(|a|a.say(format!("Deleted {} proxies.",number(&r,"deleted")),false));load_view().await?;},"assign-proxies"=>{let r=request("POST","/api/proxies/assign",Some(json!({}))).await?;change(|a|a.say(format!("Assigned proxies to {} profile(s).",number(&r,"assigned")),false));refresh().await?;load_view().await?;},"revoke-token"=>{if let Some(id)=ids.first(){request("DELETE",&format!("/api/tokens/{id}"),None).await?;change(|a|a.say("Revoked.",false));load_view().await?;}},_=>{}}Ok::<_,ApiError>(())}.await;
    change(|a| {
        a.busy = false;
        if result.is_ok() {
            a.modal.clear();
        }
    });
    if result.is_err() {
        let _ = refresh_config().await;
    }
    result
}
