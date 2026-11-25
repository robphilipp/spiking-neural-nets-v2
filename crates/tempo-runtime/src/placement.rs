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
    spatial_partitions_weighted(positions, &vec![1; positions.len()], partitions)
}

/// Like [`spatial_partitions`], but cuts the curve into runs of equal total `weight` (an actor's expected work,
/// e.g. its incoming connections), so partitions, and the shards and nodes that run them, get equal work. With
/// equal weights it gives exactly what [`spatial_partitions`] gives.
///
/// # Panics
/// If `partitions` is zero, or `weights` isn't as long as `positions`.
pub fn spatial_partitions_weighted(positions: &[[f64; 3]], weights: &[u64], partitions: u32) -> Vec<Partition> {
    assert!(partitions > 0, "need at least one partition");
    assert_eq!(weights.len(), positions.len(), "a weight for each position");
    if positions.is_empty() {
        return Vec::new();
    }
    let order = curve_order(positions);
    let total: u128 = weights.iter().map(|&w| u128::from(w)).sum::<u128>().max(1);
    let mut result = vec![Partition(0); positions.len()];
    let mut before: u128 = 0;
    for &i in &order {
        result[i] = Partition((before * u128::from(partitions) / total).min(u128::from(partitions - 1)) as u32);
        before += u128::from(weights[i]);
    }
    result
}

/// Like [`spatial_partitions_weighted`], with some actors pinned to a range of partitions (`[first, end)`, a node's:
/// see [`crate::NodePlacement::partitions_of`]). Each range's pinned actors are cut along the curve over its
/// partitions; the free actors then fill every partition up to an equal share of all the work (what's pinned
/// counts), in curve order, so the work stays balanced.
///
/// # Panics
/// If `partitions` is zero, the slices' lengths differ, or a range is empty or beyond `partitions`.
pub fn spatial_partitions_pinned(
    positions: &[[f64; 3]],
    weights: &[u64],
    pinned: &[Option<(u32, u32)>],
    partitions: u32,
) -> Vec<Partition> {
    assert!(partitions > 0, "need at least one partition");
    assert!(
        weights.len() == positions.len() && pinned.len() == positions.len(),
        "a weight and pin for each position"
    );
    let order = curve_order(positions);
    let p = partitions as usize;
    let mut load = vec![0f64; p];
    let mut result = vec![Partition(0); positions.len()];
    // Pinned actors, range by range.
    let mut ranges: Vec<(u32, u32)> = pinned.iter().flatten().copied().collect();
    ranges.sort_unstable();
    ranges.dedup();
    for &(first, end) in &ranges {
        assert!(
            first < end && end <= partitions,
            "a partition range within the partitions"
        );
        let members: Vec<usize> = order
            .iter()
            .copied()
            .filter(|&i| pinned[i] == Some((first, end)))
            .collect();
        let total: u128 = members.iter().map(|&i| u128::from(weights[i])).sum::<u128>().max(1);
        let span = u128::from(end - first);
        let mut before: u128 = 0;
        for i in members {
            let k = first + (before * span / total).min(span - 1) as u32;
            result[i] = Partition(k);
            load[k as usize] += weights[i] as f64;
            before += u128::from(weights[i]);
        }
    }
    // Free actors fill what's left of each partition's share.
    let free: Vec<usize> = order.iter().copied().filter(|&i| pinned[i].is_none()).collect();
    if free.is_empty() {
        return result;
    }
    let all: f64 = weights.iter().map(|&w| w as f64).sum();
    let share = all / p as f64;
    let mut room: Vec<f64> = load.iter().map(|&l| (share - l).max(0.0)).collect();
    if room.iter().sum::<f64>() <= 0.0 {
        room = vec![1.0; p];
    }
    let room_total: f64 = room.iter().sum();
    let free_total: f64 = free.iter().map(|&i| weights[i] as f64).sum::<f64>().max(1.0);
    let (mut k, mut end_of_k, mut before) = (0usize, room[0], 0f64);
    for i in free {
        // Where this actor starts along the free work, in units of room.
        let at = before / free_total * room_total;
        while at >= end_of_k && k + 1 < p {
            k += 1;
            end_of_k += room[k];
        }
        result[i] = Partition(k as u32);
        before += weights[i] as f64;
    }
    result
}

/// The positions' indices in Z-order (Morton) along their bounding box.
fn curve_order(positions: &[[f64; 3]]) -> Vec<usize> {
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
    order.into_iter().map(|(_, i)| i).collect()
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
    fn weighted_partitions_balance_the_weight() {
        // A line of 1000 points; the first 100 weigh 10 each, the rest 1: 1900 in all, so 4 partitions of ~475.
        let positions: Vec<[f64; 3]> = (0..1000).map(|i| [f64::from(i), 0.0, 0.0]).collect();
        let weights: Vec<u64> = (0..1000).map(|i| if i < 100 { 10 } else { 1 }).collect();
        let parts = spatial_partitions_weighted(&positions, &weights, 4);
        for p in 0..4 {
            let w: u64 = parts
                .iter()
                .zip(&weights)
                .filter(|(x, _)| x.0 == p)
                .map(|(_, w)| w)
                .sum();
            assert!((465..=485).contains(&w), "partition {p} weighs {w}");
        }
        // Equal weights: the plain cut.
        assert_eq!(
            spatial_partitions_weighted(&positions, &[3; 1000], 4),
            spatial_partitions(&positions, 4)
        );
    }

    #[test]
    fn pinned_actors_stay_in_their_range_and_the_rest_balances() {
        // 1000 points on a line, 64 partitions over 2 nodes ([0, 32) and [32, 64)); the first 300 pinned to node 1.
        let positions: Vec<[f64; 3]> = (0..1000).map(|i| [f64::from(i), 0.0, 0.0]).collect();
        let weights = vec![1u64; 1000];
        let pinned: Vec<Option<(u32, u32)>> = (0..1000).map(|i| (i < 300).then_some((32, 64))).collect();
        let parts = spatial_partitions_pinned(&positions, &weights, &pinned, 64);
        assert!(parts[..300].iter().all(|p| (32..64).contains(&p.0)));
        // Node 0 takes about half the work (500), all of it free; node 1 the pinned 300 and about 200 free.
        let on_node_0 = parts.iter().filter(|p| p.0 < 32).count();
        assert!((480..=520).contains(&on_node_0), "{on_node_0}");
        // Nothing pinned: the plain cut.
        assert_eq!(
            spatial_partitions_pinned(&positions, &weights, &vec![None; 1000], 64),
            spatial_partitions(&positions, 64)
        );
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
