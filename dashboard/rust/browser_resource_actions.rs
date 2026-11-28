use super::*;

pub(super) async fn dispatch(act: &str, id: &str, a: &App) -> Result<bool, ApiError> {
    match act {
        "copy-engine-install" => {
            if let Some(command) = render::engine_install_command(id) {
                copy(command).await?;
            }
        }
        "create-folder" => {
            let name = s(&a.form, "newFolder").trim();
            if !name.is_empty() && !a.folders().contains(&name.into()) {
                if a.live() {
                    match request("POST", "/api/folders", Some(json!({"name":name}))).await {
                        Ok(config) => change(|a| a.config = config),
                        Err(e) => {
                            let _ = refresh_config().await;
                            return Err(e);
                        }
                    }
                } else {
                    change(|a| {
                        let mut folders = a.folders();
                        folders.push(name.into());
                        a.config["folders"] = json!(folders);
                    });
                }
            }
            change(|a| a.form["newFolder"] = json!(""));
        }
        "rename-folder" => change(|a| {
            a.modal = "rename-folder".into();
            a.form = json!({"from":id,"to":id});
        }),
        "commit-folder" => {
            let from = s(&a.form, "from");
            let to = s(&a.form, "to").trim();
            if !to.is_empty() && to != from && !a.folders().contains(&to.into()) {
                if a.live() {
                    match request(
                        "POST",
                        "/api/folders/rename",
                        Some(json!({"from":from,"to":to})),
                    )
                    .await
                    {
                        Ok(config) => {
                            change(|a| {
                                a.config = config;
                                if a.folder == from {
                                    a.folder = to.into();
                                }
                            });
                            refresh().await?;
                        }
                        Err(e) => {
                            let _ = refresh_config().await;
                            let _ = refresh().await;
                            return Err(e);
                        }
                    }
                } else {
                    change(|a| {
                        a.config["folders"] = json!(
                            a.folders()
                                .into_iter()
                                .map(|f| if f == from { to.into() } else { f })
                                .collect::<Vec<_>>()
                        );
                        for p in &mut a.profiles {
                            if s(p, "folder") == from {
                                p["folder"] = json!(to);
                            }
                        }
                        if a.folder == from {
                            a.folder = to.into();
                        }
                    });
                }
            }
            change(|a| a.modal.clear());
        }
        "delete-folder" => confirm(
            "delete-folder",
            vec![id.into()],
            "Delete folder?",
            "Profiles inside it move to Unfiled. Their sessions remain.",
            "Delete folder",
        ),
        "default-engine" => {
            if a.live() {
                match request("PUT", "/api/config", Some(json!({"default_engine":id}))).await {
                    Ok(config) => change(|a| a.config = config),
                    Err(e) => {
                        let _ = refresh_config().await;
                        return Err(e);
                    }
                }
            } else {
                change(|a| {
                    a.config["default_engine"] = json!(id);
                    a.say(
                        "Connect the engine (numera-engine serve) to save this setting.",
                        false,
                    );
                });
            }
        }
        "add-extension" => {
            let entry = s(&a.form, "extension").trim();
            if entry.is_empty() {
                return Ok(true);
            }
            if entry.to_lowercase().starts_with("http://")
                || entry.to_lowercase().starts_with("https://")
            {
                if !a.live() {
                    return Err(fail("Installing from a link needs the engine running."));
                }
                let result = request(
                    "POST",
                    "/api/extensions/install",
                    Some(json!({"url":entry})),
                )
                .await?;
                change(|a| {
                    a.config["default_extensions"] = result["default_extensions"].clone();
                    a.form["extension"] = json!("");
                    a.say(
                        format!(
                            "{}: {}",
                            if result["reused"] == true {
                                "Already installed"
                            } else {
                                "Installed"
                            },
                            s(&result, "name")
                        ),
                        false,
                    );
                });
                load_extensions().await?;
            } else {
                let mut paths = strings(&a.config["default_extensions"]);
                if paths.contains(&entry.into()) {
                    return Err(fail("That folder is already in the list."));
                }
                paths.push(entry.into());
                save_extensions(paths).await?;
                change(|a| a.form["extension"] = json!(""));
            }
        }
        "remove-extension" => {
            let paths = strings(&a.config["default_extensions"])
                .into_iter()
                .filter(|p| p != id)
                .collect();
            save_extensions(paths).await?;
        }
        "apply-extensions" => {
            if !a.live() || a.busy {
                return Ok(true);
            }
            change(|a| a.busy = true);
            let result = async {
                let all = request("GET", "/api/profiles", None).await?;
                let items = strings(&a.config["default_extensions"]);
                let (mut changed, mut skipped, mut failed) = (0, 0, 0);
                for mut p in list(&all) {
                    let mut have = s(&p, "extensions")
                        .lines()
                        .map(str::trim)
                        .filter(|v| !v.is_empty())
                        .map(str::to_owned)
                        .collect::<Vec<_>>();
                    let missing = items
                        .iter()
                        .filter(|x| !have.contains(x))
                        .cloned()
                        .collect::<Vec<_>>();
                    if missing.is_empty() {
                        skipped += 1;
                        continue;
                    }
                    have.extend(missing);
                    p["extensions"] = json!(have.join("\n"));
                    if request(
                        "PUT",
                        &format!("/api/profiles/{}", s(&p, "id")),
                        Some(for_write(&p)),
                    )
                    .await
                    .is_ok()
                    {
                        changed += 1
                    } else {
                        failed += 1
                    }
                }
                let _ = refresh().await;
                change(|a| {
                    a.say(
                        format!("Updated {changed}, already had them {skipped}, failed {failed}."),
                        failed > 0,
                    )
                });
                Ok::<_, ApiError>(())
            }
            .await;
            change(|a| a.busy = false);
            result?;
        }
        "refresh-view" => {
            load_view().await?;
        }
        "probe" | "health-trust" | "health-align" => {
            if a.busy {
                return Ok(true);
            }
            let is_health = act.starts_with("health-");
            let kind = if act == "health-trust" {
                "trust"
            } else if act == "health-align" {
                "align"
            } else {
                id
            };
            let pid = if is_health {
                id.to_owned()
            } else {
                a.editor
                    .as_ref()
                    .map(|e| s(&e.draft, "id").into())
                    .unwrap_or_default()
            };
            change(|a| {
                a.busy = true;
                if !is_health && let Some(ed) = a.editor.as_mut() {
                    ed.probe = json!({"state":"running"});
                }
            });
            draw();
            let result = request("POST", &format!("/api/profiles/{pid}/{kind}"), None).await;
            let editor_session = a.editor_session;
            change(|a| {
                a.busy = false;
                let editor = if !is_health && a.editor_session == editor_session {
                    a.editor.as_mut().filter(|ed| s(&ed.draft, "id") == pid)
                } else {
                    None
                };
                match &result {
                    Ok(v) => {
                        if let Some(ed) = editor {
                            ed.probe = v.clone();
                            if !ed.saving
                                && v["profile"].is_object()
                                && s(&v["profile"], "id") == pid
                            {
                                ed.stale(v["profile"].clone());
                            }
                        }
                        if is_health {
                            let name = a
                                .profiles
                                .iter()
                                .find(|p| s(p, "id") == pid)
                                .map(|p| text(&p["name"]))
                                .unwrap_or(pid.clone());
                            a.say(health_report(&name, kind, v), false);
                        }
                    }
                    Err(e) => {
                        if let Some(ed) = editor {
                            ed.probe = json!({"state":"error","error":e.message});
                        }
                        a.say(
                            format!(
                                "{} failed: {e}",
                                if kind == "trust" {
                                    "Trust check"
                                } else {
                                    "Auto-adjust"
                                }
                            ),
                            true,
                        );
                    }
                }
            });
            if is_health {
                let _ = load_view().await;
                let _ = refresh().await;
            }
        }
        "warm-now" => {
            let pid = a.editor.as_ref().map(|e| s(&e.draft, "id")).unwrap_or("");
            request(
                "POST",
                &format!("/api/profiles/{pid}/warmup"),
                Some(json!({"minutes":5,"preset":preset()})),
            )
            .await?;
            change(|a| {
                a.say(
                    "Warm-up started — runs in the background for ~5 minutes.",
                    false,
                )
            });
        }
        "schedule" => {
            let pid = a.editor.as_ref().map(|e| s(&e.draft, "id")).unwrap_or("");
            if a.schedule.is_object() {
                request(
                    "DELETE",
                    &format!("/api/schedules/{}", s(&a.schedule, "id")),
                    None,
                )
                .await?;
                change(|current| {
                    if current.editor_matches(a.editor_session, pid) {
                        current.schedule = Value::Null;
                    }
                });
            } else {
                let result = request(
                    "POST",
                    "/api/schedules",
                    Some(json!({"profileId":pid,"everyHours":12,"minutes":5,"preset":preset()})),
                )
                .await?;
                change(|current| {
                    if current.editor_matches(a.editor_session, pid) {
                        current.schedule = result;
                    }
                });
            }
        }
        "replace-cookies" => {
            change(|a| a.form["replaceCookies"] = json!(a.form["replaceCookies"] != true))
        }
        _ => return Ok(false),
    }
    Ok(true)
}
