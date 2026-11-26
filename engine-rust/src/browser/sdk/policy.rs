//! One CDP setup per page, shared by event callbacks and navigation admission.
use playwright_rs::protocol::CDPSession;
use playwright_rs::server::channel_owner::ChannelOwner;
use playwright_rs::{BrowserContext, Page};
use serde_json::{Value, json};
use std::{collections::HashMap, io, sync::Arc};
use tokio::sync::{Mutex, OnceCell};

type Session = Arc<OnceCell<Result<CDPSession, ()>>>;
pub struct Policy {
    commands: Vec<Value>,
    prefixes: Vec<String>,
    sessions: Mutex<HashMap<String, Session>>,
}
impl Policy {
    pub fn parse(value: &Value) -> io::Result<Arc<Self>> {
        let invalid = || io::Error::new(io::ErrorKind::InvalidInput, "Invalid CDP policy");
        let commands = value["commands"]
            .as_array()
            .filter(|rows| (1..=8).contains(&rows.len()))
            .ok_or_else(invalid)?
            .clone();
        if commands.iter().any(|row| {
            !matches!(
                row["method"].as_str(),
                Some("Emulation.setUserAgentOverride" | "Fetch.enable")
            ) || !row["payload"].is_object()
        }) {
            return Err(invalid());
        }
        let prefixes = value["strip_headers_prefixes"]
            .as_array()
            .ok_or_else(invalid)?
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .filter(|prefix| !prefix.is_empty())
                    .map(str::to_owned)
                    .ok_or_else(invalid)
            })
            .collect::<io::Result<Vec<_>>>()?;
        Ok(Arc::new(Self {
            commands,
            prefixes,
            sessions: Mutex::new(HashMap::new()),
        }))
    }
    pub async fn apply(self: &Arc<Self>, context: &BrowserContext) -> playwright_rs::Result<()> {
        let policy = Arc::clone(self);
        let owner = context.clone();
        context
            .on_page(move |page| {
                let policy = Arc::clone(&policy);
                let owner = owner.clone();
                async move {
                    // Record failures in the OnceCell: admission of this page
                    // fails even when an event dispatcher discards handler errors.
                    let _ = policy.ready(&owner, &page).await;
                    Ok(())
                }
            })
            .await?;
        for page in context.pages() {
            self.ready(context, &page).await?;
        }
        Ok(())
    }
    pub async fn ready(&self, context: &BrowserContext, page: &Page) -> playwright_rs::Result<()> {
        let session = self
            .sessions
            .lock()
            .await
            .entry(page.guid().to_owned())
            .or_default()
            .clone();
        let result = session
            .get_or_init(|| async { self.configure(context, page).await.map_err(|_| ()) })
            .await;
        match result {
            Ok(_) => Ok(()),
            Err(()) => Err(io::Error::other("New page CDP configuration failed").into()),
        }
    }
    async fn configure(
        &self,
        context: &BrowserContext,
        page: &Page,
    ) -> playwright_rs::Result<CDPSession> {
        let session = context.new_cdp_session(page).await?;
        if !self.prefixes.is_empty() {
            let owner = session.clone();
            let prefixes = self.prefixes.clone();
            session.on("Fetch.requestPaused", move |event| {
                let session = owner.clone();
                let prefixes = prefixes.clone();
                async move {
                    let Some(id) = event["requestId"].as_str().filter(|id| !id.is_empty()) else {
                        return Ok(());
                    };
                    let headers = event["request"]["headers"]
                        .as_object()
                        .map(|headers| {
                            headers
                                .iter()
                                .filter(|(name, _)| {
                                    !prefixes
                                        .iter()
                                        .any(|prefix| name.to_lowercase().starts_with(prefix))
                                })
                                .map(|(name, value)| json!({"name":name,"value":value}))
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    if session
                        .send(
                            "Fetch.continueRequest",
                            Some(json!({"requestId":id,"headers":headers})),
                        )
                        .await
                        .is_err()
                    {
                        let _ = session
                            .send("Fetch.continueRequest", Some(json!({"requestId":id})))
                            .await;
                    }
                    Ok(())
                }
            });
        }
        for command in &self.commands {
            // Method names were validated by parse; no arbitrary CDP surface.
            let method = command["method"]
                .as_str()
                .ok_or_else(|| io::Error::other("CDP method missing"))?;
            session
                .send(method, Some(command["payload"].clone()))
                .await?;
        }
        Ok(session)
    }
}
