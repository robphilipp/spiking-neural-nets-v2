//! Placement: which shard an actor runs on.
//!
//! Actors are spawned into **partitions**: a fixed number of fine-grained placement groups chosen when the network
//! is built (by default 64). Actor IDs are allocated per partition, and partitions are mapped onto however many
//! shards the runtime has, as contiguous ranges. So the same network gets the same IDs, and therefore the same
//! random streams and the same results, on 1 shard or 16. Spatial placement puts nearby neurons in the same or
//! neighbouring partitions, which keeps most connections inside a shard and makes cross-shard delays longer.

/// A placement group. See the module documentation.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Partition(pub u32);

/// The shard that runs `partition`, when `partitions` partitions are spread over `shards` shards (on one node).
#[cfg(test)]
pub(crate) fn shard_of(partition: Partition, partitions: u32, shards: usize) -> u16 {
    (u64::from(partition.0) * shards as u64 / u64::from(partitions)) as u16
}

/// Assigns positions to `partitions` partitions so that each partition is a compact region of space: positions are
/// ordered along a Morton (Z-order) curve and the order is cut into equal-sized runs.
///
/// # Panics
/// If `partitions` is zero.
pub fn spatial_partitions(positions: &[[f64; 3]], partitions: u32) -> Vec<Partition> {
    assert!(partitions > 0, "need at least one partition");
    if positions.is_empty() {
        return Vec::new();
    }
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for p in positions {
        for d in 0..3 {
            lo[d] = lo[d].min(p[d]);
            hi[d] = hi[d].max(p[d]);
        }
    }
    const MAX: f64 = ((1u32 << 21) - 1) as f64;
    let quantize = |v: f64, d: usize| {
        let span = hi[d] - lo[d];
        if span > 0.0 {
            (((v - lo[d]) / span) * MAX).round() as u64
        } else {
            0
        }
    };
    let mut order: Vec<(u64, usize)> = positions
        .iter()
        .enumerate()
        .map(|(i, p)| (morton3(quantize(p[0], 0), quantize(p[1], 1), quantize(p[2], 2)), i))
        .collect();
    order.sort_unstable();
    let n = positions.len() as u64;
    let mut result = vec![Partition(0); positions.len()];
    for (rank, &(_, i)) in order.iter().enumerate() {
        result[i] = Partition((rank as u64 * u64::from(partitions) / n) as u32);
    }
    result
}

/// Interleaves the low 21 bits of x, y and z into a 63-bit Morton code.
fn morton3(x: u64, y: u64, z: u64) -> u64 {
    spread(x) | (spread(y) << 1) | (spread(z) << 2)
}

/// Spreads the low 21 bits of `v` so there are two zero bits between each.
fn spread(v: u64) -> u64 {
    let mut v = v & 0x1f_ffff;
    v = (v | (v << 32)) & 0x001f_0000_0000_ffff;
    v = (v | (v << 16)) & 0x001f_0000_ff00_00ff;
    v = (v | (v << 8)) & 0x100f_00f0_0f00_f00f;
    v = (v | (v << 4)) & 0x10c3_0c30_c30c_30c3;
    v = (v | (v << 2)) & 0x1249_2492_4924_9249;
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn morton_interleaves_bits() {
        assert_eq!(morton3(1, 0, 0), 0b001);
        assert_eq!(morton3(0, 1, 0), 0b010);
        assert_eq!(morton3(0, 0, 1), 0b100);
        assert_eq!(morton3(3, 0, 0), 0b001_001);
    }

    #[test]
    fn partitions_are_balanced_and_local() {
        // A 10 × 10 × 10 grid into 8 partitions: each gets 125 points, and points in one octant share a partition.
        let positions: Vec<[f64; 3]> = (0..1000)
            .map(|i| [(i % 10) as f64, ((i / 10) % 10) as f64, (i / 100) as f64])
            .collect();
        let parts = spatial_partitions(&positions, 8);
        for p in 0..8 {
            assert_eq!(parts.iter().filter(|x| x.0 == p).count(), 125);
        }
        let octant = |p: &[f64; 3]| (p[0] >= 5.0) as u8 | ((p[1] >= 5.0) as u8) << 1 | ((p[2] >= 5.0) as u8) << 2;
        for (pos, part) in positions.iter().zip(&parts) {
            let same_octant_part = positions
                .iter()
                .zip(&parts)
                .find(|(q, _)| octant(q) == octant(pos))
                .map(|(_, p)| *p);
            assert_eq!(same_octant_part, Some(*part));
        }
    }

    #[test]
    fn partitions_map_to_nodes_then_shards() {
        use crate::NodePlacement;
        let one = NodePlacement::single(3);
        for p in 0..64 {
            assert_eq!(one.home_of(p, 64), (0, shard_of(Partition(p), 64, 3)));
        }
        let three = NodePlacement {
            node: 0,
            shards_per_node: vec![2, 1, 3],
        };
        let homes: Vec<(u32, u16)> = (0..12).map(|p| three.home_of(p, 12)).collect();
        assert_eq!(
            homes,
            [
                (0, 0),
                (0, 0),
                (0, 1),
                (0, 1),
                (1, 0),
                (1, 0),
                (1, 0),
                (1, 0),
                (2, 0),
                (2, 0),
                (2, 1),
                (2, 2)
            ]
            .map(|(n, s)| (n, s as u16))
        );
        assert_eq!(three.partitions_of(1, 12), (4, 8));
    }

    #[test]
    fn partitions_map_to_contiguous_shard_ranges() {
        let shards: Vec<u16> = (0..8).map(|p| shard_of(Partition(p), 8, 3)).collect();
        assert_eq!(shards, [0, 0, 0, 1, 1, 1, 2, 2]);
        assert!((0..64).all(|p| shard_of(Partition(p), 64, 1) == 0));
    }
}
