#[cfg(feature = "update-acceptance")]
pub mod acceptance;
pub mod archive;
pub mod bridge;
mod custody;
pub mod download;
pub mod files;
pub mod install_lease;
pub mod log;
pub mod manifest;
pub mod recovery;
#[cfg(test)]
mod tests;
pub mod transaction;
pub mod worker;
use serde::Serialize;
use serde_json::{json, Value};
use std::{
    io,
    path::PathBuf,
    process::Child,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::{self, JoinHandle},
    time::Duration,
};

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Current {
    pub version: String,
    pub source_sha: String,
    pub generation: u64,
    pub platform: String,
    pub portable: bool,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OfferView {
    pub id: String,
    pub version: String,
    pub source_sha: String,
    pub generation: u64,
    pub download_bytes: u64,
    pub expanded_bytes: u64,
    pub components: Value,
    pub restart_required: bool,
}
#[derive(Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Progress {
    pub received_bytes: u64,
    pub total_bytes: u64,
}
#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Status {
    pub state: String,
    pub message: String,
    pub current: Current,
    pub offer: Option<OfferView>,
    pub progress: Progress,
    pub can_cancel: bool,
}
fn private_receipt(response: reqwest::blocking::Response) -> io::Result<Value> {
    use std::io::Read;
    if response.content_length().is_some_and(|size| size > 4096) {
        return Err(manifest::error("Private update receipt exceeds its limit"));
    }
    let mut bytes = Vec::new();
    response.take(4097).read_to_end(&mut bytes)?;
    if bytes.len() > 4096 {
        return Err(manifest::error("Private update receipt exceeds its limit"));
    }
    serde_json::from_slice(&bytes).map_err(|_| manifest::error("Private update receipt malformed"))
}
struct State {
    status: Status,
    offer: Option<download::Offer>,
    journal: Option<transaction::Journal>,
}
pub struct Service {
    root: PathBuf,
    key: [u8; 32],
    platform: String,
    engine_port: u16,
    engine_token: String,
    state: Mutex<State>,
    cancelled: AtomicBool,
    operation: Mutex<Option<JoinHandle<()>>>,
    worker: Mutex<Option<Child>>,
    exit: Arc<dyn Fn() + Send + Sync>,
    log: log::Log,
}
impl Service {
    pub fn new(
        root: PathBuf,
        engine_port: u16,
        engine_token: String,
        exit: Arc<dyn Fn() + Send + Sync>,
    ) -> io::Result<Arc<Self>> {
        let key = manifest::decode_hex::<32>(manifest::PUBLIC_KEY)?;
        let platform = manifest::platform()?.to_owned();
        let build = transaction::installed(&root, &key, &platform)
            .ok()
            .filter(|build| {
                std::env::var_os("NUMERA_ENGINE").is_none()
                    && build.source_sha == env!("NUMERA_BUILD_SOURCE_SHA")
                    && env!("NUMERA_BUILD_GENERATION").parse::<u64>().ok() == Some(build.generation)
                    && build.version == env!("CARGO_PKG_VERSION")
            });
        let current = Current {
            version: build
                .as_ref()
                .map_or(env!("CARGO_PKG_VERSION").into(), |build| {
                    build.version.clone()
                }),
            source_sha: build
                .as_ref()
                .map_or(String::new(), |build| build.source_sha.clone()),
            generation: build.as_ref().map_or(0, |build| build.generation),
            platform: platform.clone(),
            portable: build.is_some(),
        };
        let status=Status{state:if current.portable{"idle"}else{"unavailable"}.into(),message:if current.portable{"Check for a signed compatible application and browser generation."}else{"Source and unsigned installations cannot self-update; use the documented release route."}.into(),current,offer:None,progress:Progress::default(),can_cancel:false};
        let log = log::Log::new(&root, "desktop-updates");
        log.event("INFO", "Update service initialized");
        Ok(Arc::new(Self {
            root,
            key,
            platform,
            engine_port,
            engine_token,
            state: Mutex::new(State {
                status,
                offer: None,
                journal: None,
            }),
            cancelled: AtomicBool::new(false),
            operation: Mutex::new(None),
            worker: Mutex::new(None),
            exit,
            log,
        }))
    }
    pub fn status(&self) -> Status {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .status
            .clone()
    }
    fn set(&self, state: &str, message: &str) {
        self.log.event(
            if state == "error" { "ERROR" } else { "INFO" },
            &format!("State changed state={state}"),
        );
        let mut inner = self
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        inner.status.state = state.into();
        inner.status.message = message.into();
        inner.status.can_cancel =
            matches!(state, "downloading" | "checking" | "ready" | "deferred");
    }
    fn start(
        self: &Arc<Self>,
        action: impl FnOnce(Arc<Self>) -> io::Result<()> + Send + 'static,
    ) -> io::Result<()> {
        let mut owner = self
            .operation
            .lock()
            .map_err(|_| io::Error::other("Update operation owner poisoned"))?;
        if owner.as_ref().is_some_and(|worker| !worker.is_finished()) {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "An update operation is already active",
            ));
        }
        if let Some(worker) = owner.take() {
            worker
                .join()
                .map_err(|_| io::Error::other("Update operation failed"))?;
        }
        self.cancelled.store(false, Ordering::Release);
        let service = self.clone();
        *owner = Some(thread::spawn(move || {
            if let Err(error) = action(service.clone()) {
                service.log.event(
                    "ERROR",
                    &format!("Update operation refused kind={:?}", error.kind()),
                );
                service.set(if error.kind()==io::ErrorKind::ConnectionAborted{"cancelled"}else{"error"},if error.kind()==io::ErrorKind::ConnectionAborted{"Update cancelled; the current installation is unchanged."}else{"Update could not complete safely. The current generation or its recovery journal is retained."});
            }
        }));
        Ok(())
    }
    pub fn check(self: &Arc<Self>) -> io::Result<()> {
        if !self.status().current.portable {
            return Ok(());
        }
        self.start(|service|{
        service.set("checking","Checking authenticated release generations…");
        {let mut state=service.state.lock().map_err(|_|io::Error::other("Update state poisoned"))?;
        state.status.offer=None;state.offer=None;}
        let current=service.status().current;
        let generation=transaction::generation_floor(&service.root,current.generation)?;
        let offer=download::discover(generation,&service.platform,&service.key,transaction::now()?,&service.cancelled)?;
        if service.cancelled.load(Ordering::Acquire){return Err(io::Error::new(io::ErrorKind::ConnectionAborted,"Update check cancelled"));}
        let mut state=service.state.lock().map_err(|_|io::Error::other("Update state poisoned"))?;
        state.status.offer=offer.as_ref().map(|offer|OfferView{id:offer.id.clone(),version:offer.manifest.version.clone(),source_sha:offer.manifest.source_sha.clone(),generation:offer.manifest.generation,download_bytes:offer.asset.size,expanded_bytes:offer.asset.expanded_size,components:offer.manifest.components[&service.platform].clone(),restart_required:true});
        state.status.state=if offer.is_some(){"available"}else{"current"}.into();state.status.message=if offer.is_some(){"A signed application and compatible browser update is available. Download requires your approval."}else{"No newer compatible signed generation was found."}.into();state.status.can_cancel=false;state.offer=offer;Ok(())
    })
    }
    pub fn download(self: &Arc<Self>, offer_id: &str, approve: bool) -> io::Result<()> {
        if !approve {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Explicit download approval is required",
            ));
        }
        let id = offer_id.to_owned();
        self.start(move |service| {
            let offer = {
                let state = service
                    .state
                    .lock()
                    .map_err(|_| io::Error::other("Update state poisoned"))?;
                state
                    .offer
                    .as_ref()
                    .filter(|offer| offer.id == id)
                    .cloned()
                    .ok_or_else(|| manifest::error("Update offer is no longer current"))?
            };
            if !service.status().current.portable {
                return Err(io::Error::new(
                    io::ErrorKind::Unsupported,
                    "This installation is not a signed portable generation",
                ));
            }
            let _lock = files::lock(&service.root)?;
            if recovery::pending(&service.root)?.is_some() {
                return Err(manifest::error(
                    "An unfinished update requires recovery before another download",
                ));
            }
            let old = transaction::installed(&service.root, &service.key, &service.platform)?;
            transaction::validate_files(&service.root, &old)?;
            manifest::signed_manifest(
                &offer.bytes,
                &offer.signature,
                &service.key,
                &service.platform,
                transaction::generation_floor(&service.root, old.generation)?,
                transaction::now()?,
            )?;
            let previous = old
                .files
                .iter()
                .map(|file| match file {
                    manifest::InstalledFile::Regular(file) => file.size,
                    _ => 0,
                })
                .sum();
            download::check_space(&service.root, &offer.asset, previous)?;
            let journal = transaction::create(&service.root, &offer.id)?;
            files::write(&journal.directory.join("manifest.json"), &offer.bytes)?;
            files::write(&journal.directory.join("manifest.sig"), &offer.signature)?;
            service.set("downloading", "Downloading the approved signed generation…");
            {
                let mut state = service
                    .state
                    .lock()
                    .map_err(|_| io::Error::other("Update state poisoned"))?;
                state.status.progress = Progress {
                    received_bytes: 0,
                    total_bytes: offer.asset.size,
                };
                state.journal = Some(journal.clone());
            }
            let path = journal.directory.join("archive.part");
            download::stream(
                &download::client()?,
                &download::asset_url(&offer.manifest.source_sha, &offer.asset.name),
                &offer.asset,
                &path,
                &service.cancelled,
                |received| {
                    if let Ok(mut state) = service.state.lock() {
                        state.status.progress.received_bytes = received;
                    }
                },
            )?;
            let build = archive::extract(
                &path,
                &journal.staged(),
                &offer.asset,
                &service.key,
                &service.cancelled,
            )?;
            offer.manifest.validate_build(
                &build,
                &manifest::digest(&files::read(
                    &journal.staged().join("BUILD.json"),
                    manifest::BUILD_LIMIT,
                )?),
            )?;
            service.finish_download()
        })
    }
    fn finish_download(&self) -> io::Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("Update state poisoned"))?;
        if self.cancelled.load(Ordering::Acquire) {
            return Err(io::Error::new(
                io::ErrorKind::ConnectionAborted,
                "Update download cancelled",
            ));
        }
        state.status.state = "ready".into();
        state.status.message = "Download verified. Installation and restart require your approval; active work will defer activation.".into();
        state.status.can_cancel = true;
        Ok(())
    }
    pub fn install(self: &Arc<Self>, offer_id: &str, approve: bool) -> io::Result<()> {
        if !approve {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "Explicit installation approval is required",
            ));
        }
        let id = offer_id.to_owned();
        self.start(move|service|{
            let mut journal={let mut state=service.state.lock().map_err(|_|io::Error::other("Update state poisoned"))?;
                if !matches!(state.status.state.as_str(),"ready"|"deferred")||state.offer.as_ref().is_none_or(|offer|offer.id!=id){return Err(manifest::error("No verified approved update is ready"));}
                let journal=state.journal.clone().ok_or_else(||manifest::error("Update transaction missing"))?;
                if journal.manifest_digest!=id{return Err(manifest::error("Staged generation differs from the approved offer"));}
                state.status.state="activating".into();state.status.can_cancel=false;
                state.status.message="Preparing the approved update; active work will defer installation.".into();
                journal};
            let _lock=files::lock(&service.root)?;
            let installed=transaction::installed(&service.root,&service.key,&service.platform)?;
            let metadata=files::read(&journal.directory.join("manifest.json"),manifest::METADATA_LIMIT)?;
            manifest::signed_manifest(&metadata,&files::read(&journal.directory.join("manifest.sig"),64)?,&service.key,&service.platform,transaction::generation_floor(&service.root,installed.generation)?,transaction::now()?)?;
            let client=reqwest::blocking::Client::builder().no_proxy().timeout(Duration::from_secs(15)).build().map_err(io::Error::other)?;
            let prepared=client.post(format!("http://127.0.0.1:{}/api/updates/prepare",service.engine_port)).bearer_auth(&service.engine_token).json(&json!({})).send().map_err(|_|io::Error::other("Could not prepare update recovery"))?;
            if !prepared.status().is_success(){return Err(io::Error::other("Engine could not prepare update recovery"));}
            let prepared=private_receipt(prepared)?;
            journal.data_root=PathBuf::from(prepared["root"].as_str().ok_or_else(||manifest::error("Update data root missing"))?);
            if !journal.data_root.is_absolute() || prepared["gate"]!=".updates/admission.json" {return Err(manifest::error("Update data root or admission path is invalid"));}
            let root_meta=std::fs::symlink_metadata(&journal.data_root)?;
            if !root_meta.is_dir() || files::linked(&root_meta) {return Err(manifest::error("Prepared update data root is linked or invalid"));}
            journal.ticket=files::nonce()?;journal.phase="quiescing".into();journal.save()?;
            let response=client.post(format!("http://127.0.0.1:{}/api/updates/quiesce",service.engine_port)).bearer_auth(&service.engine_token).json(&json!({"ticket":journal.ticket})).send().map_err(|_|io::Error::other("Could not prove application quiescence; recovery journal retained"))?;
            if response.status().as_u16()==409{
                if recovery::quiesced(&journal)?{return Err(manifest::error("Quiescence is uncertain; matching recovery evidence retained"));}
                journal.phase="staging".into();journal.ticket.clear();journal.save()?;service.set("deferred","Active or uncertain work prevents installation. Stop it normally and try again.");return Ok(());
            }
            if !response.status().is_success(){return Err(io::Error::other("Engine cannot prove update admission is closed"));}
            let receipt=private_receipt(response)?;
            let ticket=receipt["ticket"].as_str().ok_or_else(||manifest::error("Quiescence ticket missing"))?;manifest::decode_hex::<32>(ticket)?;
            if ticket!=journal.ticket||receipt["root"].as_str()!=journal.data_root.to_str(){return Err(manifest::error("Quiescence receipt belongs to another prepared transaction"));}
            journal.phase="ready".into();journal.save()?;
            #[cfg(feature = "update-acceptance")]
            acceptance::checkpoint("quiesced")?;
            match worker::spawn(&journal){Ok(child)=>{*service.worker.lock().map_err(|_|io::Error::other("Update worker owner poisoned"))?=Some(child);service.set("activating","Restarting into the approved verified generation…");(service.exit)();Ok(())},Err(error)=>{
                let resumed=client.post(format!("http://127.0.0.1:{}/api/updates/resume",service.engine_port)).bearer_auth(&service.engine_token).json(&json!({"ticket":journal.ticket})).send().map_err(|_|io::Error::other("Failed update worker retains its admission gate"))?;
                if !resumed.status().is_success(){return Err(io::Error::other("Failed update worker retains its admission gate"));}
                journal.phase="cancelled".into();journal.ticket.clear();journal.save()?;
                Err(error)
            }}
        })
    }
    pub fn cancel(&self, id: &str) -> io::Result<()> {
        let mut state = self
            .state
            .lock()
            .map_err(|_| io::Error::other("Update state poisoned"))?;
        if !state.status.can_cancel || state.offer.as_ref().is_some_and(|offer| offer.id != id) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                "This update operation cannot be cancelled",
            ));
        }
        self.cancelled.store(true, Ordering::Release);
        if matches!(state.status.state.as_str(), "ready" | "deferred") {
            if let Some(mut journal) = state.journal.take() {
                journal.phase = "cancelled".into();
                journal.save()?;
            }
            state.status.state = "cancelled".into();
            state.status.message =
                "Update cancelled. The current generation was not changed.".into();
            state.status.can_cancel = false;
        }
        Ok(())
    }
    pub fn shutdown(&self) {
        self.cancelled.store(true, Ordering::Release);
        if let Ok(mut operation) = self.operation.lock() {
            if let Some(worker) = operation.take() {
                let _ = worker.join();
            }
        }
        if let Ok(mut owner) = self.worker.lock() {
            if let Some(mut child) = owner.take() {
                child.stdin.take();
            }
        }
    }
}
