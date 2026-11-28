use super::*;

#[derive(Clone, Default, Debug)]
pub struct Editor {
    pub base: Value,
    pub draft: Value,
    pub conflicts: Vec<FieldPath>,
    pub tab: String,
    pub message: String,
    pub saving: bool,
    pub probe: Value,
}
impl Editor {
    pub fn new(p: Value) -> Self {
        Self {
            base: p.clone(),
            draft: p,
            tab: "general".into(),
            ..Self::default()
        }
    }
    pub fn editable(&self) -> bool {
        !self.saving && self.conflicts.is_empty()
    }
    pub fn set(&mut self, path: &[String], value: Value) {
        if !self.editable() {
            return;
        }
        with_field(&mut self.draft, path, value);
        if path == ["proxy", "pass"] {
            self.draft["_passwordSet"] = json!(true)
        }
    }
    pub fn resolve(&mut self, index: usize, latest: bool) -> bool {
        let Some(path) = self.conflicts.get(index).cloned() else {
            return false;
        };
        if latest {
            with_field(&mut self.draft, &path, field(&self.base, &path).clone());
            if path[0] == "proxy" && (path.len() == 1 || path[1] == "pass") {
                self.draft["_passwordSet"] = json!(false);
            }
        } else if path == ["proxy"] && self.draft["proxy"].is_object() {
            if !self.draft["proxy"]["pass"].is_string() {
                self.message="The proxy password is unavailable. Use latest, or cancel and reload after unlocking the vault before restoring this proxy.".into();
                return false;
            }
            self.draft["_passwordSet"] = json!(true);
        }
        self.conflicts.remove(index);
        self.message = "Conflict resolved. Review your edits and save again.".into();
        true
    }
    pub fn begin_save(&mut self, live: bool) -> Option<Value> {
        if !self.editable() || text(&self.draft["name"]).is_empty() {
            return None;
        }
        self.saving = true;
        self.message.clear();
        Some(if live && self.draft["_new"] != true {
            profile_write(&self.base, &self.draft)
        } else {
            let mut p = self.draft.clone();
            p["_new"] = json!(false);
            p
        })
    }
    pub fn stale(&mut self, latest: Value) {
        let (p, c) = rebase(&self.base, &self.draft, &latest);
        self.base = latest;
        self.draft = p;
        self.conflicts = c;
        self.saving = false;
        self.message=if self.conflicts.is_empty(){"Your edits were rebased onto the latest profile. Review them and save again."}else{"You and another writer both changed these fields. Choose which edits to keep before saving."}.into();
    }
    pub fn failed(&mut self, error: &crate::contracts::ApiError) {
        self.saving = false;
        self.message = format!("Save failed: {error}");
    }
}

#[derive(Clone, Debug)]
pub struct Batch {
    pub count: usize,
    pub os: Vec<String>,
    pub countries: Vec<String>,
    pub versions: Vec<String>,
    pub engine: String,
    pub prefix: String,
    pub start: usize,
    pub tags: String,
    pub randomize: bool,
}
impl Default for Batch {
    fn default() -> Self {
        Self {
            count: 50,
            os: vec!["Windows".into()],
            countries: vec!["US".into()],
            versions: VERSIONS.map(str::to_owned).to_vec(),
            engine: String::new(),
            prefix: "Batch".into(),
            start: 1,
            tags: "bulk".into(),
            randomize: true,
        }
    }
}
impl Batch {
    pub(crate) fn names(&self) -> impl DoubleEndedIterator<Item = String> + '_ {
        let count = self.count.clamp(1, 10000);
        let start = self.start as u128;
        let pad = (start + count as u128 - 1).to_string().len().max(2);
        (0..count).map(move |i| format!("{}-{:0pad$}", self.prefix, start + i as u128))
    }

    pub fn build(&self, folder: &str, mut random: impl FnMut(usize) -> usize) -> Vec<Value> {
        self.names()
            .enumerate()
            .map(|(i, name)| {
                let os = &self.os[i % self.os.len()];
                let country = &self.countries[(i / self.os.len()) % self.countries.len()];
                let version = &self.versions[random(self.versions.len()) % self.versions.len()];
                let mut p = new_profile(folder, &self.engine);
                change_os(&mut p, os);
                change_locale(&mut p, country_locale(country));
                p["name"] = json!(name);
                p["tags"] = json!(split_tags(&self.tags));
                p["browserVersion"] = json!(version);
                p["userAgent"] = json!(ua(os, version));
                let g = if self.randomize {
                    random(gpus(os).len()) % gpus(os).len()
                } else {
                    0
                };
                p["webglRenderer"] = json!(gpus(os)[g]);
                p["webglVendor"] = json!(vendor(gpus(os)[g]));
                if self.randomize {
                    p["screen"] = json!(screens(os)[random(screens(os).len()) % screens(os).len()]);
                    p["cores"] = json!(cores(os)[random(cores(os).len()) % cores(os).len()]);
                    p["memory"] = json!(ram(os)[random(ram(os).len()) % ram(os).len()]);
                }
                p
            })
            .collect()
    }
}
pub fn sync_patch(values: &Value) -> Value {
    let mut p = json!({});
    for k in ["engine", "locale", "folder"] {
        if !s(values, k).is_empty() {
            p[k] = values[k].clone();
        }
    }
    let tags = split_tags(s(values, "tags"));
    if !tags.is_empty() {
        p["addTags"] = json!(tags);
    }
    p
}
pub fn merge_patch(p: &mut Value, patch: &Value) {
    if let Some(fields) = patch.as_object() {
        for (k, v) in fields {
            if k != "addTags" {
                p[k] = v.clone();
            }
        }
    }
    if !s(patch, "locale").is_empty() {
        p["timezone"] = json!(zone(s(patch, "locale")));
    }
    if patch["addTags"].is_array() {
        let mut tags = list(&p["tags"]);
        for t in list(&patch["addTags"]) {
            if !tags.contains(&t) {
                tags.push(t)
            }
        }
        p["tags"] = json!(tags);
    }
}
pub fn export_profiles(profiles: &[Value]) -> (Value, String) {
    let withheld = profiles
        .iter()
        .filter(|p| p["proxy"].is_object() && p["proxy"]["pass"].is_null())
        .count();
    (
        json!({"numeraProfiles":1,"profiles":profiles}),
        format!(
            "Exported {} profile configuration{}. Browser sessions — cookies and logins — are not included; use `numera-engine backup` for those.{}",
            profiles.len(),
            if profiles.len() == 1 { "" } else { "s" },
            if withheld > 0 {
                format!(
                    " {withheld} proxy password{} locked in the vault and not in the file.",
                    if withheld == 1 { " is" } else { "s are" }
                )
            } else {
                String::new()
            }
        ),
    )
}
pub fn import_report(v: &Value) -> String {
    let n = number(v, "imported");
    let flagged = list(&v["results"])
        .iter()
        .filter(|r| !list(&r["issues"]).is_empty())
        .count();
    format!(
        "Imported {n} profile{}.{}{}",
        if n == 1 { "" } else { "s" },
        if s(v, "format") == "vendor" {
            " Read as a vendor export, so each fingerprint was regenerated from the fields that file did carry."
        } else {
            ""
        },
        if flagged > 0 {
            format!(" {flagged} flagged for review — open them to see the notes.")
        } else {
            String::new()
        }
    )
}
pub fn health_report(name: &str, action: &str, result: &Value) -> String {
    if action == "trust" {
        let g = if result["after"].is_object() {
            &result["after"]
        } else {
            result
        };
        format!(
            "{name}: trust {}.",
            if s(g, "grade").is_empty() {
                "no grade came back".into()
            } else {
                format!(
                    "{} ({}/100){}",
                    s(g, "grade"),
                    text(&g["score"]),
                    if number(g, "total") > 0 {
                        format!(
                            ", {} of {} checks passed",
                            text(&g["passed"]),
                            text(&g["total"])
                        )
                    } else {
                        String::new()
                    }
                )
            }
        )
    } else {
        let changed = strings(&result["changed"]);
        let before = s(&result["before"], "grade");
        let after = s(&result["after"], "grade");
        let moved = if !before.is_empty() && !after.is_empty() && before != after {
            format!(" {before} → {after}")
        } else {
            String::new()
        };
        if changed.is_empty() {
            format!("{name}: already aligned{moved} — nothing needed changing.")
        } else {
            format!(
                "{name}: aligned{moved} — rewrote {} to what the browser presents.",
                changed.join(", ")
            )
        }
    }
}
pub fn pool_password_patch(p: &Value) -> Value {
    let mut patch = json!({});
    for key in [
        "label",
        "type",
        "host",
        "port",
        "user",
        "pass",
        "country",
        "rotateUrl",
    ] {
        let value = p[key].clone();
        patch[key] = if key == "pass" {
            value
        } else if ["label", "user", "country", "rotateUrl"].contains(&key)
            && text(&value).is_empty()
        {
            Value::Null
        } else if key == "country" {
            json!(text(&value).to_uppercase())
        } else if key == "host" || key == "rotateUrl" {
            json!(text(&value).trim())
        } else {
            value
        };
    }
    patch
}
pub fn collision_ids(profiles: &[Value]) -> BTreeSet<String> {
    let mut by = BTreeMap::<String, Vec<String>>::new();
    for p in profiles {
        let sig = [
            "os",
            "userAgent",
            "screen",
            "cores",
            "memory",
            "webglRenderer",
        ]
        .map(|k| text(&p[k]))
        .join("|");
        by.entry(sig).or_default().push(s(p, "id").into());
    }
    by.into_values()
        .filter(|ids| ids.len() > 1)
        .flatten()
        .collect()
}
