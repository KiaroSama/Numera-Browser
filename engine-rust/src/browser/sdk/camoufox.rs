// SPDX-License-Identifier: MPL-2.0
// Native launch path for the Camoufox options Numera supplies. Corresponding
// source: daijro/camoufox 079a6ce8249bab98063e5ac9241e3879a3e6578d.
use super::{
    camoufox_coherence as coherence, camoufox_config as config, camoufox_cpu, camoufox_geo,
    camoufox_host,
    camoufox_identity::{self, Os},
    camoufox_webgl,
    driver::Driver,
    fingerprint::Model,
    options,
};
use crate::storage::archive_paths::Staging;
use playwright_rs::BrowserContext;
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, HashSet},
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

fn bad(message: &'static str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
fn read(path: &Path, limit: u64) -> io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    fs::File::open(path)?
        .take(limit + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(bad("Camoufox resource exceeds limit"));
    }
    Ok(bytes)
}
fn resource_root(binary: &Path) -> io::Result<PathBuf> {
    let parent = binary
        .parent()
        .ok_or_else(|| bad("Camoufox binary has no parent"))?;
    let resources = parent.parent().map(|p| p.join("Resources"));
    Ok(resources
        .filter(|p| p.join("properties.json").is_file())
        .unwrap_or_else(|| parent.to_owned()))
}
pub fn binary(options: &Map<String, Value>) -> io::Result<PathBuf> {
    let selected = options
        .get("executable_path")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("CAMOUFOX_EXECUTABLE_PATH").map(PathBuf::from))
        .ok_or_else(|| bad("Camoufox requires a provisioned browser executable"))?;
    if !selected.is_file() {
        return Err(bad("Camoufox browser executable is missing"));
    }
    fs::canonicalize(selected)
}
fn version(root: &Path, options: &Map<String, Value>) -> io::Result<u32> {
    if let Some(value) = options.get("ff_version") {
        return value
            .as_u64()
            .or_else(|| value.as_str().and_then(|v| v.parse().ok()))
            .and_then(|v| u32::try_from(v).ok())
            .filter(|v| (100..=999).contains(v))
            .ok_or_else(|| bad("Camoufox Firefox version is invalid"));
    }
    // A caller-selected build is versioned from beside that binary, never from
    // an unrelated managed installation or a requested package label.
    let bytes = read(&root.join("application.ini"), 65536)?;
    let text =
        std::str::from_utf8(&bytes).map_err(|_| bad("Camoufox version metadata is invalid"))?;
    let mut section = "";
    for line in text.lines().map(str::trim) {
        if line.starts_with('[') {
            section = line;
        }
        if section == "[App]"
            && let Some(value) = line.strip_prefix("Version=")
        {
            return value
                .split('.')
                .next()
                .and_then(|v| v.parse::<u32>().ok())
                .filter(|v| (100..=999).contains(v))
                .ok_or_else(|| bad("Camoufox version metadata is invalid"));
        }
    }
    Err(bad("Camoufox version metadata is missing"))
}
fn environment() -> Map<String, Value> {
    std::env::vars()
        .filter(|(key, _)| {
            key != "NUMERA_PASSWORD"
                && key != "NUMERA_API_TOKEN"
                && key != "NUMERA_UPDATE_TOKEN"
                && key != "NUMERA_UPDATE_TICKET"
                && key != "NUMERA_UPDATE_ACTIVATION"
                && !key.starts_with("CAMOU_CONFIG_")
                && !key.starts_with("CAMOU_PREFS_")
                && key != "FONTCONFIG_FILE"
        })
        .map(|(key, value)| (key, value.into()))
        .collect()
}
fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
pub fn fontconfig(root: &Path, os: Os, output: &Path) -> io::Result<PathBuf> {
    crate::browser::camoufox_fonts::require(root)?;
    let name = config::os_name(os);
    let source = [
        root.join("fontconfig").join(name).join("fonts.conf"),
        root.join("fontconfigs").join(name).join("fonts.conf"),
    ]
    .into_iter()
    .find(|path| path.is_file())
    .ok_or_else(|| bad("Camoufox font configuration is missing"))?;
    let fonts = root.join("fonts");
    let mut directories = Vec::new();
    if fonts.join("groups.json").is_file() {
        let groups: Value = serde_json::from_slice(&read(&fonts.join("groups.json"), 1_048_576)?)
            .map_err(|_| bad("Camoufox font groups are invalid"))?;
        if let Some(groups) = groups["readBy"][os.key()].as_array() {
            let letter = match os {
                Os::Windows => 'W',
                Os::Macos => 'M',
                Os::Linux => 'L',
            };
            for group in groups {
                let group = group
                    .as_str()
                    .filter(|g| {
                        ["L", "M", "W", "LM", "LW", "MW", "LMW"].contains(g) && g.contains(letter)
                    })
                    .ok_or_else(|| bad("Camoufox font group is invalid"))?;
                if fonts.join(group).is_dir() {
                    directories.push(fonts.join(group));
                }
            }
        }
    }
    if directories.is_empty() {
        directories.push(if fonts.join(name).is_dir() {
            fonts.join(name)
        } else {
            fonts
        });
    }
    let replacement = directories
        .iter()
        .map(|path| {
            path.to_str()
                .map(|p| format!("<dir>{}</dir>", xml(p)))
                .ok_or_else(|| bad("Camoufox font path is not Unicode"))
        })
        .collect::<io::Result<Vec<_>>>()?
        .join("\n\t");
    let source = String::from_utf8(read(&source, 1_048_576)?)
        .map_err(|_| bad("Camoufox font configuration is invalid"))?;
    if !source.contains("<dir prefix=\"cwd\">fonts</dir>") {
        return Err(bad("Camoufox font configuration has no bundle directory"));
    }
    let path = output.join("fonts.conf");
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)?;
    file.write_all(
        source
            .replace("<dir prefix=\"cwd\">fonts</dir>", &replacement)
            .as_bytes(),
    )?;
    file.sync_all()?;
    Ok(path)
}
fn validate(config: &Map<String, Value>, root: &Path, phase: &mut &'static str) -> io::Result<()> {
    let schema: Value = serde_json::from_slice(&read(&root.join("properties.json"), 1_048_576)?)
        .map_err(|_| bad("Camoufox properties are invalid"))?;
    let schema = schema
        .as_array()
        .ok_or_else(|| bad("Camoufox properties are invalid"))?;
    for (key, value) in config {
        let Some(entry) = schema
            .iter()
            .find(|entry| entry["property"].as_str() == Some(key))
        else {
            continue;
        };
        if entry.get("removed").is_some() {
            continue;
        }
        let number = value
            .as_f64()
            .or_else(|| value.as_bool().map(|v| f64::from(u8::from(v))));
        let valid = match entry["type"].as_str() {
            Some("str") => value.is_string(),
            Some("bool") => value.is_boolean(),
            Some("array") => value.is_array(),
            Some("dict") => value.is_object(),
            Some("double") => number.is_some_and(f64::is_finite),
            Some("int") => number.is_some_and(|n| n.is_finite() && n.fract() == 0.0),
            Some("uint") => number.is_some_and(|n| n.is_finite() && n >= 0.0 && n.fract() == 0.0),
            _ => false,
        };
        if !valid {
            *phase = match key.as_str() {
                "navigator.userAgent" => "schema-user-agent",
                "navigator.appVersion" => "schema-app-version",
                "navigator.platform" => "schema-platform",
                "navigator.oscpu" => "schema-oscpu",
                "navigator.hardwareConcurrency" => "schema-cores",
                "navigator.maxTouchPoints" => "schema-touch",
                "screen.availWidth" => "schema-available-width",
                "screen.availHeight" => "schema-available-height",
                "screen.width" => "schema-width",
                "screen.height" => "schema-height",
                "screen.colorDepth" => "schema-color",
                "screen.pixelDepth" => "schema-pixel",
                "screen.availLeft" => "schema-available-left",
                "screen.availTop" => "schema-available-top",
                "window.outerWidth" => "schema-outer-width",
                "window.outerHeight" => "schema-outer-height",
                "window.screenX" => "schema-position-x",
                "window.screenY" => "schema-position-y",
                "window.devicePixelRatio" => "schema-scale",
                "fonts" => "schema-fonts",
                "addons" => "schema-addons",
                _ => "schema-other",
            };
            return Err(bad("Camoufox property has an invalid type"));
        }
    }
    Ok(())
}

#[derive(Default)]
pub(super) struct Prepared {
    stages: Vec<Staging>,
    probe: Option<BrowserContext>,
    pub phase: &'static str,
}
impl Prepared {
    pub async fn cleanup_probe(&mut self) -> bool {
        if let Some(context) = self.probe.take() {
            matches!(
                tokio::time::timeout(Duration::from_secs(10), context.close()).await,
                Ok(Ok(()))
            )
        } else {
            true
        }
    }
    /// Keep temporary profiles/fontconfig owned until driver cleanup even when
    /// preparation or a probe fails. The enclosing worker owns this guard first.
    pub async fn prepare(
        &mut self,
        input: &Map<String, Value>,
        driver: &Driver,
    ) -> playwright_rs::Result<Value> {
        self.phase = "resources";
        let os = config::parse_os(input.get("os").and_then(Value::as_str).unwrap_or("windows"))?;
        let executable = binary(input)?;
        let root = resource_root(&executable)?;
        let major = version(&root, input)?;
        let properties: Value =
            serde_json::from_slice(&read(&root.join("properties.json"), 1_048_576)?)
                .map_err(|_| bad("Camoufox properties are invalid"))?;
        let cpu_available = camoufox_cpu::supported(&properties);
        let requested_cpu = input.get("cpu").filter(|v| !v.is_null());
        let mut cpu_config = Map::new();
        let mut cpu_keep = HashSet::new();
        if let Some(caller) = input.get("config").and_then(Value::as_object) {
            for key in ["cpu:arch", "cpu:manufacturer"] {
                if let Some(value) = caller.get(key) {
                    cpu_config.insert(key.into(), value.clone());
                    cpu_keep.insert(key.into());
                }
            }
        }
        let cpu = camoufox_cpu::prepare(&mut cpu_config, requested_cpu, os, cpu_available)?;
        let profile = input
            .get("user_data_dir")
            .and_then(Value::as_str)
            .filter(|v| !v.is_empty())
            .ok_or_else(|| bad("Camoufox profile directory is missing"))?;
        fs::create_dir_all(profile)?;
        let profile = fs::canonicalize(profile)?;
        let headless = input
            .get("headless")
            .map_or(Some(false), Value::as_bool)
            .ok_or_else(|| bad("Camoufox headless option is invalid"))?;
        let stage = Staging::new(&profile, ".numera-camoufox-")?;
        let scratch = stage.path()?.to_owned();
        self.stages.push(stage);
        let mut env = environment();
        self.phase = "fonts";
        if cfg!(target_os = "linux") {
            let path = fontconfig(&root, os, &scratch)?;
            env.insert(
                "FONTCONFIG_FILE".into(),
                path.to_str()
                    .ok_or_else(|| bad("Camoufox font path is not Unicode"))?
                    .into(),
            );
        }
        let mut display = None;
        let mut host_gpu = None;
        self.phase = "probe";
        if !headless || os == config::host_os() {
            fs::create_dir(scratch.join("probe"))?;
            let mut probe_env = env.clone();
            probe_env.remove("FONTCONFIG_FILE");
            let probe_options = options::persistent(
                json!({"user_data_dir":scratch.join("probe"),"executable_path":executable,
                "headless":headless,"viewport":null,"env":probe_env})
                .as_object()
                .ok_or_else(|| bad("Camoufox probe options are invalid"))?,
            )?;
            let context = driver.open("camoufox", probe_options).await?;
            self.probe = Some(context.clone());
            let observation = tokio::time::timeout(Duration::from_secs(30), async {
                let page = match context.pages().into_iter().next() {
                    Some(page) => page,
                    None => context.new_page().await?,
                };
                let expression =
                    crate::page_runtime::expression(crate::page_runtime::Program::HostEnvironment);
                let value = super::evaluate_raw(&page, &expression, &Value::Null, false).await?;
                super::values::decode(&value).map_err(playwright_rs::Error::from)
            })
            .await;
            if !self.cleanup_probe().await {
                return Err(bad("Camoufox host probe did not close").into());
            }
            if let Ok(Ok(observed)) = observation {
                if !headless
                    && let (Some(width), Some(height)) =
                        (observed["width"].as_i64(), observed["height"].as_i64())
                {
                    display = Some(config::Display { width, height });
                }
                if os == config::host_os()
                    && let (Some(vendor), Some(renderer)) =
                        (observed["gpu"][0].as_str(), observed["gpu"][1].as_str())
                {
                    host_gpu = Some((vendor.to_owned(), renderer.to_owned()));
                }
            }
        }
        self.phase = "model";
        let model = if let Some(path) = std::env::var_os("NUMERA_CAMOUFOX_MODEL") {
            Model::open(&PathBuf::from(path))?
        } else {
            Model::embedded()?
        };
        let generated = model.generate(&BTreeMap::from([
            ("browser".into(), "Firefox".into()),
            (
                "os".into(),
                match os {
                    Os::Windows => "Windows",
                    Os::Macos => "macOS",
                    Os::Linux => "Linux",
                }
                .into(),
            ),
        ]))?;
        self.phase = "geometry";
        let mut identity = config::from_fpgen(&generated, major)?;
        if let Some(dpr) = generated["window"]
            .get("devicePixelRatio")
            .filter(|dpr| coherence::is_zoomed_ratio(dpr, os))
        {
            identity.insert("window.devicePixelRatio".into(), dpr.clone());
            coherence::repair_device_pixel_ratio(&mut identity, os, None);
        }
        config::normalize(&mut identity, os, camoufox_host::cpu_count(), display);
        let mut salt = [0; 8];
        getrandom::fill(&mut salt).map_err(|_| bad("Camoufox identity entropy unavailable"))?;
        let seed = config::identity_seed(&identity, u64::from_le_bytes(salt));
        let native_fonts = os == config::host_os() && matches!(os, Os::Windows | Os::Macos);
        let variant = os == Os::Windows
            && std::env::var_os("WINDIR")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
                .join("Fonts/SegUIVar.ttf")
                .is_file();
        self.phase = "catalogs";
        let fonts = camoufox_identity::font_subset(os, seed, native_fonts, None, variant);
        identity.insert("fonts".into(), json!(fonts));
        let media = camoufox_identity::media_devices(os, seed);
        identity.extend(
            media
                .as_object()
                .ok_or_else(|| bad("Camoufox media data is invalid"))?
                .clone(),
        );
        self.phase = "locale";
        identity.extend(camoufox_geo::resolve(input).await?);
        identity.extend(cpu_config);
        let locale = identity
            .get("locale:language")
            .and_then(Value::as_str)
            .map(|language| {
                identity
                    .get("locale:region")
                    .and_then(Value::as_str)
                    .map_or_else(
                        || language.to_owned(),
                        |region| format!("{language}-{region}"),
                    )
            });
        identity.extend(
            camoufox_identity::voice_config(os, seed, locale.as_deref())
                .as_object()
                .ok_or_else(|| bad("Camoufox voice data is invalid"))?
                .clone(),
        );
        identity.insert("voices:blockIfNotDefined".into(), true.into());
        self.phase = "webgl";
        let gpus = camoufox_webgl::firefox_gpus(&model, os)?;
        let known_host = host_gpu.as_ref().filter(|gpu| gpus.contains(gpu));
        let cover_canvas = os != config::host_os() || (host_gpu.is_some() && known_host.is_none());
        let mut webgl = camoufox_webgl::sample_for_cpu(
            &model,
            os,
            identity.get("screen.width").and_then(Value::as_i64),
            identity.get("screen.height").and_then(Value::as_i64),
            identity
                .get("navigator.hardwareConcurrency")
                .and_then(Value::as_i64),
            seed,
            major,
            known_host.map(|(v, r)| (v.as_str(), r.as_str())),
            cpu.as_ref(),
        )?;
        let enable_webgl2 = webgl
            .as_object_mut()
            .ok_or_else(|| bad("Camoufox WebGL data is invalid"))?
            .remove("webGl2Enabled")
            .unwrap_or(false.into());
        identity.extend(
            webgl
                .as_object()
                .ok_or_else(|| bad("Camoufox WebGL data is invalid"))?
                .clone(),
        );
        self.phase = "coherence";
        if cpu_available {
            if requested_cpu.is_some() && camoufox_cpu::conflict(&identity, os).is_some() {
                return Err(
                    bad("Camoufox requested CPU contradicts the generated hardware").into(),
                );
            }
            camoufox_cpu::set_default(&mut identity, os, u64::from_le_bytes(salt));
        }
        let violations = coherence::apply(&mut identity, os, &cpu_keep);
        if !violations.is_empty() {
            return Err(bad("Camoufox generated an incoherent identity").into());
        }
        self.phase = "preferences";
        let mut prefs = camoufox_geo::firefox_preferences(&identity);
        prefs.extend(Map::from_iter([
            ("webgl.enable-webgl2".into(), enable_webgl2),
            ("webgl.force-enabled".into(), true.into()),
            ("privacy.donottrackheader.enabled".into(), false.into()),
            ("privacy.globalprivacycontrol.enabled".into(), false.into()),
            (
                "mousewheel.system_scroll_override.enabled".into(),
                (os != Os::Macos).into(),
            ),
        ]));
        let constants: Value =
            serde_json::from_str(include_str!("camoufox-data/launcher-constants.json"))
                .expect("Compiled Camoufox constants");
        let win11 = fonts.is_empty()
            || fonts.iter().any(|font| {
                constants["windows11MarkerFonts"]
                    .as_array()
                    .is_some_and(|markers| {
                        markers.iter().any(|marker| marker.as_str() == Some(font))
                    })
            });
        prefs.insert(
            "ui.useOverlayScrollbars".into(),
            i32::from(os != Os::Windows || win11).into(),
        );
        if os == Os::Windows && config::host_os() != Os::Windows {
            prefs.insert("javascript.options.use_ucrt_math".into(), true.into());
            identity.insert("input:windowsKeyboard".into(), true.into());
            prefs.extend(
                constants["windowsBehaviourPrefs"]
                    .as_object()
                    .ok_or_else(|| bad("Camoufox behavior preferences are invalid"))?
                    .clone(),
            );
            prefs.insert(
                "widget.non-native-theme.scrollbar.style".into(),
                if win11 { 5 } else { 4 }.into(),
            );
        }
        if os == Os::Linux {
            prefs.insert("gfx.font_rendering.fallback.async".into(), false.into());
        }
        if let Some(capacity) = camoufox_host::stock_disk_limit() {
            prefs.insert(
                "dom.quotaManager.temporaryStorage.fixedLimit".into(),
                capacity.into(),
            );
        }
        if cover_canvas {
            prefs.insert("privacy.baselineFingerprintingProtection.overrides".into(), "+CanvasImageExtractionPrompt,+CanvasExtractionBeforeUserInputIsBlocked,+CanvasExtractionFromThirdPartiesIsBlocked".into());
        }
        let webgpu = os == Os::Windows
            || (os == Os::Macos
                && identity.get("webGl:vendor").and_then(Value::as_str) == Some("Apple"));
        prefs.insert("dom.webgpu.enabled".into(), webgpu.into());
        if webgpu {
            let hardware_host =
                known_host.is_some_and(|(_, r)| !coherence::is_software_renderer(Some(r)));
            prefs.insert(
                "gfx.blacklist.webgpu".into(),
                if hardware_host { 1 } else { 4 }.into(),
            );
        }
        if let Some(dpr) = identity
            .get("window.devicePixelRatio")
            .and_then(Value::as_f64)
            .filter(|v| *v > 0.0)
        {
            prefs.insert("layout.css.devPixelsPerPx".into(), dpr.to_string().into());
        }
        if cpu_available {
            if let Some(caller) = input.get("firefox_user_prefs").and_then(Value::as_object) {
                for key in ["general.cpu_identity", "media.libvpx.realtime_only"] {
                    if let Some(value) = caller.get(key) {
                        prefs.insert(key.into(), value.clone());
                    }
                }
            }
            camoufox_cpu::preferences(&identity, os, &mut prefs);
        }
        self.phase = "addons";
        let mut addons = input.get("addons").map_or(Ok(Vec::new()), |v| {
            v.as_array()
                .cloned()
                .ok_or_else(|| bad("Camoufox addons are invalid"))
        })?;
        let excluded = input
            .get("exclude_addons")
            .and_then(Value::as_array)
            .is_some_and(|v| v.iter().any(|v| v.as_str() == Some("UBO")));
        if !excluded {
            let addon = [root.join("addons/UBO"), root.join("addons/ublock-origin")]
                .into_iter()
                .find(|path| path.is_dir())
                .ok_or_else(|| bad("Camoufox default addon is missing"))?;
            addons.push(
                addon
                    .to_str()
                    .ok_or_else(|| bad("Camoufox addon path is not Unicode"))?
                    .into(),
            );
        }
        for addon in &addons {
            if !addon.as_str().is_some_and(|path| {
                Path::new(path).is_dir() && Path::new(path).join("manifest.json").is_file()
            }) {
                return Err(bad("Camoufox addon is missing or invalid").into());
            }
        }
        if !addons.is_empty() {
            identity.insert("addons".into(), addons.into());
        }
        self.phase = "schema";
        validate(&identity, &root, &mut self.phase)?;
        self.phase = "environment";
        env.extend(config::chunk_environment(
            "CAMOU_CONFIG",
            &serde_json::to_string(&identity).map_err(io::Error::other)?,
            cfg!(windows),
        ));
        env.extend(config::chunk_environment(
            "CAMOU_PREFS",
            &config::ascii_json(&json!(prefs))?,
            cfg!(windows),
        ));
        let mut launch = json!({"user_data_dir":profile,"executable_path":executable,"headless":headless,"env":env,"firefox_user_prefs":prefs,"viewport":null,
            "args":input.get("args").cloned().unwrap_or(json!([]))});
        if let Some(proxy) = input.get("proxy").filter(|v| !v.is_null()) {
            launch["proxy"] = proxy.clone();
        }
        self.phase = "launch-options";
        let prepared = options::persistent(
            launch
                .as_object()
                .ok_or_else(|| bad("Camoufox launch options are invalid"))?,
        )?;
        self.phase = "driver-launch";
        Ok(prepared)
    }
}
