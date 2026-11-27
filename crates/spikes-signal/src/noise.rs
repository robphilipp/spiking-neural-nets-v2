//! Seeded noise for formulas, counter-based: a value is a hash of (key, channel, sample, call site), not the next
//! draw of a stream. So it doesn't depend on evaluation order, threads or shards, a run repeats exactly, and the UI's
//! preview computes the very same values (`signal.ts` mirrors this file, with BigInt).
//!
//! The hash is splitmix64's finalizer; uniforms take its top 53 bits.

const GOLDEN: u64 = 0x9E37_79B9_7F4A_7C15;

/// splitmix64's finalizer.
pub fn mix(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// FNV-1a over a name's UTF-8 bytes.
pub fn fnv1a(name: &str) -> u64 {
    name.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// A source's noise key: the run's seed and the source's name.
pub fn key(seed: u64, source: &str) -> u64 {
    mix(seed.wrapping_add(GOLDEN) ^ fnv1a(source))
}

/// Uniform in [0, 1) for `(key, channel, sample, site, part)`.
pub fn uniform(key: u64, channel: u32, sample: u64, site: u32, part: u32) -> f64 {
    let mut z = mix(key ^ u64::from(channel).wrapping_mul(GOLDEN));
    z = mix(z ^ sample.wrapping_mul(0xD6E8_FEB8_6659_FD93));
    z = mix(z ^ (u64::from(site) << 32 | u64::from(part)).wrapping_mul(0xA076_1D64_78BD_642F));
    (z >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
}

/// Standard normal (Box–Muller, the cosine branch) for `(key, channel, sample, site)`.
pub fn gauss(key: u64, channel: u32, sample: u64, site: u32) -> f64 {
    let u1 = 1.0 - uniform(key, channel, sample, site, 0); // (0, 1]: the logarithm is finite
    let u2 = uniform(key, channel, sample, site, 1);
    (-2.0 * u1.ln()).sqrt() * (std::f64::consts::TAU * u2).cos()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uniforms_and_normals_have_the_right_moments() {
        let k = key(1, "noise");
        let n = 100_000;
        let us: Vec<f64> = (0..n).map(|s| uniform(k, 0, s, 0, 0)).collect();
        let mean = us.iter().sum::<f64>() / n as f64;
        assert!((mean - 0.5).abs() < 0.01, "mean {mean}");
        assert!(us.iter().all(|u| (0.0..1.0).contains(u)));
        let gs: Vec<f64> = (0..n).map(|s| gauss(k, 3, s, 1)).collect();
        let m = gs.iter().sum::<f64>() / n as f64;
        let var = gs.iter().map(|g| (g - m).powi(2)).sum::<f64>() / n as f64;
        assert!(m.abs() < 0.02 && (var - 1.0).abs() < 0.03, "mean {m} var {var}");
    }

    #[test]
    fn keys_depend_on_seed_and_name() {
        assert_ne!(key(1, "a"), key(2, "a"));
        assert_ne!(key(1, "a"), key(1, "b"));
        assert_eq!(key(1, "a"), key(1, "a"));
    }
}
