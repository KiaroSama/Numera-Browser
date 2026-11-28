use super::*;

pub(super) async fn dispatch(act: &str, id: &str, a: &App) -> Result<bool, ApiError> {
    match act {
        "nav" => {
            release_icons();
            change(|a| {
                a.view = id.into();
                a.view_error = None;
                a.modal.clear();
                a.rename.clear();
            });
            draw();
            load_visible_view(false).await;
        }
        "retry-view" => load_visible_view(true).await,
        "folder" => change(|a| {
            a.folder = id.into();
            a.view = "profiles".into();
            a.view_error = None;
            a.page = 0;
            a.modal.clear();
        }),
        "theme" => {
            theme_sweep::stop(true);
            let from_dark = snapshot().theme != "light";
            let next = if from_dark { "light" } else { "dark" };
            theme_sweep::play(&window(), &document(), from_dark, move || {
                change(|app| app.theme = next.into());
                if let Ok(Some(storage)) = window().local_storage() {
                    let _ = storage.set_item("ps-theme", next);
                }
                draw();
            });
        }
        "new" => {
            let mut p = new_profile(&a.folder, a.default_engine());
            p["id"] = json!(local_id());
            open_editor(p).await;
        }
        "edit" => {
            if let Some(p) = a.profiles.iter().find(|p| s(p, "id") == id) {
                open_editor(p.clone()).await;
            }
        }
        "close-editor" => {
            if a.editor.as_ref().is_none_or(|e| !e.saving) {
                change(|a| a.editor = None);
            }
        }
        "save" => save().await?,
        "tab" => change(|a| {
            if let Some(ed) = a.editor.as_mut()
                && ed.editable()
            {
                ed.tab = id.into();
            }
        }),
        "resolve-mine" | "resolve-latest" => change(|a| {
            if let Some(ed) = a.editor.as_mut() {
                ed.resolve(id.parse().unwrap_or(0), act == "resolve-latest");
            }
        }),
        "toggle-proxy" => change(|a| {
            if let Some(ed) = a.editor.as_mut()
                && ed.editable()
            {
                if ed.draft["proxy"].is_object() {
                    ed.draft["proxy"] = Value::Null;
                } else {
                    ed.draft["proxy"] = json!({"type":"HTTP","host":"","port":"","user":"","pass":"","country":"US"});
                    ed.draft["_passwordSet"] = json!(true);
                }
            }
        }),
        "dnt" => change(|a| {
            if let Some(ed) = a.editor.as_mut() {
                let next = ed.draft["dnt"] != true;
                ed.set(&["dnt".into()], json!(next));
            }
        }),
        "pick" => {
            let parts = id.splitn(3, '|').collect::<Vec<_>>();
            if parts.len() == 3 {
                edit(parts[0], parts[1], parts[2]).map_err(fail)?;
            }
        }
        "ip" | "timezone-auto" | "timezone-manual" => change(|a| {
            if let Some(ed) = a.editor.as_mut() {
                if !ed.editable() {
                    return;
                }
                let signal = if act == "ip" { id } else { "timezone" };
                let mut v = strings(&ed.draft["ipBased"]);
                let on = v.contains(&signal.to_owned());
                if act == "timezone-manual" || on && act == "ip" {
                    v.retain(|v| v != signal);
                } else if ed.draft["proxy"].is_object() && !on {
                    v.push(signal.into());
                }
                ed.draft["ipBased"] = json!(v);
            }
        }),
        "keep" => {
            if let Some((key, value)) = id.split_once('|') {
                change(|a| {
                    if let Some(ed) = a.editor.as_mut() {
                        if !ed.editable() {
                            return;
                        }
                        let mut v = strings(&ed.draft[key]);
                        if v.contains(&value.into()) {
                            v.retain(|v| v != value);
                        } else {
                            v.push(value.into());
                        }
                        ed.draft[key] = json!(v);
                    }
                });
            }
        }
        "select" => change(|a| App::toggle_set(&mut a.selected, id.into())),
        "select-all" => change(App::toggle_all),
        "clear-selection" => change(|a| a.selected.clear()),
        "filters" => change(|a| a.filter_open = !a.filter_open),
        "filter" => {
            if let Some((key, value)) = id.split_once('|') {
                change(|a| {
                    let v = if a.filters.get(key).is_some_and(|v| v == value) {
                        String::new()
                    } else {
                        value.into()
                    };
                    a.filters.insert(key.into(), v);
                    a.page = 0;
                });
            }
        }
        "clear-filters" | "clear-search" => change(|a| {
            a.filters.clear();
            a.query.clear();
            a.page = 0;
        }),
        "page" => change(|a| a.page = id.parse().unwrap_or(0)),
        "run" => {
            let running = a
                .profiles
                .iter()
                .find(|p| s(p, "id") == id)
                .is_some_and(|p| s(p, "status") == "running");
            if let Err(e) = profile_session(id, !running).await {
                change(|a| {
                    a.say(
                        format!("{} failed: {e}", if running { "Stop" } else { "Launch" }),
                        true,
                    )
                });
                let _ = refresh().await;
            }
        }
        "clone" => {
            if let Some(p) = a.profiles.iter().find(|p| s(p, "id") == id) {
                let mut copy = p.clone();
                copy.as_object_mut().map(|p| p.remove("id"));
                copy["name"] = json!(format!("{}-copy", text(&p["name"])));
                copy["status"] = json!("stopped");
                copy["lastActive"] = json!("never");
                copy["_new"] = json!(true);
                if a.live() {
                    request("POST", "/api/profiles", Some(without_marker(copy))).await?;
                    refresh().await?;
                } else {
                    copy["id"] = json!(local_id());
                    change(|a| a.profiles.push(copy));
                }
            }
        }
        "rename-profile" => change(|a| {
            a.rename = id.into();
            a.form["rename"] = a
                .profiles
                .iter()
                .find(|p| s(p, "id") == id)
                .map(|p| json!(text(&p["name"])))
                .unwrap_or(Value::Null);
        }),
        "commit-rename" => {
            if let Some(p) = a.profiles.iter().find(|p| s(p, "id") == a.rename) {
                let name = s(&a.form, "rename").trim();
                if !name.is_empty() && p["name"] != name {
                    let mut patch = p.clone();
                    patch["name"] = json!(name);
                    if a.live() {
                        match request(
                            "PUT",
                            &format!("/api/profiles/{}", a.rename),
                            Some(for_write(&patch)),
                        )
                        .await
                        {
                            Ok(saved) => change(|a| {
                                if let Some(p) =
                                    a.profiles.iter_mut().find(|p| s(p, "id") == a.rename)
                                {
                                    *p = saved;
                                }
                            }),
                            Err(err) => change(|a| a.say(err.message, true)),
                        }
                    } else {
                        change(|a| {
                            if let Some(p) = a.profiles.iter_mut().find(|p| s(p, "id") == a.rename)
                            {
                                p["name"] = json!(name);
                            }
                        });
                    }
                }
            }
            change(|a| a.rename.clear());
        }
        "bulk-launch" => bulk_sessions(false).await?,
        "bulk-warmup" => bulk_sessions(true).await?,
        "delete-profile" | "delete-selected" => {
            let rows = chosen_profiles(if act == "delete-profile" { id } else { "" });
            let n = rows.len();
            let running = rows.iter().filter(|p| s(p, "status") == "running").count();
            confirm(
                "delete-profiles",
                rows.iter().map(|p| s(p, "id").into()).collect(),
                &if n == 1 {
                    "Delete this profile?".into()
                } else {
                    format!("Delete {n} profiles?")
                },
                &format!(
                    "Deleting {n} profiles. The browser session goes too — cookies, logins and local storage. There is no undo. {running} of them are running right now."
                ),
                "Delete",
            );
        }
        "export-profiles" => {
            let (file, msg) = export_profiles(&a.selected_profiles());
            download(
                &format!("numera-profiles-{}.json", a.selected_profiles().len()),
                &serde_json::to_string_pretty(&file)
                    .map_err(|e| fail(format!("Could not encode profiles: {e}")))?,
                "application/json",
            )?;
            change(|a| a.say(msg, false));
        }
        "import-profiles" => {
            if !a.live() {
                change(|a| {
                    a.say(
                        "Start the engine (numera-engine serve) to import profiles.",
                        false,
                    )
                });
            } else {
                pick_file("profiles", ".json,application/json")?;
            }
        }
        "bulk" => change(|a| {
            a.modal = "bulk".into();
            a.batch = Batch::default();
            a.batch.engine = a.default_engine().into();
        }),
        "sync" => change(|a| {
            a.modal = "sync".into();
            a.form = json!({});
        }),
        "batch-chip" => {
            if let Some((key, value)) = id.split_once('|') {
                change(|a| {
                    let v = match key {
                        "os" => &mut a.batch.os,
                        "countries" => &mut a.batch.countries,
                        _ => &mut a.batch.versions,
                    };
                    if v.contains(&value.into()) {
                        if v.len() > 1 {
                            v.retain(|v| v != value);
                        }
                    } else {
                        v.push(value.into());
                    }
                });
            }
        }
        "batch-randomize" => change(|a| a.batch.randomize = !a.batch.randomize),
        "create-batch" => {
            if a.busy {
                return Ok(true);
            }
            change(|a| {
                a.busy = true;
                a.batch_progress = 0;
            });
            draw();
            let mut profiles = a
                .batch
                .build(&a.folder, |n| (js_sys::Math::random() * n as f64) as usize);
            let result = async {
                if a.live() {
                    for (chunk_index, chunk) in profiles.chunks(500).enumerate() {
                        request(
                            "POST",
                            "/api/profiles/batch",
                            Some(json!({"profiles":chunk})),
                        )
                        .await?;
                        change(|a| {
                            a.batch_progress = ((chunk_index + 1) * 500).min(profiles.len())
                        });
                        draw();
                    }
                    refresh().await?;
                } else {
                    for p in &mut profiles {
                        p["id"] = json!(local_id());
                    }
                    change(|a| a.profiles.extend(profiles));
                }
                Ok::<_, ApiError>(())
            }
            .await;
            change(|a| {
                a.busy = false;
                if result.is_ok() {
                    a.modal.clear();
                }
            });
            result?;
        }
        "apply-sync" => {
            let patch = sync_patch(&a.form);
            if patch.as_object().is_none_or(|o| o.is_empty()) || a.busy {
                return Ok(true);
            }
            let ids = a
                .selected_profiles()
                .iter()
                .map(|p| s(p, "id").to_owned())
                .collect::<Vec<_>>();
            if a.live() {
                match request(
                    "POST",
                    "/api/profiles/bulk",
                    Some(json!({"ids":ids,"patch":patch})),
                )
                .await
                {
                    Ok(saved) => {
                        let done = list(&saved).len();
                        change(|a| {
                            a.say(
                                if done < ids.len() {
                                    format!(
                                        "Edited {done} of {}. {} no longer existed.",
                                        ids.len(),
                                        ids.len() - done
                                    )
                                } else {
                                    format!("Edited {done} profile(s).")
                                },
                                done < ids.len(),
                            )
                        });
                        let _ = refresh().await;
                        let _ = refresh_config().await;
                    }
                    Err(e) => {
                        let _ = refresh_config().await;
                        change(|a| a.say(format!("Bulk edit failed: {e}"), true));
                    }
                }
            } else {
                change(|a| {
                    for p in &mut a.profiles {
                        if ids.contains(&s(p, "id").to_owned()) {
                            merge_patch(p, &patch);
                        }
                    }
                    if !s(&patch, "engine").is_empty() {
                        a.config["default_engine"] = patch["engine"].clone();
                    }
                });
            }
            change(|a| {
                a.selected.clear();
                a.modal.clear();
            });
        }
        _ => return Ok(false),
    }
    Ok(true)
}
