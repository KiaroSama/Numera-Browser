//! Bounded observations of retained owners. These diagnostics never replace
//! lifetime peak measurements used by benchmark acceptance.
use super::{Result, fail};
use numera_core::ownership::{
    memory::{JointSnapshot, Row, Snapshot},
    tree::OwnedTree,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
};

pub const MAX_PROCESSES: usize = 512;
pub const MAX_SNAPSHOTS: usize = 1800;
pub const MAX_WINDOWS_INTERVALS: usize = 128;
pub const ROLES: [&str; 3] = ["command", "sdk-leader", "owned-descendant"];
const COUNTERS: [&str; 4] = [
    "rss_bytes",
    "peak_rss_bytes",
    "private_bytes",
    "peak_private_bytes",
];

pub trait Procfs {
    fn entries(&mut self, root: &Path) -> io::Result<Vec<PathBuf>>;
    fn read(&mut self, path: &Path) -> io::Result<Vec<u8>>;
}
pub struct NativeProcfs;
impl Procfs for NativeProcfs {
    fn entries(&mut self, root: &Path) -> io::Result<Vec<PathBuf>> {
        fs::read_dir(root)?
            .take(8193)
            .map(|entry| entry.map(|entry| entry.path()))
            .collect()
    }
    fn read(&mut self, path: &Path) -> io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        fs::File::open(path)?.take(65537).read_to_end(&mut bytes)?;
        if bytes.len() > 65536 {
            return Err(io::Error::other(
                "Memory diagnostic input exceeds its bound",
            ));
        }
        Ok(bytes)
    }
}
pub fn identity(bytes: &[u8]) -> Result<(u32, u32, u64)> {
    let text = std::str::from_utf8(bytes)?;
    let end = text
        .rfind(')')
        .ok_or("Memory diagnostic process identity is invalid")?;
    let pid = text
        .split_once(' ')
        .ok_or("Memory diagnostic PID missing")?
        .0
        .parse::<u32>()?;
    let fields = text
        .get(end + 2..)
        .ok_or("Memory diagnostic fields missing")?
        .split_ascii_whitespace()
        .collect::<Vec<_>>();
    let group = fields
        .get(2)
        .ok_or("Memory diagnostic group missing")?
        .parse::<u32>()?;
    let birth = fields
        .get(19)
        .ok_or("Memory diagnostic birth missing")?
        .parse::<u64>()?;
    if pid == 0 || group == 0 || birth == 0 {
        return fail("Memory diagnostic process identity is invalid");
    }
    Ok((pid, group, birth))
}
fn admitted(io: &mut impl Procfs, root: &Path, groups: &BTreeMap<u32, u64>) -> BTreeSet<u32> {
    groups
        .iter()
        .filter_map(|(&group, &birth)| {
            io.read(&root.join(group.to_string()).join("stat"))
                .ok()
                .and_then(|bytes| identity(&bytes).ok())
                .filter(|value| *value == (group, group, birth))
                .map(|_| group)
        })
        .collect()
}
pub fn linux_snapshot(
    io: &mut impl Procfs,
    root: &Path,
    groups: &BTreeMap<u32, u64>,
    limit: usize,
) -> Result<Snapshot> {
    let active = admitted(io, root, groups);
    let mut result = Snapshot {
        rows: Vec::new(),
        missing: (groups.len() - active.len()) as u64,
    };
    let pattern = regex::Regex::new(r"^(VmRSS|VmHWM):\s+([0-9]{1,16}) kB$")?;
    for (index, entry) in io.entries(root)?.into_iter().enumerate() {
        if index >= 8192 {
            result.missing += 1;
            break;
        }
        let Some(name) = entry.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.is_empty() || !name.bytes().all(|byte| byte.is_ascii_digit()) {
            continue;
        }
        let sample = (|| -> Result<Option<Row>> {
            let before = identity(&io.read(&entry.join("stat"))?)?;
            if before.0.to_string() != name {
                return fail("Memory diagnostic PID differs");
            }
            if !active.contains(&before.1) {
                return Ok(None);
            }
            let mut counters = BTreeMap::new();
            let raw = io.read(&entry.join("status"))?;
            for line in std::str::from_utf8(&raw)?.lines() {
                if line.starts_with("VmRSS:") || line.starts_with("VmHWM:") {
                    let captured = pattern
                        .captures(line)
                        .ok_or("Memory diagnostic counter is invalid")?;
                    let key = if &captured[1] == "VmRSS" {
                        "rss_bytes"
                    } else {
                        "peak_rss_bytes"
                    };
                    let value = captured[2]
                        .parse::<u64>()?
                        .checked_mul(1024)
                        .ok_or("Memory diagnostic counter overflow")?;
                    if counters.insert(key.into(), value).is_some() {
                        return fail("Memory diagnostic counter is duplicate");
                    }
                }
            }
            if counters.len() != 2 || identity(&io.read(&entry.join("stat"))?)? != before {
                return fail("Memory diagnostic evidence is incomplete");
            }
            Ok(Some(Row {
                pid: before.0,
                birth: before.2,
                counters,
            }))
        })();
        match sample {
            Ok(Some(row)) => {
                if result.rows.len() >= limit {
                    result.missing += 1;
                    break;
                }
                result.rows.push(row);
            }
            Ok(None) => (),
            Err(_) => result.missing += 1,
        }
    }
    let after = admitted(io, root, groups);
    if after != active {
        result.missing += active.difference(&after).count() as u64;
        result.rows.clear();
    }
    result.rows.sort_by_key(|row| (row.pid, row.birth));
    Ok(result)
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Owner {
    pub pid: u32,
    pub birth: u64,
    pub group: Option<u32>,
}
impl Owner {
    pub fn from_record(record: &Value) -> Result<Self> {
        let pid = record["pid"]
            .as_u64()
            .and_then(|pid| u32::try_from(pid).ok())
            .filter(|pid| *pid > 0)
            .ok_or("Memory diagnostic owner PID missing")?;
        let raw = record["token"].as_u64();
        let normalized = record["pidToken"].as_u64();
        if raw.is_some() && normalized.is_some() && raw != normalized {
            return fail("Memory diagnostic owner birth identities conflict");
        }
        let birth = raw
            .or(normalized)
            .filter(|birth| *birth > 0)
            .ok_or("Memory diagnostic owner birth missing")?;
        let group = if record["tree"]["kind"] == "pgid" {
            Some(
                record["tree"]["group"]
                    .as_u64()
                    .and_then(|group| u32::try_from(group).ok())
                    .filter(|group| *group > 0)
                    .ok_or("Memory diagnostic owner group missing")?,
            )
        } else {
            None
        };
        Ok(Self { pid, birth, group })
    }
}
pub struct Recorder {
    direct: Owner,
    prior: BTreeSet<Owner>,
    next: f64,
    snapshots: usize,
    missing: u64,
    truncated: bool,
    identities: BTreeMap<&'static str, BTreeSet<(u32, u64)>>,
    maxima: BTreeMap<&'static str, BTreeMap<String, u64>>,
    intervals: Option<Vec<Value>>,
    pub max_snapshots: usize,
    pub max_intervals: usize,
}
impl Recorder {
    pub fn new(direct: Owner, prior: BTreeSet<Owner>, windows: bool) -> Self {
        Self {
            direct,
            prior,
            next: 0.0,
            snapshots: 0,
            missing: 0,
            truncated: false,
            identities: BTreeMap::new(),
            maxima: BTreeMap::new(),
            intervals: windows.then(Vec::new),
            max_snapshots: MAX_SNAPSHOTS,
            max_intervals: MAX_WINDOWS_INTERVALS,
        }
    }
    pub fn sample(
        &mut self,
        now: f64,
        complete: bool,
        owners: &[Owner],
        query: impl FnOnce(&BTreeMap<u32, u64>) -> Result<(Snapshot, Option<JointSnapshot>)>,
    ) -> Result<()> {
        if !now.is_finite() || now < 0.0 {
            return fail("Memory diagnostic clock is invalid");
        }
        if now < self.next {
            return Ok(());
        }
        self.next = now + 0.05;
        if self.snapshots >= self.max_snapshots {
            self.truncated = true;
            return Ok(());
        }
        self.snapshots += 1;
        if !complete {
            self.missing += 1;
            return Ok(());
        }
        let owners = owners
            .iter()
            .filter(|owner| *owner != &self.direct && !self.prior.contains(*owner))
            .collect::<Vec<_>>();
        let leaders = owners
            .iter()
            .map(|owner| (owner.pid, owner.birth))
            .collect::<BTreeSet<_>>();
        let groups = std::iter::once(&self.direct)
            .chain(owners.iter().copied())
            .filter_map(|owner| owner.group.map(|group| (group, owner.birth)))
            .collect();
        let (snapshot, joint) = match query(&groups) {
            Ok(value) => value,
            Err(error) => {
                // A failed explicit handle close is ownership failure, not missing diagnostics.
                if error.to_string().contains("handle did not close") {
                    return Err(error);
                }
                self.missing += 1;
                return Ok(());
            }
        };
        self.missing += snapshot.missing;
        let mut private = BTreeMap::from(ROLES.map(|role| (role, 0u64)));
        let mut members = private.clone();
        let mut missing = snapshot.missing;
        for row in snapshot.rows {
            let identity = (row.pid, row.birth);
            let role = if identity == (self.direct.pid, self.direct.birth) {
                ROLES[0]
            } else if leaders.contains(&identity) {
                ROLES[1]
            } else {
                ROLES[2]
            };
            let count = self.identities.values().map(BTreeSet::len).sum::<usize>();
            let known = self.identities.entry(role).or_default();
            if !known.contains(&identity) && count >= MAX_PROCESSES {
                self.truncated = true;
                missing += 1;
                continue;
            }
            known.insert(identity);
            let maxima = self.maxima.entry(role).or_default();
            for (key, value) in &row.counters {
                let max = maxima.entry(key.clone()).or_default();
                *max = (*max).max(*value);
            }
            if joint.is_some() {
                *private.get_mut(role).ok_or("Unknown memory role")? += row
                    .counters
                    .get("private_bytes")
                    .copied()
                    .ok_or("Memory private counter missing")?;
                *members.get_mut(role).ok_or("Unknown memory role")? += 1;
            }
        }
        if let Some(joint) = joint {
            let interval = json!({"started_ns":joint.started_ns,"finished_ns":joint.finished_ns,"peak_before_bytes":joint.peak_before_bytes,"peak_after_bytes":joint.peak_after_bytes,"membership_changed":joint.membership_changed,"missing":missing,"private_bytes":private,"members":members});
            let intervals = self
                .intervals
                .as_mut()
                .ok_or("Windows diagnostic interval on another platform")?;
            if intervals.len() < self.max_intervals {
                intervals.push(interval);
            } else {
                self.truncated = true;
            }
        }
        Ok(())
    }
    pub fn sample_native(
        &mut self,
        now: f64,
        tree: &OwnedTree,
        receipt: Option<&Path>,
    ) -> Result<()> {
        let (complete, owners) = if let Some(path) = receipt {
            match numera_core::ownership::unit_report::read(path) {
                Ok(report) => {
                    let mut complete = report.complete;
                    let mut owners = Vec::new();
                    for record in &report.trees {
                        match Owner::from_record(record) {
                            Ok(owner) => owners.push(owner),
                            Err(_) => complete = false,
                        }
                    }
                    (complete, owners)
                }
                Err(_) => (false, Vec::new()),
            }
        } else {
            (true, Vec::new())
        };
        #[cfg(target_os = "linux")]
        let query = |groups: &BTreeMap<u32, u64>| {
            linux_snapshot(&mut NativeProcfs, Path::new("/proc"), groups, MAX_PROCESSES)
                .map(|snapshot| (snapshot, None))
        };
        #[cfg(windows)]
        let query = |_groups: &BTreeMap<u32, u64>| -> Result<_> {
            let joint = numera_core::ownership::memory::joint_snapshot(tree)?;
            Ok((joint.snapshot.clone(), Some(joint)))
        };
        #[cfg(not(any(target_os = "linux", windows)))]
        let query = |_groups: &BTreeMap<u32, u64>| -> Result<_> {
            fail("Memory diagnostic platform is unsupported")
        };
        #[cfg(not(windows))]
        let _ = tree;
        self.sample(now, complete, &owners, query)
    }
    pub fn result(&self) -> Value {
        let roles = self
            .maxima
            .iter()
            .map(|(role, max)| {
                (
                    *role,
                    json!({"identities":self.identities[role].len(),"max_observed":max}),
                )
            })
            .collect::<BTreeMap<_, _>>();
        let mut value = json!({"coverage":"sampled-not-lifetime-attribution","snapshots":self.snapshots,"missing":self.missing,"truncated":self.truncated,"roles":roles});
        if let Some(intervals) = &self.intervals {
            value["windows_intervals"] = json!(intervals);
        }
        value
    }
}

fn fields(value: &Value, expected: &[&str]) -> bool {
    value.as_object().is_some_and(|object| {
        object.len() == expected.len() && expected.iter().all(|key| object.contains_key(*key))
    })
}
pub fn project(value: &Value, runtime: &str) -> Result<Vec<String>> {
    let base = ["coverage", "snapshots", "missing", "truncated", "roles"];
    let mut extended = base.to_vec();
    extended.push("windows_intervals");
    if !["original", "native"].contains(&runtime)
        || (!fields(value, &base) && !fields(value, &extended))
        || value["coverage"] != "sampled-not-lifetime-attribution"
        || value["snapshots"].as_u64().is_none()
        || value["missing"].as_u64().is_none()
        || value["truncated"].as_bool().is_none()
    {
        return fail("Memory diagnostic evidence is invalid");
    }
    let roles = value["roles"]
        .as_object()
        .ok_or("Memory diagnostic roles missing")?;
    let mut result = vec![format!(
        "Unmeasured memory coverage=sampled-not-lifetime-attribution runtime={runtime} snapshots={} missing={} truncated={}",
        value["snapshots"], value["missing"], value["truncated"]
    )];
    for (role, row) in roles {
        if !ROLES.contains(&role.as_str())
            || !fields(row, &["identities", "max_observed"])
            || !row["identities"]
                .as_u64()
                .is_some_and(|n| (1..=MAX_PROCESSES as u64).contains(&n))
        {
            return fail("Memory diagnostic role is invalid");
        }
        for (metric, number) in row["max_observed"]
            .as_object()
            .ok_or("Memory diagnostic counters invalid")?
        {
            if !COUNTERS.contains(&metric.as_str()) || number.as_u64().is_none() {
                return fail("Memory diagnostic counters invalid");
            }
            result.push(format!("Unmeasured memory runtime={runtime} role={role} identities={} metric={metric} max_observed={number}",row["identities"]));
        }
    }
    let mut previous = (0, 0);
    if let Some(intervals) = value.get("windows_intervals") {
        let intervals = intervals
            .as_array()
            .filter(|rows| rows.len() <= MAX_WINDOWS_INTERVALS)
            .ok_or("Memory diagnostic interval bound exceeded")?;
        for (index, interval) in intervals.iter().enumerate() {
            let numeric = [
                "started_ns",
                "finished_ns",
                "peak_before_bytes",
                "peak_after_bytes",
                "missing",
            ];
            if !fields(
                interval,
                &[
                    "started_ns",
                    "finished_ns",
                    "peak_before_bytes",
                    "peak_after_bytes",
                    "missing",
                    "membership_changed",
                    "private_bytes",
                    "members",
                ],
            ) || numeric.iter().any(|key| interval[key].as_u64().is_none())
                || interval["membership_changed"].as_bool().is_none()
            {
                return fail("Memory diagnostic interval is invalid");
            }
            let numbers = numeric.map(|key| interval[key].as_u64().expect("checked number"));
            if numbers[0] < previous.0
                || numbers[1] < numbers[0]
                || numbers[2] < previous.1
                || numbers[3] < numbers[2]
            {
                return fail("Memory diagnostic counters decreased");
            }
            previous = (numbers[1], numbers[3]);
            for key in ["private_bytes", "members"] {
                if !fields(&interval[key], &ROLES)
                    || ROLES
                        .iter()
                        .any(|role| interval[key][role].as_u64().is_none())
                {
                    return fail("Memory diagnostic vector is invalid");
                }
            }
            let total = ROLES
                .iter()
                .try_fold(0u64, |sum, role| {
                    sum.checked_add(interval["members"][role].as_u64().expect("checked member"))
                })
                .ok_or("Memory diagnostic member overflow")?;
            if total > MAX_PROCESSES as u64 {
                return fail("Memory diagnostic members exceed bound");
            }
            result.push(format!("Unmeasured memory coverage=non-atomic-current-private runtime={runtime} interval={index} started_ns={} finished_ns={} peak_before_bytes={} peak_after_bytes={} membership_changed={} missing={} command_private_bytes={} sdk_leader_private_bytes={} owned_descendant_private_bytes={} command_members={} sdk_leader_members={} owned_descendant_members={}",numbers[0],numbers[1],numbers[2],numbers[3],interval["membership_changed"],numbers[4],interval["private_bytes"][ROLES[0]],interval["private_bytes"][ROLES[1]],interval["private_bytes"][ROLES[2]],interval["members"][ROLES[0]],interval["members"][ROLES[1]],interval["members"][ROLES[2]]));
        }
    }
    Ok(result)
}
