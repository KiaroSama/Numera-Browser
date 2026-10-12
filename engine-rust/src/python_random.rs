//! Source-compatible integer-seeded MT19937, not a security random generator.
//! Algorithm attribution and redistribution terms: docs/migration-random-notices.txt.
//! Keep this compatibility boundary while seeded profile identities require Python parity.

pub(crate) struct PythonRandom {
    state: [u32; 624],
    index: usize,
}

impl PythonRandom {
    pub(crate) fn new(seed: i64) -> Self {
        let magnitude = seed.unsigned_abs();
        let mut key = vec![magnitude as u32];
        if magnitude >> 32 != 0 {
            key.push((magnitude >> 32) as u32);
        }
        let mut rng = Self {
            state: [0; 624],
            index: 624,
        };
        rng.state[0] = 19_650_218;
        for i in 1..624 {
            rng.state[i] = 1_812_433_253u32
                .wrapping_mul(rng.state[i - 1] ^ (rng.state[i - 1] >> 30))
                .wrapping_add(i as u32);
        }
        let (mut i, mut j) = (1, 0);
        for _ in 0..624.max(key.len()) {
            rng.state[i] = (rng.state[i]
                ^ (rng.state[i - 1] ^ (rng.state[i - 1] >> 30)).wrapping_mul(1_664_525))
            .wrapping_add(key[j])
            .wrapping_add(j as u32);
            i += 1;
            j += 1;
            if i == 624 {
                rng.state[0] = rng.state[623];
                i = 1;
            }
            if j == key.len() {
                j = 0;
            }
        }
        for _ in 0..623 {
            rng.state[i] = (rng.state[i]
                ^ (rng.state[i - 1] ^ (rng.state[i - 1] >> 30)).wrapping_mul(1_566_083_941))
            .wrapping_sub(i as u32);
            i += 1;
            if i == 624 {
                rng.state[0] = rng.state[623];
                i = 1;
            }
        }
        rng.state[0] = 0x8000_0000;
        rng
    }

    fn word(&mut self) -> u32 {
        if self.index == 624 {
            for i in 0..624 {
                let y = (self.state[i] & 0x8000_0000) | (self.state[(i + 1) % 624] & 0x7fff_ffff);
                self.state[i] = self.state[(i + 397) % 624]
                    ^ (y >> 1)
                    ^ if y & 1 == 1 { 0x9908_b0df } else { 0 };
            }
            self.index = 0;
        }
        let mut y = self.state[self.index];
        self.index += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9d2c_5680;
        y ^= (y << 15) & 0xefc6_0000;
        y ^ (y >> 18)
    }

    pub(crate) fn random(&mut self) -> f64 {
        let a = self.word() >> 5;
        let b = self.word() >> 6;
        (a as f64 * 67_108_864.0 + b as f64) / 9_007_199_254_740_992.0
    }

    pub(crate) fn below(&mut self, n: usize) -> usize {
        // All callers are bounded device pools, never attacker-controlled lengths.
        debug_assert!(n > 0 && n <= u32::MAX as usize);
        let bits = usize::BITS - n.leading_zeros();
        loop {
            let value = (self.word() >> (32 - bits)) as usize;
            if value < n {
                return value;
            }
        }
    }

    pub(crate) fn weighted(&mut self, weights: &[u64]) -> usize {
        let total: u64 = weights.iter().sum();
        let value = self.random() * total as f64;
        let mut cumulative = 0;
        for (i, weight) in weights.iter().enumerate() {
            cumulative += weight;
            if value < cumulative as f64 {
                return i;
            }
        }
        weights.len() - 1
    }

    pub(crate) fn sample_indices(&mut self, n: usize, k: usize) -> Vec<usize> {
        let mut pool: Vec<usize> = (0..n).collect();
        let mut selected = Vec::with_capacity(k);
        // Existing desktop font samples always take all but at most six entries:
        // CPython selects its pool-swap branch for these concrete catalog sizes.
        for i in 0..k {
            let j = self.below(n - i);
            selected.push(pool[j]);
            pool[j] = pool[n - i - 1];
        }
        selected
    }
}
