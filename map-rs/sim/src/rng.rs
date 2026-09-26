//! Deterministic randomness. One ChaCha8 stream per run, seeded from
//! SHA-256(scenario, sweep-point index, seed index), so a run's result
//! depends only on (config, seed) and never on thread scheduling. The
//! distributions are written out here (not taken from `rand_distr`) so the
//! exact sampling procedure is part of the documented method.

use rand::{Rng as _, RngCore, SeedableRng};
use rand_chacha::ChaCha8Rng;
use sha2::{Digest, Sha256};

pub struct Rng(ChaCha8Rng);

impl Rng {
    pub fn for_run(scenario: &str, point: usize, seed: u64) -> Rng {
        let mut h = Sha256::new();
        h.update(b"map-sim/run\0");
        h.update(scenario.as_bytes());
        h.update(0u8.to_le_bytes());
        h.update((point as u64).to_le_bytes());
        h.update(seed.to_le_bytes());
        Rng(ChaCha8Rng::from_seed(h.finalize().into()))
    }

    /// A child stream (e.g. per node key material) that does not perturb
    /// the parent's sequence beyond one draw.
    pub fn fork(&mut self) -> Rng {
        let mut s = [0u8; 32];
        self.0.fill_bytes(&mut s);
        Rng(ChaCha8Rng::from_seed(s))
    }

    pub fn bytes32(&mut self) -> [u8; 32] {
        let mut s = [0u8; 32];
        self.0.fill_bytes(&mut s);
        s
    }

    /// Uniform in [0, 1).
    pub fn f64(&mut self) -> f64 {
        self.0.gen::<f64>()
    }

    pub fn uniform(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.f64()
    }

    pub fn bernoulli(&mut self, p: f64) -> bool {
        self.f64() < p
    }

    /// Uniform integer in [0, n).
    pub fn below(&mut self, n: usize) -> usize {
        self.0.gen_range(0..n)
    }

    /// Exponential with the given rate (inverse-CDF).
    pub fn exp(&mut self, rate: f64) -> f64 {
        -(1.0 - self.f64()).ln() / rate
    }

    /// Standard normal (Box–Muller, one value per call).
    pub fn normal(&mut self, mean: f64, sd: f64) -> f64 {
        let u1 = 1.0 - self.f64();
        let u2 = self.f64();
        mean + sd * (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
    }

    /// Poisson(λ) by inversion (fine for the small λ used here).
    pub fn poisson(&mut self, lambda: f64) -> u64 {
        let l = (-lambda).exp();
        let (mut k, mut p) = (0u64, 1.0);
        loop {
            p *= self.f64();
            if p <= l {
                return k;
            }
            k += 1;
        }
    }

    /// k distinct indices from [0, n), in random order (partial Fisher–Yates).
    pub fn sample_indices(&mut self, n: usize, k: usize) -> Vec<usize> {
        let mut v: Vec<usize> = (0..n).collect();
        let k = k.min(n);
        for i in 0..k {
            let j = i + self.below(n - i);
            v.swap(i, j);
        }
        v.truncate(k);
        v
    }

    pub fn shuffle<T>(&mut self, v: &mut [T]) {
        for i in (1..v.len()).rev() {
            let j = self.below(i + 1);
            v.swap(i, j);
        }
    }

    /// Index chosen with probability proportional to `weights`.
    pub fn weighted(&mut self, weights: &[f64]) -> usize {
        let total: f64 = weights.iter().sum();
        let mut x = self.f64() * total;
        for (i, w) in weights.iter().enumerate() {
            if x < *w {
                return i;
            }
            x -= w;
        }
        weights.len() - 1
    }
}
