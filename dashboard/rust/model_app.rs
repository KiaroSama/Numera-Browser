use super::*;

#[derive(Clone, Debug)]
pub struct Toast {
    pub id: u64,
    pub text: String,
    pub error: bool,
    pub expires: f64,
}
#[derive(Clone, Debug)]
pub struct App {
    pub mode: String,
    pub auth_error: String,
    pub ip_checks: BTreeMap<String, Value>,
    pub release_status: Value,
    pub release_preferences: Value,
    pub release_preferences_error: bool,
    pub mcp_bridge_command: Option<String>,
    pub profiles: Vec<Value>,
    pub config: Value,
    pub engines: Value,
    pub view: String,
    pub folder: String,
    pub query: String,
    pub filters: BTreeMap<String, String>,
    pub selected: BTreeSet<String>,
    pub page: usize,
    pub filter_open: bool,
    pub editor: Option<Editor>,
    pub modal: String,
    pub form: Value,
    pub batch: Batch,
    pub batch_progress: usize,
    pub pool: Vec<Value>,
    pub pool_selected: BTreeSet<String>,
    pub pool_tab: String,
    pub health: Value,
    pub tokens: Value,
    pub minted: Value,
    pub extensions: Vec<Value>,
    pub icons: BTreeMap<String, String>,
    pub connections: BTreeMap<String, Value>,
    pub theme: String,
    pub busy: bool,
    pub message: String,
    pub message_error: bool,
    pub toasts: Vec<Toast>,
    pub next_toast: u64,
    pub command: String,
    pub command_index: usize,
    pub cloak: Value,
    pub cloak_approved: bool,
    pub update_bridge: Option<crate::updates::Bridge>,
    pub update: Value,
    pub update_approval: Value,
    pub update_error: String,
    pub update_busy: bool,
    pub dismissed: String,
    pub reveal: BTreeSet<String>,
    pub rename: String,
    pub schedule: Value,
}
impl Default for App {
    fn default() -> Self {
        Self {
            mode: "connecting".into(),
            auth_error: String::new(),
            ip_checks: BTreeMap::new(),
            release_status: Value::Null,
            release_preferences: Value::Null,
            release_preferences_error: false,
            mcp_bridge_command: None,
            profiles: demo_profiles(),
            config: json!({"folders":["Facebook Ads","Amazon Stores","Crypto","QA Testing"]}),
            engines: json!({}),
            view: "profiles".into(),
            folder: "All profiles".into(),
            query: String::new(),
            filters: BTreeMap::new(),
            selected: BTreeSet::new(),
            page: 0,
            filter_open: false,
            editor: None,
            modal: String::new(),
            form: json!({}),
            batch: Batch::default(),
            batch_progress: 0,
            pool: vec![],
            pool_selected: BTreeSet::new(),
            pool_tab: "all".into(),
            health: json!({}),
            tokens: json!({}),
            minted: Value::Null,
            extensions: vec![],
            icons: BTreeMap::new(),
            connections: BTreeMap::new(),
            theme: "dark".into(),
            busy: false,
            message: String::new(),
            message_error: false,
            toasts: vec![],
            next_toast: 0,
            command: String::new(),
            command_index: 0,
            cloak: Value::Null,
            cloak_approved: false,
            update_bridge: None,
            update: Value::Null,
            update_approval: Value::Null,
            update_error: String::new(),
            update_busy: false,
            dismissed: String::new(),
            reveal: BTreeSet::new(),
            rename: String::new(),
            schedule: Value::Null,
        }
    }
}
impl App {
    pub fn commit_profile(&mut self, mut saved: Value) {
        saved["_new"] = json!(false);
        if let Some(index) = self.profiles.iter().position(|p| p["id"] == saved["id"]) {
            self.profiles[index] = saved;
        } else {
            self.profiles.push(saved);
        }
        self.editor = None;
    }
    pub fn bulk_result(
        &mut self,
        verb: &str,
        results: &[(String, Result<(), String>)],
        skipped: usize,
    ) {
        let failed: Vec<_> = results
            .iter()
            .filter(|(_, result)| result.is_err())
            .collect();
        self.selected = failed.iter().map(|(id, _)| id.clone()).collect();
        self.busy = false;
        let note = if skipped > 0 {
            format!(" {skipped} already running.")
        } else {
            String::new()
        };
        self.say(
            if failed.is_empty() {
                format!("{verb} {} profile(s).{note}", results.len())
            } else {
                format!(
                    "{verb} {} of {}. {} failed: {}.{note}",
                    results.len() - failed.len(),
                    results.len(),
                    failed.len(),
                    failed
                        .first()
                        .and_then(|(_, r)| r.as_ref().err())
                        .map_or("No details", String::as_str)
                )
            },
            !failed.is_empty(),
        );
    }
    pub fn cloak_reviewed(&mut self, offer: Value) {
        self.cloak = offer;
        self.cloak_approved = false;
    }
    pub fn begin_cloak(&mut self) -> Option<Value> {
        if self.busy
            || !self.live()
            || !self.cloak_approved
            || self.cloak["available"] != true
            || self.cloak["installed"] == true
            || s(&self.cloak, "offer_id").is_empty()
        {
            return None;
        }
        self.cloak_approved = false;
        self.busy = true;
        Some(json!({"offer_id": self.cloak["offer_id"], "approve": true}))
    }
    pub fn finish_cloak(&mut self, result: Result<Value, String>) {
        self.busy = false;
        match result {
            Ok(result) => {
                self.cloak["installed"] = json!(true);
                self.say(
                    format!("Approved browser acquired: {}.", s(&result, "version")),
                    false,
                );
            }
            Err(error) => {
                self.cloak = Value::Null;
                self.say(error, true);
            }
        }
    }
    pub fn live(&self) -> bool {
        self.mode == "live"
    }
    pub fn default_engine(&self) -> &str {
        s(&self.config, "default_engine")
    }
    pub fn folders(&self) -> Vec<String> {
        let mut out = strings(&self.config["folders"]);
        for p in &self.profiles {
            let f = s(p, "folder");
            if !f.is_empty() && !out.contains(&f.to_owned()) {
                out.push(f.into());
            }
        }
        out
    }
    pub fn selected_profiles(&self) -> Vec<Value> {
        self.selected
            .iter()
            .filter_map(|id| self.profiles.iter().find(|p| s(p, "id") == id))
            .cloned()
            .collect()
    }
    pub fn filtered(&self) -> Vec<&Value> {
        let q = self.query.to_lowercase();
        self.profiles
            .iter()
            .filter(|p| {
                (self.folder == "All profiles" || s(p, "folder") == self.folder)
                    && (text(&p["name"]).to_lowercase().contains(&q)
                        || strings(&p["tags"])
                            .iter()
                            .any(|t| t.to_lowercase().contains(&q)))
                    && self.filters.iter().all(|(k, v)| {
                        v.is_empty()
                            || match k.as_str() {
                                "proxy" => {
                                    if v == "with" {
                                        p["proxy"].is_object()
                                    } else {
                                        !p["proxy"].is_object()
                                    }
                                }
                                "engine" => {
                                    let e = s(p, "engine");
                                    (if e.is_empty() { "playwright" } else { e }) == v
                                }
                                "tag" => strings(&p["tags"]).contains(v),
                                _ => s(p, k) == v,
                            }
                    })
            })
            .collect()
    }
    pub fn toggle_all(&mut self) {
        let ids = self
            .filtered()
            .iter()
            .map(|p| s(p, "id").to_owned())
            .collect::<Vec<_>>();
        let all = !ids.is_empty() && ids.iter().all(|id| self.selected.contains(id));
        for id in ids {
            if all {
                self.selected.remove(&id);
            } else {
                self.selected.insert(id);
            }
        }
    }
    pub fn say(&mut self, msg: impl Into<String>, error: bool) {
        self.message = msg.into();
        self.message_error = error;
        self.next_toast += 1;
        #[cfg(target_arch = "wasm32")]
        let expires = js_sys::Date::now() + 3800.;
        #[cfg(not(target_arch = "wasm32"))]
        let expires = f64::INFINITY;
        self.toasts.push(Toast {
            id: self.next_toast,
            text: self.message.clone(),
            error,
            expires,
        });
    }
    pub fn toggle_set(set: &mut BTreeSet<String>, key: String) {
        if !set.remove(&key) {
            set.insert(key);
        }
    }
    pub fn poll_every(&self) -> u32 {
        if self.profiles.len() > 500 {
            12000
        } else {
            3000
        }
    }
    pub fn can_poll(&self) -> bool {
        self.live() && self.editor.is_none() && !["bulk", "sync"].contains(&self.modal.as_str())
    }
    pub fn open_editor(&mut self, p: Value) {
        self.editor = Some(Editor::new(p));
        self.schedule = Value::Null;
        self.form["preset"] = json!("general");
        self.form["replaceCookies"] = json!(false);
    }
    pub fn launch_ids(&self) -> Vec<String> {
        self.selected_profiles()
            .iter()
            .filter(|p| s(p, "status") != "running")
            .map(|p| s(p, "id").to_owned())
            .collect()
    }
    pub fn set_config(&mut self, c: Value) {
        self.config = c;
    }
    pub fn adopt_token(&self, old: &str, made: &Value) -> String {
        if list(&self.tokens["tokens"]).is_empty() && self.tokens["envToken"] != true {
            s(made, "token").into()
        } else {
            old.into()
        }
    }
    pub fn commands(&self) -> Vec<(String, String, String)> {
        let mut out = VIEWS
            .iter()
            .map(|(v, l)| (format!("Go to {l}"), "nav".into(), v.to_string()))
            .collect::<Vec<_>>();
        out.push(("New profile".into(), "new".into(), String::new()));
        out.push((
            "Toggle light / dark theme".into(),
            "theme".into(),
            String::new(),
        ));
        for p in self.profiles.iter().take(150) {
            out.push((
                format!("Open {}", text(&p["name"])),
                "edit".into(),
                s(p, "id").into(),
            ));
            out.push((
                format!(
                    "{} {}",
                    if s(p, "status") == "running" {
                        "Stop"
                    } else {
                        "Launch"
                    },
                    text(&p["name"])
                ),
                "run".into(),
                s(p, "id").into(),
            ));
        }
        out.into_iter()
            .filter(|c| {
                c.0.to_lowercase()
                    .contains(&self.command.trim().to_lowercase())
            })
            .take(40)
            .collect()
    }
}
