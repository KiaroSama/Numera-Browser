// SPDX-License-Identifier: MPL-2.0
// Native port of Camoufox's conditioned Firefox WebGL draws.
// Source: daijro/camoufox, 079a6ce8249bab98063e5ac9241e3879a3e6578d.
use super::{
    camoufox_coherence::{
        gpu_fits_machine, gpu_fits_os, gpu_screen_is_plausible, is_software_renderer,
    },
    camoufox_cpu::{self, Cpu},
    camoufox_identity::Os,
    fingerprint::Model,
};
use crate::python_random::PythonRandom;
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    io,
};
fn bad() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "No compatible recorded Firefox WebGL device",
    )
}
fn choose(rng: &mut PythonRandom, rows: &[(u32, f64)]) -> io::Result<u32> {
    let total = rows.iter().map(|(_, p)| *p).sum::<f64>();
    if !total.is_finite() || total <= 0.0 {
        return Err(bad());
    }
    let anchor = rng.random() * total;
    let mut cumulative = 0.0;
    for (id, p) in rows {
        cumulative += p;
        if anchor < cumulative {
            return Ok(*id);
        }
    }
    rows.last().map(|(id, _)| *id).ok_or_else(bad)
}
fn trace(
    model: &Model,
    node: usize,
    evidence: &BTreeMap<usize, BTreeSet<u32>>,
) -> io::Result<Vec<(u32, f64)>> {
    let mut rows = model.trace(node, evidence)?;
    // fpgen.trace sorts stably by probability before random.choices sees it.
    rows.sort_by(|a, b| b.1.total_cmp(&a.1));
    Ok(rows)
}
fn base(model: &Model, os: Os) -> io::Result<BTreeMap<usize, BTreeSet<u32>>> {
    model.evidence(&BTreeMap::from([
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
    ]))
}
pub fn firefox_gpus(model: &Model, os: Os) -> io::Result<Vec<(String, String)>> {
    let rows = trace(model, model.node_index("gpu")?, &base(model, os)?)?;
    let values = model.values(rows.iter().map(|(id, _)| *id))?;
    rows.into_iter()
        .map(|(id, _)| {
            let gpu = values.get(&id).ok_or_else(bad)?;
            Ok((
                gpu["vendor"].as_str().ok_or_else(bad)?.into(),
                gpu["renderer"].as_str().ok_or_else(bad)?.into(),
            ))
        })
        .collect()
}
#[allow(clippy::too_many_arguments)]
pub fn sample(
    model: &Model,
    os: Os,
    width: Option<i64>,
    height: Option<i64>,
    cores: Option<i64>,
    seed: u32,
    ff_major: u32,
    host_gpu: Option<(&str, &str)>,
) -> io::Result<Value> {
    sample_for_cpu(
        model, os, width, height, cores, seed, ff_major, host_gpu, None,
    )
}
#[allow(clippy::too_many_arguments)]
pub fn sample_for_cpu(
    model: &Model,
    os: Os,
    width: Option<i64>,
    height: Option<i64>,
    cores: Option<i64>,
    seed: u32,
    ff_major: u32,
    host_gpu: Option<(&str, &str)>,
    cpu: Option<&Cpu>,
) -> io::Result<Value> {
    let mut evidence = base(model, os)?;
    let gpu_node = model.node_index("gpu")?;
    let mut candidates = trace(model, gpu_node, &evidence)?;
    let values = model.values(candidates.iter().map(|(id, _)| *id))?;
    let mut rng = PythonRandom::new(i64::from(seed));
    let gpu = if let Some((vendor, renderer)) = host_gpu {
        if cpu.is_some_and(|cpu| !camoufox_cpu::gpu_fits(Some(renderer), os, cpu)) {
            return Err(bad());
        }
        candidates
            .iter()
            .find(|(id, _)| {
                values
                    .get(id)
                    .is_some_and(|v| v["vendor"] == vendor && v["renderer"] == renderer)
            })
            .map(|(id, _)| *id)
            .ok_or_else(bad)?
    } else {
        candidates.retain(|(id, _)| {
            let Some(renderer) = values.get(id).and_then(|v| v["renderer"].as_str()) else {
                return false;
            };
            !is_software_renderer(Some(renderer))
                && renderer != "Mozilla"
                && gpu_fits_os(Some(renderer), os)
                && gpu_fits_machine(Some(renderer), os, cores, width, height)
                && gpu_screen_is_plausible(Some(renderer), width, height)
                && cpu.is_none_or(|cpu| camoufox_cpu::gpu_fits(Some(renderer), os, cpu))
        });
        choose(&mut rng, &candidates)?
    };
    evidence.insert(gpu_node, BTreeSet::from([gpu]));
    let webgl_node = model.node_index("webgl")?;
    let webgl = choose(&mut rng, &trace(model, webgl_node, &evidence)?)?;
    evidence.insert(webgl_node, BTreeSet::from([webgl]));
    let webgl2 = choose(
        &mut rng,
        &trace(model, model.node_index("webgl2")?, &evidence)?,
    )?;
    let values = model.values([webgl, webgl2])?;
    to_config(
        values.get(&webgl).ok_or_else(bad)?,
        values.get(&webgl2).ok_or_else(bad)?,
        os,
        ff_major,
    )
}
fn extensions(webgl: &Value) -> io::Result<Vec<String>> {
    webgl["supportedExtensions"]
        .as_array()
        .ok_or_else(bad)?
        .iter()
        .map(|v| v.as_str().map(str::to_owned).ok_or_else(bad))
        .collect()
}
fn context(
    prefix: &str,
    webgl: &Value,
    os: Os,
    backend: &[&str],
    major: u32,
) -> io::Result<Map<String, Value>> {
    let mut extensions = extensions(webgl)?;
    extensions.retain(|v| {
        ![
            "WEBGL_multi_draw",
            "WEBGL_clip_cull_distance",
            "EXT_texture_norm16",
            "WEBGL_compressed_texture_etc1",
        ]
        .contains(&v.as_str())
            && (os == Os::Windows || v != "OVR_multiview2")
            && (major >= 158 || v != "EXT_polygon_offset_clamp")
            && (major >= 159
                || !["EXT_clip_control", "EXT_texture_mirror_clamp_to_edge"].contains(&v.as_str()))
    });
    for extension in backend {
        if !extensions.iter().any(|v| v == extension) {
            let at = extensions
                .iter()
                .position(|v| v.to_lowercase() > extension.to_lowercase())
                .unwrap_or(extensions.len());
            extensions.insert(at, (*extension).into());
        }
    }
    let parameters = webgl["params"]
        .as_object()
        .ok_or_else(bad)?
        .iter()
        .map(|(k, v)| Ok((k.clone(), v.get("value").ok_or_else(bad)?.clone())))
        .collect::<io::Result<Map<String, Value>>>()?;
    let precision = webgl["shaderPrecisionFormats"]
        .as_array()
        .ok_or_else(bad)?
        .iter()
        .map(|v| {
            let shader = v.get("shaderType").ok_or_else(bad)?;
            let kind = v.get("precisionType").ok_or_else(bad)?;
            Ok((
                format!("{shader},{kind}"),
                v.get("shaderPrecisionFormat").ok_or_else(bad)?.clone(),
            ))
        })
        .collect::<io::Result<Map<String, Value>>>()?;
    Ok(Map::from_iter([
        (format!("{prefix}:supportedExtensions"), json!(extensions)),
        (format!("{prefix}:parameters"), Value::Object(parameters)),
        (
            format!("{prefix}:shaderPrecisionFormats"),
            Value::Object(precision),
        ),
    ]))
}
pub fn to_config(webgl: &Value, webgl2: &Value, os: Os, ff_major: u32) -> io::Result<Value> {
    let enabled = webgl2.as_object().is_some_and(|v| !v.is_empty());
    if !enabled && webgl2.as_array().is_none_or(|v| !v.is_empty()) {
        return Err(bad());
    }
    let desktop_gl = os == Os::Linux && extensions(webgl)?.iter().any(|v| v == "EXT_depth_clamp");
    let mut backend = Vec::new();
    if ff_major >= 156 && os == Os::Windows {
        backend.push("EXT_depth_clamp");
    }
    if (os == Os::Windows && enabled) || desktop_gl {
        if ff_major >= 158 {
            backend.push("EXT_polygon_offset_clamp");
        }
        if ff_major >= 159 {
            backend.push("EXT_clip_control");
        }
    }
    if ff_major >= 159 && (os == Os::Windows || desktop_gl) {
        backend.push("EXT_texture_mirror_clamp_to_edge");
    }
    let mut config = Map::new();
    config.insert(
        "webGl:vendor".into(),
        webgl
            .get("vendor")
            .filter(|v| v.is_string())
            .ok_or_else(bad)?
            .clone(),
    );
    config.insert(
        "webGl:renderer".into(),
        webgl
            .get("renderer")
            .filter(|v| v.is_string())
            .ok_or_else(bad)?
            .clone(),
    );
    config.extend(context("webGl", webgl, os, &backend, ff_major)?);
    config.insert("webGl2Enabled".into(), json!(enabled));
    if enabled {
        if ff_major >= 159 {
            backend.push("EXT_texture_norm16");
        }
        config.extend(context("webGl2", webgl2, os, &backend, ff_major)?);
        if ff_major >= 156 {
            let shared = config["webGl2:parameters"]
                .as_object()
                .ok_or_else(bad)?
                .clone();
            let first = config
                .get_mut("webGl:parameters")
                .and_then(Value::as_object_mut)
                .ok_or_else(bad)?;
            for (key, value) in shared {
                if first.contains_key(&key) && !value.is_string() {
                    first.insert(key, value);
                }
            }
        }
        if ff_major >= 159 {
            let parameters = config
                .get_mut("webGl2:parameters")
                .and_then(Value::as_object_mut)
                .ok_or_else(bad)?;
            if parameters
                .get("35376")
                .and_then(Value::as_f64)
                .is_some_and(|n| n > 65536.0)
            {
                parameters.insert("35376".into(), json!(65536));
            }
        }
    }
    Ok(Value::Object(config))
}
