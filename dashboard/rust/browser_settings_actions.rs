use super::*;

pub(super) async fn dispatch(act: &str, id: &str, a: &App) -> Result<bool, ApiError> {
    match act {
        "create-token" => {
            let name = s(&a.form, "tokenName").trim();
            if name.is_empty() {
                return Ok(true);
            }
            let made = request("POST", "/api/tokens", Some(json!({"name":name}))).await?;
            set_auth(&a.adopt_token(&auth(), &made));
            change(|a| {
                a.minted = made;
                a.form["tokenName"] = json!("");
            });
            load_view().await?;
        }
        "copy-token" => copy(s(&a.minted, "token")).await?,
        "dismiss-token" => change(|a| a.minted = Value::Null),
        "revoke-token" => {
            let name = list(&a.tokens["tokens"])
                .iter()
                .find(|t| s(t, "id") == id)
                .map(|t| text(&t["name"]))
                .unwrap_or_default();
            confirm(
                "revoke-token",
                vec![id.into()],
                "Revoke this token?",
                &format!(
                    "Anything using {name} stops working immediately. Other tokens are unaffected. There is no undo — mint a new one if you need it back."
                ),
                "Revoke",
            );
        }
        "unlock" => {
            set_auth(s(&a.form, "token"));
            change(|a| a.busy = true);
            let state = health().await;
            change(|a| {
                a.busy = false;
                a.auth_error = if state == "locked" {
                    "That token was not accepted.".into()
                } else if state == "off" {
                    "The engine stopped answering. Is it still running?".into()
                } else {
                    String::new()
                };
            });
            if state == "ok" {
                change(|a| a.mode = "connecting".into());
                connect().await;
            }
        }
        "cloak-review" => {
            if a.busy || !a.live() {
                return Ok(true);
            }
            change(|a| {
                a.busy = true;
                a.cloak_approved = false;
            });
            draw();
            let r = request("GET", "/api/engines/cloak/acquisition", None).await;
            change(|a| {
                a.busy = false;
                a.cloak_approved = false;
            });
            change(|a| a.cloak_reviewed(r.as_ref().cloned().unwrap_or(Value::Null)));
            r?;
        }
        "cloak-consent" => change(|a| {
            if !a.busy && a.cloak["available"] == true {
                a.cloak_approved = !a.cloak_approved;
            }
        }),
        "cloak-acquire" => {
            let mut body = None;
            change(|a| body = a.begin_cloak());
            let Some(body) = body else { return Ok(true) };
            draw();
            let result = request("POST", "/api/engines/cloak/acquisition", Some(body)).await;
            change(|a| a.finish_cloak(result.as_ref().cloned().map_err(|e| e.message.clone())));
            result?;
        }
        "update-check" => run_update("check", "").await?,
        "update-cancel" => run_update("cancel", s(&a.update["offer"], "id")).await?,
        "update-review" => {
            if a.update["offer"].is_object() {
                change(
                    |a| a.update_approval = json!({"offer":a.update["offer"],"install":(["ready","deferred"].contains(&s(&a.update,"state")))}),
                );
            }
        }
        "update-later" => change(|a| a.update_approval = Value::Null),
        "update-approve" => {
            if updates::approval_valid(
                &a.update,
                &a.update_approval,
                a.update_busy,
                &a.update_error,
            ) {
                let action = if a.update_approval["install"] == true {
                    "install"
                } else {
                    "download"
                };
                let id = s(&a.update_approval["offer"], "id").to_owned();
                change(|a| a.update_approval = Value::Null);
                run_update(action, &id).await?;
            }
        }
        "dismiss-update" => change(|a| a.dismissed = s(&a.update["offer"], "id").into()),
        "commands" => change(|a| {
            a.modal = "commands".into();
            a.command.clear();
            a.command_index = 0;
        }),
        "command" => {
            if let Some((_, action, id)) =
                a.commands().get(id.parse::<usize>().unwrap_or(0)).cloned()
            {
                change(|a| a.modal.clear());
                Box::pin(handle(&action, &id)).await?;
            }
        }
        "close-modal" => {
            if !a.busy {
                change(|a| a.modal.clear());
            }
        }
        "clear-message" => change(|a| {
            a.toasts.retain(|t| t.id.to_string() != id);
            a.message.clear();
        }),
        "confirm" => execute_confirmation().await?,
        _ => return Ok(false),
    }
    Ok(true)
}
