// SPDX-License-Identifier: Apache-2.0
// Native port of fpgen's pinned Bayesian model sampling algorithm.
// Original: daijro and Scrapfly, https://github.com/scrapfly/fingerprint-generator
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};
use std::{
    cmp::{Ordering, Reverse},
    collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, HashSet},
    fs::File,
    io::{self, Read},
    path::Path,
};

const ARCHIVE_SHA: &str = "6530b8322cdaa4ec042921c8d9a0369a0e6e0269ba636c01a7203e4a2f109936";
const DATA_SHA: &str = "9ce7d16630e91654e73988f7d37e8c99987c09b9c779a7bb9dc7ad6355dcc2f0";
const LIMIT: usize = 256 * 1024 * 1024;
const EMPTY: u32 = u32::MAX;
const BEAM: usize = 1000;
fn bad() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "Invalid pinned fingerprint model",
    )
}
fn hex(bytes: impl AsRef<[u8]>) -> String {
    bytes
        .as_ref()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
fn code(text: &str) -> io::Result<u32> {
    const ALPHABET: &[u8] =
        b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz!#$%&()*+-;<=>?@^_`{|}~";
    if text.is_empty() || text.len() > 5 {
        return Err(bad());
    }
    let mut value = 0u64;
    for i in 0..5 {
        let digit = if i < text.len() {
            ALPHABET
                .iter()
                .position(|byte| *byte == text.as_bytes()[i])
                .ok_or_else(bad)? as u64
        } else {
            84
        };
        value = value
            .checked_mul(85)
            .and_then(|v| v.checked_add(digit))
            .ok_or_else(bad)?;
    }
    let value = u32::try_from(value).map_err(|_| bad())?;
    Ok(if text.len() == 1 {
        0
    } else {
        value >> ((5 - text.len()) * 8)
    })
}
fn strings(value: &Value) -> io::Result<Vec<String>> {
    value
        .as_array()
        .ok_or_else(bad)?
        .iter()
        .map(|v| v.as_str().map(str::to_owned).ok_or_else(bad))
        .collect()
}
struct Node {
    name: String,
    parents: Vec<usize>,
    possible: Vec<u32>,
    ancestors: HashSet<usize>,
    tables: HashMap<Vec<u32>, Vec<(u32, f64)>>,
}
fn tables(
    value: &Value,
    depth: usize,
    prefix: &mut Vec<u32>,
    out: &mut HashMap<Vec<u32>, Vec<(u32, f64)>>,
) -> io::Result<()> {
    let object = value.as_object().ok_or_else(bad)?;
    if depth == 0 {
        let rows = object
            .iter()
            .map(|(key, value)| {
                let p = value
                    .as_f64()
                    .filter(|p| p.is_finite() && *p >= 0.0)
                    .ok_or_else(bad)?;
                Ok((code(key)?, p))
            })
            .collect::<io::Result<Vec<_>>>()?;
        out.insert(prefix.clone(), rows);
    } else {
        for (key, value) in object {
            prefix.push(code(key)?);
            tables(value, depth - 1, prefix, out)?;
            prefix.pop();
        }
    }
    Ok(())
}
pub struct Model {
    nodes: Vec<Node>,
    positions: Vec<(u64, usize)>,
    compressed: Vec<u8>,
}
impl Model {
    pub fn open(path: &Path) -> io::Result<Self> {
        let mut bytes = Vec::new();
        File::open(path)?.take(1_564_572).read_to_end(&mut bytes)?;
        Self::from_bytes(bytes)
    }
    /// The admitted model is application data, provisioned at build time. A
    /// launch never needs to download a model or write into a user's home.
    pub fn embedded() -> io::Result<Self> {
        Self::from_bytes(include_bytes!("fingerprint-data/model-2-2026.zip").to_vec())
    }
    fn from_bytes(bytes: Vec<u8>) -> io::Result<Self> {
        if bytes.len() != 1_564_571 || hex(Sha256::digest(&bytes)) != ARCHIVE_SHA {
            return Err(bad());
        }
        let mut zip = zip::ZipArchive::new(io::Cursor::new(bytes)).map_err(|_| bad())?;
        if zip.len() != 3 {
            return Err(bad());
        }
        let mut members = HashMap::new();
        for (name, digest) in [
            (
                "fingerprint-network.json.zst",
                "e1b0a7e60837c347f4b7d5dad4a20c356d521e0593e1bf4a8be39ea1e6a41ac4",
            ),
            (
                "values.json.zst",
                "294decde5b6a1a52ed53d50a894130fa5c58f7cc804b78198f5c33f55b3ba66f",
            ),
            (
                "values.dat.zst",
                "3da2cf0891a4a85ef6458f0fbdf9346acfcd04e5fd279a0ea22d7919e4eaf122",
            ),
        ] {
            let mut data = Vec::new();
            zip.by_name(name)
                .map_err(|_| bad())?
                .take(2_000_000)
                .read_to_end(&mut data)?;
            if hex(Sha256::digest(&data)) != digest {
                return Err(bad());
            }
            members.insert(name, data);
        }
        let decode = |name: &str| -> io::Result<Value> {
            let compressed = members.get(name).ok_or_else(bad)?;
            let decoder = ruzstd::decoding::StreamingDecoder::new(compressed.as_slice())
                .map_err(|_| bad())?;
            let mut data = Vec::new();
            decoder.take(4_000_001).read_to_end(&mut data)?;
            if data.len() > 4_000_000 {
                return Err(bad());
            }
            serde_json::from_slice(&data).map_err(|_| bad())
        };
        let index = decode("values.json.zst")?;
        let positions = index
            .as_object()
            .ok_or_else(bad)?
            .iter()
            .map(|(offset, length)| {
                let offset = u64::from_str_radix(offset, 16).map_err(|_| bad())?;
                let length = length
                    .as_u64()
                    .and_then(|v| usize::try_from(v).ok())
                    .filter(|v| *v <= 1_048_576)
                    .ok_or_else(bad)?;
                if offset
                    .checked_add(length as u64)
                    .is_none_or(|end| end > 211_540_141)
                {
                    return Err(bad());
                }
                Ok((offset, length))
            })
            .collect::<io::Result<Vec<_>>>()?;
        let network = decode("fingerprint-network.json.zst")?;
        let definitions = network["nodes"]
            .as_array()
            .filter(|nodes| nodes.len() == 120)
            .ok_or_else(bad)?;
        let mut nodes: Vec<Node> = Vec::new();
        let mut names = HashMap::new();
        for definition in definitions {
            let name = definition["name"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or_else(bad)?
                .to_owned();
            if names.contains_key(&name) {
                return Err(bad());
            }
            let parents = strings(&definition["parentNames"])?
                .into_iter()
                .map(|name| names.get(&name).copied().ok_or_else(bad))
                .collect::<io::Result<Vec<usize>>>()?;
            let possible = strings(&definition["possibleValues"])?
                .iter()
                .map(|value| code(value))
                .collect::<io::Result<Vec<_>>>()?;
            if possible.is_empty() || possible.iter().any(|id| *id as usize >= positions.len()) {
                return Err(bad());
            }
            let mut ancestors = HashSet::new();
            for parent in &parents {
                ancestors.insert(*parent);
                ancestors.extend(nodes[*parent].ancestors.iter().copied());
            }
            let mut probabilities = HashMap::new();
            tables(
                &definition["conditionalProbabilities"],
                parents.len(),
                &mut Vec::new(),
                &mut probabilities,
            )?;
            names.insert(name.clone(), nodes.len());
            nodes.push(Node {
                name,
                parents,
                possible,
                ancestors,
                tables: probabilities,
            });
        }
        Ok(Self {
            nodes,
            positions,
            compressed: members.remove("values.dat.zst").ok_or_else(bad)?,
        })
    }

    pub(crate) fn values(
        &self,
        ids: impl IntoIterator<Item = u32>,
    ) -> io::Result<HashMap<u32, Value>> {
        let mut wanted = ids
            .into_iter()
            .collect::<HashSet<_>>()
            .into_iter()
            .map(|id| {
                let (offset, length) = *self.positions.get(id as usize).ok_or_else(bad)?;
                Ok((offset, length, id))
            })
            .collect::<io::Result<Vec<_>>>()?;
        wanted.sort_unstable();
        let mut decoder = ruzstd::decoding::StreamingDecoder::new(self.compressed.as_slice())
            .map_err(|_| bad())?;
        let mut position = 0u64;
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 65536];
        let mut result = HashMap::new();
        for (offset, length, id) in wanted {
            if offset < position {
                return Err(bad());
            }
            while position < offset {
                let count = (offset - position).min(buffer.len() as u64) as usize;
                decoder.read_exact(&mut buffer[..count])?;
                hash.update(&buffer[..count]);
                position += count as u64;
            }
            let mut data = vec![0u8; length];
            decoder.read_exact(&mut data)?;
            hash.update(&data);
            position += length as u64;
            result.insert(id, serde_json::from_slice(&data).map_err(|_| bad())?);
        }
        loop {
            let count = decoder.read(&mut buffer)?;
            if count == 0 {
                break;
            }
            position += count as u64;
            if position > LIMIT as u64 {
                return Err(bad());
            }
            hash.update(&buffer[..count]);
        }
        if position != 211_540_141 || hex(hash.finalize()) != DATA_SHA {
            return Err(bad());
        }
        Ok(result)
    }
    pub fn node_index(&self, name: &str) -> io::Result<usize> {
        self.nodes
            .iter()
            .position(|node| node.name.eq_ignore_ascii_case(name))
            .ok_or_else(bad)
    }
    pub fn evidence(
        &self,
        conditions: &BTreeMap<String, String>,
    ) -> io::Result<BTreeMap<usize, BTreeSet<u32>>> {
        let wanted = conditions
            .keys()
            .map(|name| {
                self.nodes
                    .iter()
                    .position(|node| node.name.eq_ignore_ascii_case(name))
                    .ok_or_else(bad)
            })
            .collect::<io::Result<Vec<_>>>()?;
        let values = self.values(
            wanted
                .iter()
                .flat_map(|index| self.nodes[*index].possible.iter().copied()),
        )?;
        let mut evidence = BTreeMap::new();
        for ((_, condition), index) in conditions.iter().zip(wanted) {
            let allowed = self.nodes[index]
                .possible
                .iter()
                .filter(|id| {
                    values
                        .get(id)
                        .and_then(Value::as_str)
                        .is_some_and(|v| v.eq_ignore_ascii_case(condition))
                })
                .copied()
                .collect::<BTreeSet<_>>();
            if allowed.is_empty() {
                return Err(bad());
            }
            evidence.insert(index, allowed);
        }
        Ok(evidence)
    }
    pub fn trace(
        &self,
        target: usize,
        evidence: &BTreeMap<usize, BTreeSet<u32>>,
    ) -> io::Result<Vec<(u32, f64)>> {
        let node = self.nodes.get(target).ok_or_else(bad)?;
        let mut relevant = node.ancestors.clone();
        relevant.insert(target);
        for index in evidence.keys() {
            let node = self.nodes.get(*index).ok_or_else(bad)?;
            relevant.insert(*index);
            relevant.extend(node.ancestors.iter().copied());
        }
        let mut beam = vec![Point {
            choices: vec![EMPTY; self.nodes.len()],
            weight: 1.0,
            order: 0,
        }];
        for (index, node) in self
            .nodes
            .iter()
            .enumerate()
            .filter(|(i, _)| relevant.contains(i))
        {
            let mut next = BinaryHeap::new();
            let mut order = 0u64;
            for assignment in beam {
                let parents = node
                    .parents
                    .iter()
                    .map(|i| assignment.choices[*i])
                    .collect::<Vec<_>>();
                if parents.contains(&EMPTY) {
                    return Err(bad());
                }
                let uniform;
                let weights = match node.tables.get(&parents).filter(|v| !v.is_empty()) {
                    Some(weights) => weights.as_slice(),
                    None => {
                        uniform = node
                            .possible
                            .iter()
                            .map(|id| (*id, 1.0 / node.possible.len() as f64))
                            .collect::<Vec<_>>();
                        &uniform
                    }
                };
                for (id, p) in weights {
                    if *p <= 0.0
                        || evidence
                            .get(&index)
                            .is_some_and(|allowed| !allowed.contains(id))
                    {
                        continue;
                    }
                    let mut choices = assignment.choices.clone();
                    choices[index] = *id;
                    let point = Point {
                        choices,
                        weight: assignment.weight * p,
                        order,
                    };
                    order += 1;
                    if next.len() < BEAM {
                        next.push(Reverse(point));
                    } else if next.peek().is_some_and(|worst| point > worst.0) {
                        next.pop();
                        next.push(Reverse(point));
                    }
                }
            }
            beam = next.into_iter().map(|point| point.0).collect();
            if order <= BEAM as u64 {
                beam.sort_by_key(|point| point.order);
            } else {
                beam.sort_by(|a, b| b.cmp(a));
            }
            if beam.is_empty() {
                return Ok(Vec::new());
            }
        }
        let mut result = Vec::<(u32, f64)>::new();
        let mut total = 0.0;
        for point in beam {
            let value = point.choices[target];
            if let Some((_, weight)) = result.iter_mut().find(|(id, _)| *id == value) {
                *weight += point.weight;
            } else {
                result.push((value, point.weight));
            }
            total += point.weight;
        }
        if total <= 0.0 {
            return Ok(Vec::new());
        }
        for (_, weight) in &mut result {
            *weight /= total;
        }
        Ok(result)
    }
    pub fn generate(&self, conditions: &BTreeMap<String, String>) -> io::Result<Value> {
        let mut evidence = self.evidence(conditions)?;
        let mut selected = Vec::new();
        for index in 0..self.nodes.len() {
            let allowed = evidence.remove(&index);
            let mut distribution = self.trace(index, &evidence)?;
            if let Some(allowed) = allowed {
                distribution.retain(|(id, _)| allowed.contains(id));
                let total: f64 = distribution.iter().map(|(_, p)| p).sum();
                if total <= 0.0 {
                    distribution = allowed
                        .iter()
                        .map(|id| (*id, 1.0 / allowed.len() as f64))
                        .collect();
                } else {
                    for (_, p) in &mut distribution {
                        *p /= total;
                    }
                }
            }
            let mut random = [0u8; 8];
            getrandom::fill(&mut random)
                .map_err(|_| io::Error::other("Fingerprint entropy unavailable"))?;
            let anchor = (u64::from_le_bytes(random) >> 11) as f64 / (1u64 << 53) as f64;
            let mut cumulative = 0.0;
            let mut selected_id = distribution.first().ok_or_else(bad)?.0;
            for (id, p) in distribution {
                cumulative += p;
                if anchor < cumulative {
                    selected_id = id;
                    break;
                }
            }
            selected.push(selected_id);
            evidence.insert(index, BTreeSet::from([selected_id]));
        }
        let values = self.values(selected.iter().copied())?;
        let mut result = Map::new();
        for (node, id) in self.nodes.iter().zip(selected) {
            insert(
                &mut result,
                &node.name.split('.').collect::<Vec<_>>(),
                values.get(&id).ok_or_else(bad)?.clone(),
            )?;
        }
        Ok(Value::Object(result))
    }
}
fn insert(root: &mut Map<String, Value>, path: &[&str], value: Value) -> io::Result<()> {
    let (first, rest) = path.split_first().ok_or_else(bad)?;
    if rest.is_empty() {
        root.insert((*first).into(), value);
        return Ok(());
    }
    let child = root
        .entry((*first).to_owned())
        .or_insert_with(|| Value::Object(Map::new()));
    insert(child.as_object_mut().ok_or_else(bad)?, rest, value)
}
#[derive(PartialEq)]
struct Point {
    choices: Vec<u32>,
    weight: f64,
    order: u64,
}
impl Eq for Point {}
impl PartialOrd for Point {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Point {
    fn cmp(&self, other: &Self) -> Ordering {
        self.weight
            .total_cmp(&other.weight)
            .then_with(|| other.order.cmp(&self.order))
    }
}
