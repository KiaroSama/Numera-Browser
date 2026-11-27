use super::*;

pub(super) async fn copy(text: &str) -> Result<(), ApiError> {
    let unavailable = |_| fail("Clipboard unavailable. Select the text and copy it manually.");
    let clipboard = Reflect::get(
        window().navigator().as_ref(),
        &JsValue::from_str("clipboard"),
    )
    .map_err(unavailable)?;
    let pending = call(&clipboard, "writeText", &[JsValue::from_str(text)])
        .and_then(JsValue::dyn_into::<js_sys::Promise>)
        .map_err(unavailable)?;
    JsFuture::from(pending).await.map_err(unavailable)?;
    change(|a| a.say("Copied.", false));
    Ok(())
}
pub(super) fn filename(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || "._-".contains(c) {
                c
            } else {
                '-'
            }
        })
        .collect()
}
pub(super) fn download(name: &str, text: &str, mime: &str) -> Result<(), ApiError> {
    let bag = web_sys::BlobPropertyBag::new();
    bag.set_type(mime);
    let parts = Array::new();
    parts.push(&JsValue::from_str(text));
    let blob = Blob::new_with_str_sequence_and_options(&parts, &bag).map_err(network)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(network)?;
    let anchor = document()
        .create_element("a")
        .map_err(network)?
        .dyn_into::<HtmlAnchorElement>()
        .map_err(|e| network(e.into()))?;
    anchor.set_href(&url);
    anchor.set_download(name);
    anchor.click();
    web_sys::Url::revoke_object_url(&url).map_err(network)
}
pub(super) fn pick_file(kind: &str, accept: &str) -> Result<(), ApiError> {
    let input = document()
        .create_element("input")
        .map_err(network)?
        .dyn_into::<HtmlInputElement>()
        .map_err(|e| network(e.into()))?;
    input.set_type("file");
    input.set_accept(accept);
    input.set_id("numera-file-picker");
    input.set_hidden(true);
    document()
        .body()
        .ok_or_else(|| fail("No document body"))?
        .append_child(&input)
        .map_err(network)?;
    let kind = kind.to_owned();
    let selected = input.clone();
    listen(input.clone().into(), "change", move |_: Event| {
        selected.remove();
        if let Some(file) = selected.files().and_then(|files| files.get(0)) {
            let kind = kind.clone();
            spawn_local(async move {
                if kind == "cookies" {
                    change(|a| a.busy = true);
                    draw();
                }
                let result = async {
                    let text = JsFuture::from(file.text())
                        .await
                        .map_err(network)?
                        .as_string()
                        .unwrap_or_default();
                    match kind.as_str() {
                        "profiles" => {
                            let r =
                                request("POST", "/api/profiles/import", Some(json!({"text":text})))
                                    .await?;
                            let _ = refresh().await;
                            change(|a| a.say(import_report(&r), false));
                        }
                        "proxies" => change(|a| a.form["text"] = json!(text)),
                        "cookies" => {
                            let a = snapshot();
                            let id = a.editor.as_ref().map(|e| s(&e.draft, "id")).unwrap_or("");
                            let r = request(
                                "POST",
                                &format!("/api/profiles/{id}/cookies"),
                                Some(json!({"text":text,"clear":a.form["replaceCookies"]==true})),
                            )
                            .await?;
                            change(|a| {
                                a.say(
                                    format!(
                                        "Imported {} cookies from {}.",
                                        number(&r, "imported"),
                                        file.name()
                                    ),
                                    false,
                                )
                            });
                        }
                        _ => {}
                    }
                    Ok::<_, ApiError>(())
                }
                .await;
                if let Err(e) = result {
                    change(|a| a.say(format!("Import failed: {e}"), true));
                }
                if kind == "cookies" {
                    change(|a| a.busy = false);
                }
                draw();
            });
        }
    });
    let cancelled = input.clone();
    listen(input.clone().into(), "cancel", move |_| cancelled.remove());
    input.click();
    Ok(())
}
pub fn timezones(zone: &str) -> (Vec<String>, bool) {
    let intl =
        Reflect::get(&js_sys::global(), &JsValue::from_str("Intl")).unwrap_or(JsValue::UNDEFINED);
    let values = call(&intl, "supportedValuesOf", &[JsValue::from_str("timeZone")])
        .unwrap_or(JsValue::UNDEFINED);
    let mut zones = if Array::is_array(&values) {
        Array::from(&values)
            .iter()
            .filter_map(|v| v.as_string())
            .collect::<Vec<_>>()
    } else {
        vec![]
    };
    zones.extend(LOCALES.map(|(_, z)| z.into()));
    zones.sort();
    zones.dedup();
    let options = js_sys::Object::new();
    let _ = Reflect::set(
        &options,
        &JsValue::from_str("timeZone"),
        &JsValue::from_str(zone),
    );
    let valid = Reflect::get(&intl, &JsValue::from_str("DateTimeFormat"))
        .ok()
        .and_then(|f| f.dyn_into::<Function>().ok())
        .is_some_and(|f| {
            let args = Array::new();
            args.push(&JsValue::from_str("en"));
            args.push(&options);
            Reflect::construct(&f, &args).is_ok()
        });
    (zones, valid)
}
