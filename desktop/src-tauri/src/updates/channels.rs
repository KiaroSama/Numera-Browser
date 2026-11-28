use super::{download, manifest, transaction, Service};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    io,
    sync::{atomic::Ordering, Arc},
    time::Duration,
};

#[derive(Clone, Copy, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub(super) enum Channel {
    App,
    Browsers,
    #[default]
    All,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Preferences {
    app_auto_update: bool,
    browser_core_auto_update: bool,
    pub(super) clean_old_versions_on_update: bool,
}

const BROWSERS: [(&str, &str, &str); 6] = [
    ("chromium", "browsers", "playwright"),
    ("chromium-patchright", "browsers", "patchright"),
    ("camoufox", "browsers", "camoufox"),
    ("playwright-sidecar", "sdk", "playwright"),
    ("patchright-sidecar", "sdk", "patchright"),
    ("camoufox-sidecar", "sdk", "camoufox"),
];

fn version<'a>(values: &'a Value, category: &str, id: &str) -> Option<&'a str> {
    values[category][id]["version"].as_str()
}

impl Service {
    pub(super) fn channel_view(&self, channel: Channel) -> io::Result<Value> {
        let state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("Update state poisoned"))?;
        let current = &state.status.current;
        let offer = state.offer.as_ref();
        let installed = if current.portable {
            transaction::installed(&self.root, &self.key, &self.platform)?.components
        } else {
            Value::Null
        };
        let requires_combined = offer
            .is_some_and(|offer| offer.manifest.components.get(&self.platform) != Some(&installed));
        let app = (channel != Channel::Browsers).then(|| {
            json!({
                "current": current.version,
                "latest": offer.map(|offer| offer.manifest.version.as_str()),
                "update_available": offer.is_some(),
                "verified": offer.is_some(),
                "notes": offer.map(|offer| offer.notes.as_str()),
            })
        });
        let browsers = if channel == Channel::App {
            Vec::new()
        } else {
            let latest = offer
                .and_then(|offer| offer.manifest.components.get(&self.platform))
                .unwrap_or(&Value::Null);
            BROWSERS.into_iter().map(|(id, category, key)| {
                let current = version(&installed, category, key);
                let latest = version(latest, category, key);
                json!({"id":id,"current":current,"latest":latest,"update_available":latest.is_some() && latest != current})
            }).collect()
        };
        Ok(
            json!({"channel":channel, "offerId":offer.map(|offer| offer.id.as_str()),
            "requiresCombined":requires_combined, "app":app, "browsers":browsers,
            "state":state.status.state, "message":state.status.message}),
        )
    }

    pub(super) fn preferences(&self) -> io::Result<Preferences> {
        let response = reqwest::blocking::Client::builder()
            .no_proxy()
            .timeout(Duration::from_secs(2))
            .build()
            .map_err(io::Error::other)?
            .get(format!(
                "http://127.0.0.1:{}/api/updates/private-preferences",
                self.engine_port
            ))
            .bearer_auth(&self.engine_token)
            .send()
            .and_then(reqwest::blocking::Response::error_for_status)
            .map_err(|_| io::Error::other("Update preferences unavailable"))?;
        serde_json::from_value(super::private_receipt(response)?).map_err(io::Error::other)
    }

    pub(super) fn auto_check(self: &Arc<Self>) -> io::Result<()> {
        if !self.status().current.portable {
            return Ok(());
        }
        self.start_check(|service| {
            let prefs = service.preferences();
            service.check_now()?;
            let Ok(prefs) = prefs else { return Ok(()) };
            if !prefs.app_auto_update {
                return Ok(());
            }
            let next = service
                .state
                .lock()
                .map_err(|_| io::Error::other("Update state poisoned"))?
                .offer
                .clone();
            if let Some(offer) = next {
                let installed =
                    transaction::installed(&service.root, &service.key, &service.platform)?;
                if prefs.browser_core_auto_update
                    || offer.manifest.components.get(&service.platform)
                        == Some(&installed.components)
                {
                    service.download_now(&offer.id)?;
                    service.install_now(&offer.id, prefs.clean_old_versions_on_update)?;
                }
            }
            Ok(())
        })
    }

    pub(super) fn queue_apply(
        self: &Arc<Self>,
        channel: Channel,
        clean_old: bool,
        offer_id: &str,
    ) -> io::Result<()> {
        manifest::decode_hex::<32>(offer_id)?;
        if channel == Channel::Browsers {
            return Err(io::Error::new(io::ErrorKind::Unsupported, "No signed standalone browser-core update is available. Select all channels to install a compatible full generation."));
        }
        let id = {
            let state = self
                .state
                .lock()
                .map_err(|_| io::Error::other("Update state poisoned"))?;
            let offer = state
                .offer
                .as_ref()
                .ok_or_else(|| manifest::error("Check for a verified application update first"))?;
            if offer.id != offer_id {
                return Err(manifest::error(
                    "The reviewed update offer changed. Check for updates and approve the new offer.",
                ));
            }
            if channel == Channel::App {
                let current = transaction::installed(&self.root, &self.key, &self.platform)?;
                if offer.manifest.components.get(&self.platform) != Some(&current.components) {
                    return Err(io::Error::new(io::ErrorKind::Unsupported, "This signed application update also changes browser resources. Select all channels to approve both."));
                }
            }
            offer.id.clone()
        };
        self.start(move |service| {
            if !matches!(service.status().state.as_str(), "ready" | "deferred") {
                service.download_now(&id)?;
            }
            service.install_now(&id, clean_old)
        })
    }

    pub(super) fn check_now(&self) -> io::Result<()> {
        let current = self.status().current;
        let generation = transaction::generation_floor(&self.root, current.generation)?;
        let offer = download::discover(
            generation,
            &self.platform,
            &self.key,
            transaction::now()?,
            &self.cancelled,
        )?;
        if self.cancelled.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "Update check cancelled",
            ));
        }
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("Update state poisoned"))?;
        state.status.offer = offer.as_ref().map(|offer| super::OfferView {
            id: offer.id.clone(),
            version: offer.manifest.version.clone(),
            source_sha: offer.manifest.source_sha.clone(),
            generation: offer.manifest.generation,
            download_bytes: offer.asset.size,
            expanded_bytes: offer.asset.expanded_size,
            components: offer.manifest.components[&self.platform].clone(),
            restart_required: true,
        });
        state.status.state = if offer.is_some() {
            "available"
        } else {
            "current"
        }
        .into();
        state.status.message = if offer.is_some() {
            "A signed update is available. Download requires your approval."
        } else {
            "No newer compatible signed generation was found."
        }
        .into();
        state.status.can_cancel = false;
        state.offer = offer;
        Ok(())
    }
}
