// SPDX-License-Identifier: MPL-2.0
// Native port of Camoufox's identity catalog draws, with unchanged catalog data.
// Source: daijro/camoufox, 079a6ce8249bab98063e5ac9241e3879a3e6578d.
use crate::python_random::PythonRandom;
use serde_json::{Value, json};
use std::{collections::HashSet, sync::OnceLock};

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Windows,
    Macos,
    Linux,
}
impl Os {
    pub fn key(self) -> &'static str {
        match self {
            Self::Windows => "win",
            Self::Macos => "mac",
            Self::Linux => "lin",
        }
    }
}
struct Catalog {
    constants: Value,
    fonts: Value,
    bases: Value,
    groups: Value,
    essential: Value,
    media: Value,
    voices: Value,
    uris: Value,
}
fn catalog() -> &'static Catalog {
    static DATA: OnceLock<Catalog> = OnceLock::new();
    DATA.get_or_init(|| {
        let parse =
            |text| serde_json::from_str(text).expect("Compiled Camoufox catalog is valid JSON");
        Catalog {
            constants: parse(include_str!("camoufox-data/launcher-constants.json")),
            fonts: parse(include_str!("camoufox-data/fonts.json")),
            bases: parse(include_str!("camoufox-data/font-bases.json")),
            groups: parse(include_str!("camoufox-data/font-groups.json")),
            essential: parse(include_str!("camoufox-data/essential-fonts.json")),
            media: parse(include_str!("camoufox-data/media-devices.json")),
            voices: parse(include_str!("camoufox-data/voice-manifests.json")),
            uris: parse(include_str!("camoufox-data/voice-uris.json")),
        }
    })
}
fn rows(value: &Value) -> &[Value] {
    value.as_array().map(Vec::as_slice).unwrap_or(&[])
}
fn strings(value: &Value) -> Vec<String> {
    rows(value)
        .iter()
        .map(|v| {
            v.as_str()
                .expect("Compiled catalog font is text")
                .to_owned()
        })
        .collect()
}
fn probability(value: &Value, key: &str, default: f64) -> f64 {
    value[key].as_f64().unwrap_or(default)
}
fn release(os: Os, rng: &mut PythonRandom) -> Option<&'static Value> {
    let bases = rows(&catalog().bases[os.key()]);
    if bases.is_empty() {
        return None;
    }
    let roll = rng.random();
    let mut cumulative = 0.0;
    for base in bases {
        cumulative += probability(base, "weight", 0.0);
        if roll < cumulative {
            return Some(base);
        }
    }
    bases.last()
}

fn add_voices(output: &mut Vec<String>, seen: &mut HashSet<String>, entries: &Value) {
    for entry in strings(entries) {
        if seen.insert(entry.clone()) {
            output.push(entry);
        }
    }
}
fn take_pack(
    rng: &mut PythonRandom,
    pack: &Value,
    output: &mut Vec<String>,
    seen: &mut HashSet<String>,
    legacy: &mut Vec<String>,
) {
    add_voices(output, seen, &pack["oneCore"]);
    let desktop = strings(&pack["desktop"]);
    if !desktop.is_empty() && rng.random() < probability(pack, "desktopProb", 0.0) {
        for entry in desktop {
            if !legacy.contains(&entry) {
                legacy.push(entry);
            }
        }
    }
}
fn pick<'a>(rng: &mut PythonRandom, items: &'a [Value]) -> &'a Value {
    let mut roll = rng.random()
        * items
            .iter()
            .map(|item| probability(item, "w", 0.0))
            .sum::<f64>();
    for item in items {
        roll -= probability(item, "w", 0.0);
        if roll <= 0.0 {
            return item;
        }
    }
    items.last().expect("Compiled voice size pool is nonempty")
}
fn sample_weighted(
    rng: &mut PythonRandom,
    mut pool: Vec<String>,
    count: usize,
    weight: impl Fn(&str) -> f64,
) -> Vec<String> {
    let mut result = Vec::new();
    while !pool.is_empty() && result.len() < count {
        let mut roll = rng.random() * pool.iter().map(|entry| weight(entry)).sum::<f64>();
        let mut selected = pool.len() - 1;
        for (index, entry) in pool.iter().enumerate() {
            roll -= weight(entry);
            if roll <= 0.0 {
                selected = index;
                break;
            }
        }
        result.push(pool.remove(selected));
    }
    result
}
fn display_pack<'a>(
    packs: &'a serde_json::Map<String, Value>,
    fallback: &'a str,
    locale: Option<&str>,
) -> &'a str {
    if let Some(locale) = locale {
        if let Some(key) = packs.keys().find(|key| key.as_str() == locale) {
            return key;
        }
        let language = locale.split('-').next().unwrap_or("").to_ascii_lowercase();
        for key in packs.keys() {
            if key.split('-').next().unwrap_or("").to_ascii_lowercase() == language {
                return key;
            }
        }
    }
    if packs.contains_key(fallback) {
        fallback
    } else {
        packs.keys().next().expect("Voice pack map is nonempty")
    }
}
fn voice_uri(os: Os, name: &str, language: &str) -> String {
    if os == Os::Windows {
        return format!("urn:moz-tts:sapi:{name}?{language}");
    }
    if let Some(uri) = catalog().uris["mac"][format!("{name}|{language}")].as_str() {
        return uri.into();
    }
    // All uncatalogued names in this pinned manifest are ASCII novelty voices.
    // Refuse a future non-ASCII entry until its catalog/normalization is ported.
    assert!(
        name.is_ascii(),
        "Uncatalogued Camoufox voice needs an explicit URI"
    );
    let compact: String = name.chars().filter(char::is_ascii_alphanumeric).collect();
    if strings(&catalog().constants["macNoveltyVoices"])
        .iter()
        .any(|item| item == name)
    {
        format!("urn:moz-tts:osx:com.apple.speech.synthesis.voice.{compact}")
    } else if strings(&catalog().constants["macEloquenceVoices"])
        .iter()
        .any(|item| item == name)
    {
        format!("urn:moz-tts:osx:com.apple.eloquence.{language}.{compact}")
    } else {
        format!("urn:moz-tts:osx:com.apple.voice.compact.{language}.{compact}")
    }
}
pub fn voice_config(os: Os, seed: u32, locale: Option<&str>) -> Value {
    let mut rng = PythonRandom::new(i64::from(seed));
    if os == Os::Linux {
        let releases = catalog().voices["lin"]["releases"]
            .as_object()
            .expect("Compiled speech releases are an object");
        let selected = release(os, &mut rng)
            .and_then(|release| release["id"].as_str())
            .and_then(|id| releases.get(id))
            .or_else(|| releases.values().next())
            .expect("Compiled speech release exists");
        return json!({"voices:speechd":selected["speechd"]});
    }
    let manifest = &catalog().voices[os.key()];
    let packs = manifest["langPacks"].as_object();
    let mut output = Vec::new();
    let mut seen = HashSet::new();
    let mut legacy = Vec::new();
    let mut chosen = HashSet::new();
    add_voices(&mut output, &mut seen, &manifest["base"]);
    if let Some(packs) = packs.filter(|packs| !packs.is_empty()) {
        let selected = display_pack(
            packs,
            manifest["fallbackLocale"].as_str().unwrap_or("en-US"),
            locale,
        );
        chosen.insert(selected.to_owned());
        take_pack(
            &mut rng,
            &packs[selected],
            &mut output,
            &mut seen,
            &mut legacy,
        );
    }
    for addition in rows(&manifest["additions"]) {
        if addition["deferred"] == true {
            continue;
        }
        if let Some(required) = addition["requiresLocale"].as_str()
            && !locale
                .unwrap_or("")
                .to_ascii_lowercase()
                .starts_with(&required.to_ascii_lowercase())
        {
            continue;
        }
        if rng.random() >= probability(addition, "prob", 0.0) {
            continue;
        }
        match addition["kind"].as_str() {
            Some("bundle") => add_voices(&mut output, &mut seen, &addition["voices"]),
            Some("alacarte") => {
                let sizes = rows(&addition["sizes"]);
                let count = if sizes.is_empty() {
                    1
                } else {
                    pick(&mut rng, sizes)["n"]
                        .as_u64()
                        .expect("Compiled voice count is integral") as usize
                };
                for entry in sample_weighted(&mut rng, strings(&addition["voices"]), count, |_| 1.0)
                {
                    if seen.insert(entry.clone()) {
                        let index = output
                            .iter()
                            .position(|voice| voice.to_lowercase() > entry.to_lowercase())
                            .unwrap_or(output.len());
                        output.insert(index, entry);
                    }
                }
            }
            Some("groups") => {
                if let Some(packs) = packs {
                    let eligible = strings(&addition["groups"])
                        .into_iter()
                        .filter(|key| packs.contains_key(key) && !chosen.contains(key))
                        .collect::<Vec<_>>();
                    if eligible.is_empty() {
                        continue;
                    }
                    let sizes = rows(&addition["sizes"]);
                    let count = if sizes.is_empty() {
                        eligible.len()
                    } else {
                        pick(&mut rng, sizes)["n"]
                            .as_u64()
                            .expect("Compiled voice count is integral")
                            as usize
                    };
                    for selected in sample_weighted(&mut rng, eligible, count, |key| {
                        let weight = probability(&packs[key], "weight", 0.01);
                        if weight == 0.0 { 0.01 } else { weight }
                    }) {
                        chosen.insert(selected.clone());
                        take_pack(
                            &mut rng,
                            &packs[&selected],
                            &mut output,
                            &mut seen,
                            &mut legacy,
                        );
                    }
                }
            }
            _ => {}
        }
    }
    let default = manifest["systemVoice"]["default"].as_str();
    let voices = output.into_iter().chain(legacy).map(|entry| {
        let mut parts = entry.rsplitn(3, ':');
        let kind = parts.next().expect("Compiled voice has a type"); let language = parts.next().expect("Compiled voice has a language"); let name = parts.next().expect("Compiled voice has a name");
        json!({"name":name,"lang":language,"voiceUri":voice_uri(os,name,language),"isDefault":default == Some(name),"isLocalService":kind == "local"})
    }).collect::<Vec<_>>();
    json!({"voices":voices})
}
pub fn font_subset(
    os: Os,
    seed: u32,
    native: bool,
    locale: Option<&str>,
    has_variant_fonts: bool,
) -> Vec<String> {
    let data = catalog();
    let key = os.key();
    let full = strings(&data.fonts[key]);
    let essential = strings(&data.essential[key])
        .into_iter()
        .collect::<HashSet<_>>();
    if native {
        let mut result = full
            .iter()
            .filter(|font| essential.contains(*font))
            .cloned()
            .collect::<Vec<_>>();
        let all = full.iter().cloned().collect::<HashSet<_>>();
        let mut extra = essential.difference(&all).cloned().collect::<Vec<_>>();
        extra.sort();
        result.extend(extra);
        if os == Os::Windows && !has_variant_fonts {
            let variants = strings(&data.constants["windows11MarkerFonts"])
                .into_iter()
                .collect::<HashSet<_>>();
            result.retain(|font| !variants.contains(font));
        }
        return result;
    }
    let mut rng = PythonRandom::new(i64::from(seed));
    let mut result = release(os, &mut rng)
        .map(|base| strings(&base["fonts"]))
        .unwrap_or_default();
    if result.is_empty() {
        result = full
            .iter()
            .filter(|font| essential.contains(*font))
            .cloned()
            .collect();
    }
    let mut chosen = result.iter().cloned().collect::<HashSet<_>>();
    for font in &full {
        if essential.contains(font) && chosen.insert(font.clone()) {
            result.push(font.clone());
        }
    }
    let mut additions = Vec::new();
    for unit in rows(&data.groups[key]) {
        if let Some(required) = unit["requiresLocale"].as_str()
            && !locale
                .unwrap_or("")
                .to_ascii_lowercase()
                .starts_with(&required.to_ascii_lowercase())
        {
            continue;
        }
        if rng.random() >= probability(unit, "prob", 0.0) {
            continue;
        }
        let mut members = strings(&unit["fonts"])
            .into_iter()
            .filter(|font| !chosen.contains(font))
            .collect::<Vec<_>>();
        if members.is_empty() {
            continue;
        }
        if unit["kind"] == "alacarte" {
            let sizes = rows(&unit["sizes"]);
            let mut count = sizes
                .last()
                .and_then(|s| s["n"].as_u64())
                .map_or(members.len(), |n| n as usize);
            let roll = rng.random()
                * if sizes.is_empty() {
                    1.0
                } else {
                    sizes.iter().map(|s| probability(s, "w", 0.0)).sum()
                };
            let mut cumulative = 0.0;
            for size in sizes {
                cumulative += probability(size, "w", 0.0);
                if roll < cumulative {
                    count = size["n"].as_u64().expect("Compiled font count is integral") as usize;
                    break;
                }
            }
            let indices = rng.sample_indices(members.len(), count.clamp(1, members.len()));
            members = indices.into_iter().map(|i| members[i].clone()).collect();
        }
        additions.extend(members);
    }
    for font in additions
        .into_iter()
        .chain(strings(&data.constants["markerFonts"][key]))
    {
        if chosen.insert(font.clone()) {
            result.push(font);
        }
    }
    result
}

fn weighted<'a>(rng: &mut PythonRandom, items: &[&'a Value]) -> &'a Value {
    let mut roll = rng.random()
        * items
            .iter()
            .map(|item| probability(item, "w", 1.0))
            .sum::<f64>();
    for item in items {
        roll -= probability(item, "w", 1.0);
        if roll < 0.0 {
            return item;
        }
    }
    items
        .last()
        .copied()
        .expect("Compiled media pool is nonempty")
}
#[derive(Default)]
struct Media {
    microphones: Vec<(String, String)>,
    speakers: Vec<(String, String)>,
    cameras: Vec<(String, String)>,
    counter: u32,
}
impl Media {
    fn group(&mut self) -> String {
        self.counter += 1;
        format!("hw-{}", self.counter)
    }
    fn add(&mut self, item: &Value, group: &str, skip_mic: bool) {
        if !skip_mic {
            self.microphones.extend(
                strings(&item["mics"])
                    .into_iter()
                    .map(|name| (name, group.to_owned())),
            );
        }
        self.speakers.extend(
            strings(&item["outs"])
                .into_iter()
                .map(|name| (name, group.to_owned())),
        );
        if let Some(camera) = item["cam"].as_str().filter(|name| !name.is_empty()) {
            let group = self.group();
            self.cameras.push((camera.into(), group));
        }
    }
}
pub fn media_devices(os: Os, seed: u32) -> Value {
    let data = &catalog().media[os.key()];
    let mut rng = PythonRandom::new(i64::from(seed));
    let mut media = Media::default();
    let card = weighted(&mut rng, &rows(&data["cards"]).iter().collect::<Vec<_>>());
    let no_mic = rng.random()
        < match os {
            Os::Windows => 0.08,
            Os::Macos => 0.0,
            Os::Linux => 0.20,
        };
    let group = media.group();
    media.add(card, &group, no_mic);
    let p_cam = match os {
        Os::Windows => 0.78,
        Os::Macos => 0.0,
        Os::Linux => 0.45,
    };
    if rng.random() < if no_mic { p_cam * 0.3 } else { p_cam } {
        let pool = rows(&data["cameras"])
            .iter()
            .filter(|item| item["mic"].as_str().is_none_or(str::is_empty))
            .collect::<Vec<_>>();
        if !pool.is_empty() {
            let camera = weighted(&mut rng, &pool)["cam"]
                .as_str()
                .expect("Compiled camera label is text");
            let group = media.group();
            media.cameras.push((camera.into(), group));
        }
    }
    for (chance, field) in [("p_headset", "headsets"), ("p_display", "displays")] {
        if rng.random() < probability(data, chance, 0.0) {
            let item = weighted(&mut rng, &rows(&data[field]).iter().collect::<Vec<_>>());
            let group = media.group();
            media.add(item, &group, false);
        }
    }
    if rng.random() < probability(data, "p_extra_camera", 0.0) {
        let mut pool = rows(&data["cameras"])
            .iter()
            .filter(|item| item["mic"].as_str().is_some_and(|name| !name.is_empty()))
            .collect::<Vec<_>>();
        if pool.is_empty() {
            pool.extend(rows(&data["cameras"]));
        }
        let camera = weighted(&mut rng, &pool);
        let group = media.group();
        media.cameras.push((
            camera["cam"]
                .as_str()
                .expect("Compiled camera label is text")
                .into(),
            group.clone(),
        ));
        if let Some(microphone) = camera["mic"].as_str().filter(|name| !name.is_empty()) {
            media.microphones.push((microphone.into(), group));
        }
    }
    if data["monitor_sources"] == true {
        media.microphones.extend(
            media
                .speakers
                .iter()
                .map(|(name, group)| (format!("Monitor of {name}"), group.clone())),
        );
    }
    let labels = |items: &[(String, String)]| {
        items
            .iter()
            .map(|(name, _)| name.clone())
            .collect::<Vec<_>>()
    };
    let groups = |items: &[(String, String)]| {
        items
            .iter()
            .map(|(_, group)| group.clone())
            .collect::<Vec<_>>()
    };
    json!({"mediaDevices:enabled":true,"mediaDevices:micros":media.microphones.len(),"mediaDevices:webcams":media.cameras.len(),"mediaDevices:speakers":media.speakers.len(),
        "mediaDevices:microphoneLabels":labels(&media.microphones),"mediaDevices:microphoneGroups":groups(&media.microphones),
        "mediaDevices:webcamLabels":labels(&media.cameras),"mediaDevices:webcamGroups":groups(&media.cameras),
        "mediaDevices:speakerLabels":labels(&media.speakers),"mediaDevices:speakerGroups":groups(&media.speakers)})
}
