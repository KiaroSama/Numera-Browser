use serde_json::Value;
use std::sync::OnceLock;

static CATALOG: OnceLock<Value> = OnceLock::new();

pub(crate) fn catalog() -> Result<&'static Value, serde_json::Error> {
    if let Some(value) = CATALOG.get() {
        return Ok(value);
    }
    let value: Value = serde_json::from_str(include_str!("device_catalog.json"))?;
    let _ = CATALOG.set(value);
    // Initialization is immutable and another racing initializer is equivalent.
    Ok(CATALOG.get().expect("immutable parsed catalog installed"))
}

/// A reproducible sampler shared with production fingerprint generation.
/// This is an integer-seeded MT19937 compatibility generator, not a source of
/// cryptographic randomness. Invalid weights and empty weighted pools leave its
/// random stream unchanged.
pub struct SeededSampler {
    random: crate::python_random::PythonRandom,
}
impl SeededSampler {
    pub fn new(seed: i64) -> Self {
        Self {
            random: crate::python_random::PythonRandom::new(seed),
        }
    }
    /// Accept the same arbitrary-width integer seed used by native profiles.
    pub fn from_number(seed: &serde_json::Number) -> Result<Self, SamplingError> {
        crate::python_random::PythonRandom::from_number(seed)
            .map(|random| Self { random })
            .ok_or(SamplingError::InvalidSeed)
    }
    pub(crate) fn below(&mut self, upper: usize) -> usize {
        self.random.below(upper)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SamplingError {
    EmptyPool,
    NonPositiveTotal,
    NonFiniteTotal,
    InvalidSeed,
    InvalidCatalog,
}
impl std::fmt::Display for SamplingError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::EmptyPool => "Cannot choose from an empty sequence",
            Self::NonPositiveTotal => "Total of weights must be greater than zero",
            Self::NonFiniteTotal => "Total of weights must be finite",
            Self::InvalidSeed => "Sampler seed must be an integer",
            Self::InvalidCatalog => "Embedded font catalog is invalid",
        })
    }
}
impl std::error::Error for SamplingError {}

/// Draw a caller-owned item without cloning it. Missing, empty, or misaligned
/// weights select uniformly. Matching weights use their cumulative total;
/// individual negative weights retain the source library's bisect behavior.
pub fn weighted<'a, T>(
    sampler: &mut SeededSampler,
    pool: &'a [T],
    weights: Option<&[f64]>,
) -> Result<&'a T, SamplingError> {
    if pool.is_empty() {
        return Err(SamplingError::EmptyPool);
    }
    let Some(weights) =
        weights.filter(|weights| !weights.is_empty() && weights.len() == pool.len())
    else {
        return Ok(&pool[sampler.below(pool.len())]);
    };
    let mut cumulative = Vec::with_capacity(weights.len());
    let mut total = 0.0;
    for weight in weights {
        total += weight;
        cumulative.push(total);
    }
    // Check in this order: negative infinity follows the source's nonpositive
    // branch, while NaN and positive infinity follow its nonfinite branch.
    if total <= 0.0 {
        return Err(SamplingError::NonPositiveTotal);
    }
    if !total.is_finite() {
        return Err(SamplingError::NonFiniteTotal);
    }
    let target = sampler.random.random() * total;
    let (mut low, mut high) = (0, pool.len() - 1);
    // CPython random.choices bisects with hi=n-1. This also defines its outcome
    // for accepted negative individual weights and excludes an out-of-range
    // result if floating-point multiplication rounds to the cumulative total.
    while low < high {
        let middle = low + (high - low) / 2;
        if target < cumulative[middle] {
            high = middle;
        } else {
            low = middle + 1;
        }
    }
    Ok(&pool[low])
}

/// Draw the source catalog's installed fonts. Unknown OS keys return an empty
/// list. Mobile fonts are complete and sorted without consuming randomness;
/// desktop selection drops zero through six fonts before sorting the sample.
pub fn sample_fonts(sampler: &mut SeededSampler, os: &str) -> Result<Vec<String>, SamplingError> {
    let data = catalog().map_err(|_| SamplingError::InvalidCatalog)?;
    let Some(pool) = data["catalog"]["FONTS"][os]
        .as_array()
        .filter(|pool| !pool.is_empty())
    else {
        return Ok(Vec::new());
    };
    let mobile = matches!(os, "android" | "ios");
    let indices = if mobile {
        (0..pool.len()).collect()
    } else {
        let count = 6.max(pool.len().saturating_sub(sampler.below(7)));
        sampler.random.sample_indices(pool.len(), count)
    };
    let mut fonts = indices
        .into_iter()
        .map(|index| {
            pool[index]
                .as_str()
                .map(str::to_owned)
                .ok_or(SamplingError::InvalidCatalog)
        })
        .collect::<Result<Vec<_>, _>>()?;
    fonts.sort();
    Ok(fonts)
}
