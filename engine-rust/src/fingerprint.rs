use crate::{devices::catalog, models::Fingerprint, python_random::PythonRandom};
use serde_json::{Value, json};
use std::{error::Error, fmt};

#[derive(Debug)]
pub struct GenerationError(&'static str);
impl fmt::Display for GenerationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}
impl Error for GenerationError {}

#[derive(Default)]
pub struct GenerateOptions<'a, S = i64> {
    pub seed: S,
    pub os: Option<&'a str>,
    pub locale: Option<&'a str>,
    pub country: Option<&'a str>,
    pub mobile: bool,
}

fn pool(data: &Value) -> Result<&[Value], GenerationError> {
    data.as_array()
        .map(Vec::as_slice)
        .ok_or(GenerationError("Device catalog pool is absent"))
}
fn text(value: &Value) -> Result<&str, GenerationError> {
    value
        .as_str()
        .ok_or(GenerationError("Device catalog text is invalid"))
}
fn pick(rng: &mut PythonRandom, values: &Value, weights: &Value) -> Result<Value, GenerationError> {
    let values = pool(values)?;
    if values.is_empty() {
        return Err(GenerationError("Device catalog pool is empty"));
    }
    let index = if let Some(weights) = weights
        .as_array()
        .filter(|weights| !weights.is_empty() && weights.len() == values.len())
    {
        let numbers: Result<Vec<u64>, _> = weights
            .iter()
            .map(|w| {
                w.as_u64()
                    .ok_or(GenerationError("Device catalog weight is invalid"))
            })
            .collect();
        let numbers = numbers?;
        if numbers.iter().sum::<u64>() == 0 {
            return Err(GenerationError("Device catalog weight is zero"));
        }
        rng.weighted(&numbers)
    } else {
        rng.below(values.len())
    };
    Ok(values[index].clone())
}

pub fn user_agent(
    os: &str,
    chrome: &str,
    version: &str,
    model: &str,
) -> Result<String, GenerationError> {
    let data =
        &catalog().map_err(|_| GenerationError("Cannot parse embedded device catalog"))?["catalog"];
    let model = if model.is_empty() {
        text(&data["ANDROID_MODELS"][0])?
    } else {
        model
    };
    let token = text(&data["OS_UA_TOKEN"][os])?
        .replace("{v}", if version.is_empty() { "14" } else { version })
        .replace(
            "{v_}",
            &if version.is_empty() {
                "18.5".to_owned()
            } else {
                version.to_owned()
            }
            .replace('.', "_"),
        )
        .replace("{model}", model);
    Ok(if os == "ios" {
        format!(
            "Mozilla/5.0 ({token}) AppleWebKit/605.1.15 (KHTML, like Gecko) CriOS/{chrome} Mobile/15E148 Safari/604.1"
        )
    } else {
        format!(
            "Mozilla/5.0 ({token}) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/{chrome}{} Safari/537.36",
            if os == "android" { " Mobile" } else { "" }
        )
    })
}

/// Explicit seed is required at this pure seam; entropy belongs at the CLI boundary.
pub fn generate<S: Into<serde_json::Number>>(
    options: GenerateOptions<'_, S>,
) -> Result<Fingerprint, GenerationError> {
    let seed = options.seed.into();
    let root = catalog().map_err(|_| GenerationError("Cannot parse embedded device catalog"))?;
    let d = &root["catalog"];
    let w = &d["WEIGHTS"];
    let mut rng = PythonRandom::from_number(&seed)
        .ok_or(GenerationError("Fingerprint seed must be an integer"))?;
    let os = match options.os.filter(|v| !v.is_empty()) {
        Some(os) => os.to_lowercase(),
        None if options.mobile => {
            text(&pick(&mut rng, &d["MOBILE_OS"], &json!([70, 30]))?)?.to_owned()
        }
        None => text(&pick(&mut rng, &d["DESKTOP_OS"], &json!([72, 21, 7]))?)?.to_owned(),
    };
    if !pool(&d["ALL_OS"])?.iter().any(|v| v == &os) {
        return Err(GenerationError("Unknown operating system"));
    }
    let mobile = os == "android" || os == "ios";
    let chrome = pick(&mut rng, &d["CHROME_VERSIONS"], &w["chrome"])?;
    let release = pick(&mut rng, &d["OS_VERSIONS"][&os], &w["os_versions"][&os])?;
    let version = text(&release[0])?;
    let model = if os == "android" {
        text(&pick(&mut rng, &d["ANDROID_MODELS"], &w["android_models"])?)?.to_owned()
    } else {
        String::new()
    };
    let screen = pick(&mut rng, &d["SCREENS"][&os], &w["screens"][&os])?;
    let sw = screen[0]
        .as_i64()
        .ok_or(GenerationError("Screen width is invalid"))?;
    let sh = screen[1]
        .as_i64()
        .ok_or(GenerationError("Screen height is invalid"))?;
    let (vw, vh) = if mobile {
        let margins: &[i64] = if os == "ios" {
            &[113, 140]
        } else {
            &[0, 80, 120]
        };
        (sw, sh - margins[rng.below(margins.len())])
    } else {
        (
            sw - [0, 16][rng.below(2)],
            sh - [74, 88, 104, 120][rng.below(4)],
        )
    };
    let cores = pick(&mut rng, &d["HARDWARE"][&os]["cores"], &w["cores"][&os])?;
    let memory = pick(&mut rng, &d["HARDWARE"][&os]["memory"], &w["memory"][&os])?;
    let touch = pick(&mut rng, &d["TOUCH_POINTS"][&os], &w["touch"][&os])?;
    let dpr = pick(&mut rng, &d["PIXEL_RATIOS"][&os], &w["pixel_ratios"][&os])?;
    let gpu = pick(&mut rng, &d["WEBGL"][&os], &w["webgl"][&os])?;
    let locales = d["LOCALES"]
        .as_object()
        .ok_or(GenerationError("Locale catalog is invalid"))?;
    let locale = if let Some(value) = options.locale.filter(|value| locales.contains_key(*value)) {
        value.to_owned()
    } else if let Some(value) = options
        .country
        .and_then(|c| d["COUNTRY_TO_LOCALE"][c.to_uppercase()].as_str())
    {
        value.to_owned()
    } else {
        locales
            .keys()
            .nth(rng.below(locales.len()))
            .ok_or(GenerationError("Locale catalog is empty"))?
            .clone()
    };
    let font_pool = pool(&d["FONTS"][&os])?;
    let mut fonts: Vec<String> = if mobile {
        font_pool
            .iter()
            .map(|v| text(v).map(str::to_owned))
            .collect::<Result<_, _>>()?
    } else {
        let count = 6.max(font_pool.len() - rng.below(7));
        rng.sample_indices(font_pool.len(), count)
            .iter()
            .map(|&i| text(&font_pool[i]).map(str::to_owned))
            .collect::<Result<_, _>>()?
    };
    fonts.sort();
    let (cameras, microphones, speakers) = if mobile {
        (2, 1, 1)
    } else {
        let cameras = [0, 1, 1, 1, 2][rng.below(5)];
        let microphones = if cameras != 0 {
            [0, 1, 1, 1, 2][rng.below(5)]
        } else {
            [0, 1][rng.below(2)]
        };
        let speakers = if microphones != 0 {
            [1, 1, 2][rng.below(3)]
        } else {
            [0, 1][rng.below(2)]
        };
        (cameras, microphones, speakers)
    };
    let ua = user_agent(&os, text(&chrome)?, version, &model)?;
    Fingerprint::from_record(&json!({"os":os,"user_agent":ua,"platform":d["NAV_PLATFORM"][&os],"ch_platform":d["CH_PLATFORM"][&os],
        "chrome_version":chrome,"screen_width":sw,"screen_height":sh,"viewport_width":vw,"viewport_height":vh,
        "hardware_concurrency":cores,"device_memory":memory,"webgl_vendor":gpu[0],"webgl_renderer":gpu[1],
        "locale":locale,"timezone":locales[&locale][0],"languages":locales[&locale][1],"is_mobile":mobile,"seed":seed,
        "fonts":fonts,"cameras":cameras,"microphones":microphones,"speakers":speakers,"os_version":version,"device_model":model,
        "max_touch_points":touch,"device_pixel_ratio":dpr})).map_err(|_| GenerationError("Generated identity is invalid"))
}
