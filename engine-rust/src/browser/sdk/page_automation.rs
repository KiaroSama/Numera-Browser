use super::{evaluate_raw, invalid, text, values};
use base64::{Engine, engine::general_purpose::STANDARD};
use playwright_rs::{
    BrowserContext, FulfillOptions, GotoOptions, Page, WaitUntil, protocol::route::FetchOptions,
    server::channel_owner::ChannelOwner,
};
use serde_json::{Value, json};
use std::{
    sync::{Arc, Mutex},
    time::Duration,
};

const MAX_TEXT: usize = 32 * 1024;
const MAX_PNG: usize = 384 * 1024;
const MAX_RESOURCE: usize = 8 * 1024 * 1024;

#[derive(Default)]
pub(super) struct QaPage {
    page: Option<Page>,
    origin: Option<reqwest::Url>,
    network: Arc<Mutex<std::collections::VecDeque<Value>>>,
}

impl QaPage {
    pub(super) async fn execute(
        &mut self,
        context: &BrowserContext,
        original: &Page,
        request: &Value,
    ) -> playwright_rs::Result<Value> {
        let timeout = request["timeout_ms"]
            .as_u64()
            .filter(|ms| (1..=10_000).contains(ms))
            .ok_or_else(invalid)?;
        let action = text(request, "action")?;
        let result = tokio::time::timeout(
            Duration::from_millis(timeout + 100),
            self.perform(context, original, request, action, timeout),
        )
        .await;
        match result {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(_)) => Ok(
                json!({"error":"Page action failed; check the local fixture URL, ownership and selector"}),
            ),
            Err(_) => Ok(
                json!({"error":"Page action timed out; check the local fixture or increase timeout_ms","timed_out":true}),
            ),
        }
    }

    async fn perform(
        &mut self,
        context: &BrowserContext,
        original: &Page,
        request: &Value,
        action: &str,
        timeout: u64,
    ) -> playwright_rs::Result<Value> {
        if request.get("url").is_some() {
            let url = crate::network::devtools::qa_url(text(request, "url")?)?;
            if self.page.as_ref().is_none_or(Page::is_closed)
                || self
                    .origin
                    .as_ref()
                    .is_none_or(|previous| previous.origin() != url.origin())
            {
                if let Some(previous) = self.page.as_ref().filter(|page| !page.is_closed()) {
                    previous.close().await?;
                }
                let page = context.new_page().await?;
                let log = Arc::new(Mutex::new(std::collections::VecDeque::new()));
                self.network = log.clone();
                let origin = url.origin();
                if let Err(error) = page
                    .route("**/*", move |route| {
                        let log = log.clone();
                        let origin = origin.clone();
                        async move {
                            let request_url = route.request().url().to_owned();
                            let Ok(mut url) = crate::network::devtools::qa_url(&request_url) else {
                                return route.abort(None).await;
                            };
                            if url.origin() != origin {
                                return route.abort(None).await;
                            }
                            // Check each destination ourselves; the browser can follow a redirect
                            // before invoking another Playwright route handler.
                            let fetched = route
                                .fetch(Some(
                                    FetchOptions::builder()
                                        .max_redirects(0)
                                        .timeout(5_000.0)
                                        .build(),
                                ))
                                .await;
                            let Ok(response) = fetched else {
                                return route.abort(None).await;
                            };
                            if (300..400).contains(&response.status())
                                || response.body().len() > MAX_RESOURCE
                            {
                                return route.abort(None).await;
                            }
                            url.set_query(None);
                            url.set_fragment(None);
                            if let Ok(mut events) = log.lock() {
                                if events.len() >= 64 {
                                    events.pop_front();
                                }
                                events.push_back(
                                    json!({"url":url.as_str(),"status":response.status()}),
                                );
                            }
                            let headers = response
                                .headers()
                                .iter()
                                .filter(|(name, _)| {
                                    !["content-length", "content-encoding", "transfer-encoding"]
                                        .iter()
                                        .any(|key| name.eq_ignore_ascii_case(key))
                                })
                                .cloned()
                                .collect();
                            route
                                .fulfill(
                                    FulfillOptions::builder()
                                        .status(response.status())
                                        .headers(headers)
                                        .body(response.body().to_vec())
                                        .build(),
                                )
                                .await
                        }
                    })
                    .await
                {
                    let _ = page.close().await;
                    return Err(error);
                }
                if let Err(error) = page
                    .route_web_socket("**/*", |socket| async move { socket.close(None).await })
                    .await
                {
                    let _ = page.close().await;
                    return Err(error);
                }
                self.page = Some(page);
                self.origin = Some(url.clone());
            }
            let page = self.page.as_ref().ok_or_else(invalid)?;
            page.goto(
                url.as_str(),
                Some(
                    GotoOptions::new()
                        .timeout(Duration::from_millis(timeout))
                        .wait_until(WaitUntil::DomContentLoaded),
                ),
            )
            .await?;
            if action == "navigate" {
                return Ok(json!({"navigated":true,"url":page.url()}));
            }
        }
        let page = self
            .page
            .as_ref()
            .filter(|page| !page.is_closed())
            .ok_or_else(invalid)?;
        if page.guid() == original.guid() {
            return Err(invalid());
        }
        crate::network::devtools::qa_url(&page.url())?;
        match action {
            "click" => {
                page.locator(text(request, "selector")?).click(None).await?;
                Ok(json!({"clicked":true}))
            }
            "fill" => {
                page.locator(text(request, "selector")?)
                    .fill(text(request, "value")?, None)
                    .await?;
                Ok(json!({"filled":true}))
            }
            "snapshot" => {
                let snapshot = page.locator("body").aria_snapshot(None).await?;
                Ok(json!({"snapshot":bounded_text(&snapshot),"truncated":snapshot.len()>MAX_TEXT}))
            }
            "console" => {
                let messages = page.console_messages();
                Ok(
                    json!({"messages":messages.iter().rev().take(32).rev().map(|message| json!({"level":message.type_(),"text":message.text().chars().take(512).collect::<String>()})).collect::<Vec<_>>()}),
                )
            }
            "network" => {
                let requests = self.network.lock().map_err(|_| invalid())?;
                Ok(json!({"requests":*requests}))
            }
            "eval" => {
                let expression = text(request, "expression")?;
                let value = evaluate_raw(page, expression, &Value::Null, false).await?;
                let result = values::decode(&value)?;
                if serde_json::to_vec(&result)
                    .map_err(std::io::Error::other)?
                    .len()
                    > MAX_TEXT
                {
                    return Err(invalid());
                }
                Ok(json!({"value":result}))
            }
            "screenshot" => {
                let png = match context.new_cdp_session(page).await {
                    Ok(session) => {
                        let result = session
                            .send("Page.captureScreenshot", Some(json!({"format":"png"})))
                            .await;
                        session.detach().await?;
                        let result = result?;
                        result["data"]
                            .as_str()
                            .filter(|data| data.len() <= MAX_PNG)
                            .ok_or_else(invalid)?
                            .to_owned()
                    }
                    Err(_) => {
                        let png = page.screenshot(None).await?;
                        if png.len() > MAX_PNG / 4 * 3 {
                            return Err(invalid());
                        }
                        STANDARD.encode(png)
                    }
                };
                Ok(json!({"screenshot":png,"media_type":"image/png","encoding":"base64"}))
            }
            "wait" => {
                if let Some(selector) = request["selector"].as_str() {
                    page.locator(selector).wait_for(None).await?;
                } else {
                    let ms = request["ms"]
                        .as_u64()
                        .filter(|ms| *ms <= timeout)
                        .ok_or_else(invalid)?;
                    tokio::time::sleep(Duration::from_millis(ms)).await;
                }
                Ok(json!({"matched":true}))
            }
            _ => Err(invalid()),
        }
    }
}

fn bounded_text(text: &str) -> &str {
    &text[..text.floor_char_boundary(text.len().min(MAX_TEXT))]
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn inspection_limits_never_split_utf8() {
        let text = format!("{}é", "x".repeat(MAX_TEXT - 1));
        assert_eq!(bounded_text(&text).len(), MAX_TEXT - 1);
        assert_eq!(bounded_text("فارسی"), "فارسی");
    }
}
