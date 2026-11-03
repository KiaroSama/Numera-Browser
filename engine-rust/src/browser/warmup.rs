use super::{adapter::Adapter, probes};
use crate::python_random::PythonRandom;
use serde_json::{Map, Value, json};
use std::{
    io,
    time::{Duration, Instant},
};

pub const HOST: &str = "() => location.host";
pub const LINKS: &str = "(host) => [...document.querySelectorAll('a[href]')]\n                 .map(a => a.href)\n                 .filter(h => h.startsWith('http') && new URL(h).host === host)\n                 .slice(0, 40)";

/// Time and entropy are injectable only at the native control boundary; the SDK
/// never decides destinations, actions, budgets or application outcomes.
pub trait Control {
    fn elapsed(&self) -> f64;
    fn pause(&mut self, seconds: f64) -> io::Result<()>;
    fn random(&mut self) -> f64;
    fn below(&mut self, count: usize) -> usize;
    fn cancelled(&mut self) -> bool;
}
pub struct Live<C> {
    started: Instant,
    rng: PythonRandom,
    cancelled: C,
}
impl<C: FnMut() -> bool> Live<C> {
    pub fn new(cancelled: C) -> io::Result<Self> {
        let mut seed = [0; 8];
        getrandom::fill(&mut seed).map_err(|error| io::Error::other(error.to_string()))?;
        Ok(Self {
            started: Instant::now(),
            rng: PythonRandom::new(i64::from_le_bytes(seed)),
            cancelled,
        })
    }
}
impl<C: FnMut() -> bool> Control for Live<C> {
    fn elapsed(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }
    fn random(&mut self) -> f64 {
        self.rng.random()
    }
    fn below(&mut self, count: usize) -> usize {
        self.rng.below(count)
    }
    fn cancelled(&mut self) -> bool {
        (self.cancelled)()
    }
    fn pause(&mut self, seconds: f64) -> io::Result<()> {
        let end = Instant::now() + Duration::from_secs_f64(seconds);
        while Instant::now() < end && !self.cancelled() {
            std::thread::sleep(
                end.saturating_duration_since(Instant::now())
                    .min(Duration::from_millis(20)),
            );
        }
        Ok(())
    }
}
fn uniform(control: &mut impl Control, low: f64, high: f64) -> f64 {
    low + (high - low) * control.random()
}
fn integer(control: &mut impl Control, low: usize, high: usize) -> usize {
    low + control.below(high - low + 1)
}
fn navigate(adapter: &mut Adapter, url: &str, timeout: u32) -> io::Result<bool> {
    let result = adapter.call(
        "navigate",
        Map::from_iter([
            ("url".into(), url.into()),
            ("timeout_ms".into(), timeout.into()),
            ("new_page".into(), false.into()),
            ("tolerate_failure".into(), true.into()),
        ]),
        Duration::from_millis(u64::from(timeout) + 5000),
    )?;
    result["navigated"]
        .as_bool()
        .ok_or_else(|| io::Error::other("Warmup navigation result missing"))
}
fn browse(adapter: &mut Adapter, control: &mut impl Control, seconds: f64) -> io::Result<()> {
    let end = control.elapsed() + seconds;
    while control.elapsed() < end && !control.cancelled() {
        let action = control.random();
        let fields = if action < 0.6 {
            json!({"action":"wheel","x":0,"y":integer(control,200,900)})
        } else if action < 0.85 {
            json!({"action":"move","x":integer(control,50,900),"y":integer(control,80,600),"steps":integer(control,5,20)})
        } else {
            json!({"action":"wheel","x":0,"y":-(integer(control,100,500) as i32)})
        };
        let observed = adapter.call(
            "mouse",
            fields
                .as_object()
                .cloned()
                .ok_or_else(|| io::Error::other("Mouse request missing"))?,
            Duration::from_secs(5),
        )?;
        if !observed["performed"]
            .as_bool()
            .ok_or_else(|| io::Error::other("Mouse result missing"))?
        {
            return Ok(());
        }
        let pause = uniform(control, 0.8, 3.5);
        control.pause(pause)?;
    }
    Ok(())
}
fn follow(adapter: &mut Adapter, control: &mut impl Control) -> io::Result<bool> {
    let Some(host) = probes::observation(adapter, HOST, Value::Null)? else {
        return Ok(false);
    };
    let Some(links) = probes::observation(adapter, LINKS, host)? else {
        return Ok(false);
    };
    let Some(links) = links.as_array().filter(|values| !values.is_empty()) else {
        return Ok(false);
    };
    let url = links[control.below(links.len())]
        .as_str()
        .ok_or_else(|| io::Error::other("Observed link is not text"))?;
    navigate(adapter, url, 30000)
}
pub fn run(
    adapter: &mut Adapter,
    minutes: f64,
    mut urls: Vec<String>,
    preset: &str,
    started: f64,
    control: &mut impl Control,
    mut event: impl FnMut(&str) -> io::Result<()>,
) -> io::Result<Value> {
    if !minutes.is_finite()
        || !(0.0..=480.0).contains(&minutes)
        || minutes == 0.0
        || urls.is_empty()
        || urls.len() > u32::MAX as usize
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Warmup duration or destinations are invalid",
        ));
    }
    for index in (1..urls.len()).rev() {
        let chosen = control.below(index + 1);
        urls.swap(index, chosen);
    }
    let end = started + minutes * 60.0;
    let mut visited = Vec::new();
    let mut errors = 0u64;
    let mut index = 0;
    while control.elapsed() < end && !control.cancelled() {
        let url = &urls[index % urls.len()];
        index += 1;
        if !navigate(adapter, url, 45000)? {
            errors += 1;
            event(&format!("  skipped {url} (unreachable)"))?;
            continue;
        }
        visited.push(url.clone());
        event(&format!("  browsing {url}"))?;
        let budget = uniform(control, 20.0, 60.0).min((end - control.elapsed()).max(5.0));
        browse(adapter, control, budget * 0.6)?;
        if control.elapsed() < end
            && !control.cancelled()
            && control.random() < 0.7
            && follow(adapter, control)?
        {
            event("    followed a link")?;
            browse(adapter, control, budget * 0.4)?;
        }
        let pause = uniform(control, 1.0, 3.0);
        control.pause(pause)?;
    }
    Ok(
        json!({"visited":visited,"pages":visited.len(),"errors":errors,
        "minutes":format!("{minutes:.2}").parse::<f64>().map_err(io::Error::other)?,"preset":preset}),
    )
}
