use crate::models::Profile;
use serde_json::{Value, json};
use std::collections::HashMap;

fn signals(profile: &Profile) -> Value {
    let fp = &profile.fingerprint;
    json!({"os":fp.os,"user_agent":fp.user_agent,"platform":fp.platform,
        "webgl_vendor":fp.webgl_vendor,"webgl_renderer":fp.webgl_renderer,
        "screen_width":fp.screen_width,"screen_height":fp.screen_height,
        "hardware_concurrency":fp.hardware_concurrency,"device_memory":fp.device_memory})
}
pub fn find(profiles: &[Profile]) -> Vec<Value> {
    let mut indices = HashMap::new();
    let mut buckets: Vec<(Value, Vec<&Profile>)> = Vec::new();
    for profile in profiles {
        let mut signature = signals(profile);
        // Python numeric equality groups integer and integral float memory alike.
        if let Some(memory) = signature["device_memory"].as_f64().filter(|value| {
            value.is_finite() && value.fract() == 0.0 && value.abs() < 9_223_372_036_854_775_808.0
        }) {
            signature["device_memory"] = (memory as i64).into();
        }
        let key = signature.to_string();
        let index = *indices.entry(key).or_insert_with(|| {
            buckets.push((signals(profile), Vec::new()));
            buckets.len() - 1
        });
        buckets[index].1.push(profile);
    }
    let mut groups = buckets.into_iter().filter(|(_, members)| members.len() > 1).map(|(signals, members)| {
        let identical = members.iter().all(|profile| profile.fingerprint.seed == members[0].fingerprint.seed);
        json!({"members":members.iter().map(|profile| json!({"id":profile.id,"name":profile.name,"seed":profile.fingerprint.seed})).collect::<Vec<_>>(),
            "identical":identical,"signals":signals})
    }).collect::<Vec<_>>();
    groups.sort_by_key(|group| std::cmp::Reverse(group["members"].as_array().map_or(0, Vec::len)));
    groups
}

pub fn summary(profiles: &[Profile]) -> Value {
    let groups = find(profiles);
    let clashing: usize = groups
        .iter()
        .map(|group| group["members"].as_array().map_or(0, Vec::len))
        .sum();
    let identical = groups
        .iter()
        .filter(|group| group["identical"] == true)
        .count();
    json!({"total": profiles.len(), "groups": groups.len(), "clashing": clashing, "identical": identical})
}
