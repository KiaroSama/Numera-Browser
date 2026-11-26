use super::*;

pub(super) async fn dispatch(act: &str, id: &str, a: &App) -> Result<bool, ApiError> {
    match act {
        "import-cookies" => pick_file("cookies", ".json,.txt")?,
        "headless" => change(|a| a.form["headless"] = json!(a.form["headless"] != true)),
        "attach" => {
            let result = request(
                "POST",
                &format!("/api/profiles/{id}/attach"),
                Some(json!({"headless":a.form["headless"]==true})),
            )
            .await?;
            change(|a| {
                a.connections.insert(id.into(), result.clone());
                a.say(format!("Attached on :{}", number(&result, "port")), false);
            });
            refresh().await?;
        }
        "detach" => {
            request("POST", &format!("/api/profiles/{id}/stop"), None).await?;
            change(|a| {
                a.connections.remove(id);
            });
            refresh().await?;
        }
        "copy-snippet" => {
            if let Some(c) = a.connections.get(id) {
                copy(&render::cdp_snippet(s(&a.form, "lang"), number(c, "port"))).await?;
            }
        }
        "proxy-tab" => change(|a| a.pool_tab = id.into()),
        "select-proxy" => change(|a| App::toggle_set(&mut a.pool_selected, id.into())),
        "select-all-proxies" => change(|a| {
            let ids = a
                .pool
                .iter()
                .filter(|p| match a.pool_tab.as_str() {
                    "archive" => p["archived"] == true,
                    "errors" => p["archived"] != true && s(p, "status") == "failed",
                    _ => p["archived"] != true,
                })
                .map(|p| s(p, "id").to_owned())
                .collect::<Vec<_>>();
            if ids.iter().all(|id| a.pool_selected.contains(id)) {
                a.pool_selected.clear();
            } else {
                a.pool_selected = ids.into_iter().collect();
            }
        }),
        "clear-proxies" => change(|a| a.pool_selected.clear()),
        "reveal-proxy" => change(|a| App::toggle_set(&mut a.reveal, id.into())),
        "copy-proxy" => {
            if let Some(p) = a.pool.iter().find(|p| s(p, "id") == id) {
                copy(s(p, "pass")).await?;
            }
        }
        "edit-proxy" => {
            if let Some(p) = a.pool.iter().find(|p| s(p, "id") == id) {
                change(|a| {
                    a.modal = "edit-proxy".into();
                    a.form = p.clone();
                });
            }
        }
        "save-proxy" => {
            if s(&a.form, "host").trim().is_empty() {
                return Err(fail("Host cannot be empty."));
            }
            if text(&a.form["port"]).trim().is_empty() {
                return Err(fail("Port cannot be empty."));
            }
            let patch = pool_password_patch(&a.form);
            request(
                "PUT",
                &format!("/api/proxies/{}", s(&a.form, "id")),
                Some(patch),
            )
            .await?;
            change(|a| {
                a.modal.clear();
                a.say("Saved.", false);
            });
            load_view().await?;
        }
        "import-proxies" => change(|a| {
            a.modal = "import-proxies".into();
            a.form = json!({"text":""});
        }),
        "load-proxy-file" => pick_file("proxies", ".txt,.csv,.json")?,
        "add-proxies" => {
            let result = request(
                "POST",
                "/api/proxies",
                Some(json!({"text":s(&a.form,"text")})),
            )
            .await?;
            change(|a| {
                a.modal.clear();
                a.say(
                    format!(
                        "Added {} proxy(ies). Pool now has {}.",
                        number(&result, "added"),
                        number(&result, "total")
                    ),
                    false,
                );
            });
            load_view().await?;
        }
        "test-proxy" | "test-proxies" | "test-all-proxies" => {
            let ids = chosen_proxies(
                if act == "test-proxy" { id } else { "" },
                act == "test-all-proxies",
            )
            .iter()
            .map(|p| s(p, "id").to_owned())
            .collect::<Vec<_>>();
            let r = if act == "test-proxy" {
                request("POST", &format!("/api/proxies/{id}/test"), None).await?
            } else {
                request("POST", "/api/proxies/test", Some(json!({"ids":ids}))).await?
            };
            change(|a| {
                a.say(
                    if act == "test-proxy" {
                        "Proxy checked.".into()
                    } else {
                        format!(
                            "Tested {}: {} live, {} not.",
                            number(&r, "tested"),
                            number(&r, "ok"),
                            number(&r, "tested").saturating_sub(number(&r, "ok"))
                        )
                    },
                    false,
                )
            });
            load_view().await?;
        }
        "rotate-proxy" | "rotate-proxies" => {
            let rows = chosen_proxies(if act == "rotate-proxy" { id } else { "" }, false);
            let ids = rows
                .iter()
                .filter(|p| !s(p, "rotateUrl").is_empty())
                .map(|p| s(p, "id").to_owned())
                .collect::<Vec<_>>();
            if ids.is_empty() {
                return Err(fail("None of these have a change-IP URL set."));
            }
            let r = if act == "rotate-proxy" {
                request("POST", &format!("/api/proxies/{id}/rotate"), None).await?
            } else {
                request("POST", "/api/proxies/rotate", Some(json!({"ids":ids}))).await?
            };
            change(|a| {
                a.say(
                    if act == "rotate-proxy" {
                        if r["rotated"] == true {
                            format!("New exit IP: {} → {}", text(&r["previousIp"]), s(&r, "ip"))
                        } else if !s(&r, "error").is_empty() {
                            s(&r, "error").into()
                        } else {
                            "The exit IP did not change.".into()
                        }
                    } else {
                        format!(
                            "{} of {} moved to a new IP.",
                            number(&r, "rotated"),
                            number(&r, "tried")
                        )
                    },
                    if act == "rotate-proxy" {
                        r["rotated"] != true
                    } else {
                        number(&r, "rotated") == 0
                    },
                )
            });
            load_view().await?;
        }
        "archive-proxy" | "archive-proxies" => {
            let rows = chosen_proxies(if act == "archive-proxy" { id } else { "" }, false);
            let archived = if act == "archive-proxy" {
                rows.first().is_none_or(|p| p["archived"] != true)
            } else {
                a.pool_tab != "archive"
            };
            request("POST","/api/proxies/archive",Some(json!({"ids":rows.iter().map(|p|s(p,"id")).collect::<Vec<_>>(),"archived":archived}))).await?;
            change(|a| {
                a.pool_selected.clear();
                a.say(
                    format!(
                        "{} {} proxies.",
                        if archived { "Archived" } else { "Restored" },
                        rows.len()
                    ),
                    false,
                );
            });
            load_view().await?;
        }
        "export-proxies" | "export-all-proxies" => {
            let ids = chosen_proxies("", act == "export-all-proxies")
                .iter()
                .map(|p| s(p, "id").to_owned())
                .collect::<Vec<_>>();
            let r = request("POST", "/api/proxies/export", Some(json!({"ids":ids}))).await?;
            download(
                &format!("numera-proxies-{}.txt", number(&r, "count")),
                s(&r, "text"),
                "text/plain",
            )?;
            change(|a| a.say(format!("Exported {} proxies.", number(&r, "count")), false));
        }
        "delete-proxy" | "delete-proxies" => {
            let rows = chosen_proxies(if act == "delete-proxy" { id } else { "" }, false);
            let used = rows.iter().map(|p| number(p, "profiles")).sum::<u64>();
            confirm(
                "delete-proxies",
                rows.iter().map(|p| s(p, "id").into()).collect(),
                &format!("Delete {} proxies?", rows.len()),
                &format!(
                    "Removing {} proxies from the pool. {used} profiles currently use them; those profiles keep their proxy settings. This cannot be undone, but you can re-import from an export.",
                    rows.len()
                ),
                "Delete",
            );
        }
        "assign-proxies" => confirm(
            "assign-proxies",
            vec![],
            "Assign proxies to profiles?",
            &format!(
                "Spreading {} proxies round-robin across all {} profiles. Each profile's locale and timezone will be aligned to its proxy's country.",
                a.pool.len(),
                a.profiles.len()
            ),
            "Assign",
        ),
        _ => return Ok(false),
    }
    Ok(true)
}
