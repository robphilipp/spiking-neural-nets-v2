//! Broadcasts to many actors at once: to a named group, or to every actor within a region of space.
//!
//! A broadcast is sent as one message per shard and expanded where it arrives, so reaching a million actors costs
//! a handful of messages. Spatial broadcasts give each actor its own delay, `base_delay + distance / speed`, which
//! models a signal spreading from a point (e.g. a neuromodulator diffusing outwards).

use std::collections::HashMap;

use tempo_core::{ActorId, SimDuration};

/// Identifies a group of actors (see [`crate::Runtime::add_group`]).
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct GroupId(pub u32);

/// A broadcast to every actor (with a position) within `radius` of `center`.
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct SpatialBroadcast {
    /// Origin of the broadcast.
    pub center: [f64; 3],
    /// Actors farther than this (in the same units as positions) are not reached; infinite reaches every actor.
    pub radius: f64,
    /// Delay before the signal leaves the origin. For a cross-shard broadcast, at least the runtime's lookahead.
    pub base_delay: SimDuration,
    /// Spreading speed in distance units per second; `f64::INFINITY` reaches every target at `base_delay`.
    pub speed: f64,
}

impl SpatialBroadcast {
    pub(crate) fn delay_at(&self, distance: f64) -> SimDuration {
        if self.speed.is_infinite() {
            self.base_delay
        } else {
            self.base_delay + SimDuration::from_secs_f64(distance / self.speed)
        }
    }
}

/// Where a broadcast goes (also what crosses to other nodes).
pub(crate) type Target = crate::cluster::BroadcastTarget;

/// A uniform grid over actor positions, for finding the actors near a point.
#[derive(Debug, Default)]
pub(crate) struct SpatialIndex {
    cell: f64,
    cells: HashMap<[i64; 3], Vec<(ActorId, [f64; 3])>>,
    /// Each actor's current cell, so moving it leaves no stale entry behind.
    cell_of: HashMap<ActorId, [i64; 3]>,
}

impl SpatialIndex {
    pub(crate) fn new(cell: f64) -> Self {
        assert!(cell > 0.0 && cell.is_finite(), "spatial cell size must be positive");
        Self {
            cell,
            cells: HashMap::new(),
            cell_of: HashMap::new(),
        }
    }

    fn key(&self, p: [f64; 3]) -> [i64; 3] {
        p.map(|v| (v / self.cell).floor() as i64)
    }

    pub(crate) fn insert(&mut self, id: ActorId, position: [f64; 3]) {
        if let Some(old) = self.cell_of.remove(&id)
            && let Some(cell) = self.cells.get_mut(&old)
        {
            cell.retain(|(other, _)| *other != id);
        }
        let key = self.key(position);
        self.cells.entry(key).or_default().push((id, position));
        self.cell_of.insert(id, key);
    }

    /// Calls `f(actor, distance)` for every actor within `radius` of `center`.
    pub(crate) fn for_each_within(&self, center: [f64; 3], radius: f64, mut f: impl FnMut(ActorId, f64)) {
        let distance =
            |p: [f64; 3]| ((p[0] - center[0]).powi(2) + (p[1] - center[1]).powi(2) + (p[2] - center[2]).powi(2)).sqrt();
        if !radius.is_finite() {
            // Unlimited reach: every actor with a position.
            for cell in self.cells.values() {
                for &(id, p) in cell {
                    f(id, distance(p));
                }
            }
            return;
        }
        let lo = self.key(center.map(|c| c - radius));
        let hi = self.key(center.map(|c| c + radius));
        for x in lo[0]..=hi[0] {
            for y in lo[1]..=hi[1] {
                for z in lo[2]..=hi[2] {
                    let Some(cell) = self.cells.get(&[x, y, z]) else {
                        continue;
                    };
                    for &(id, p) in cell {
                        let d = ((p[0] - center[0]).powi(2) + (p[1] - center[1]).powi(2) + (p[2] - center[2]).powi(2))
                            .sqrt();
                        if d <= radius {
                            f(id, d);
                        }
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_exactly_the_actors_within_the_radius() {
        let mut index = SpatialIndex::new(10.0);
        for i in 0..100u64 {
            index.insert(ActorId::from_raw(i), [i as f64, 0.0, 0.0]);
        }
        let mut found = Vec::new();
        index.for_each_within([50.0, 0.0, 0.0], 5.0, |id, _| found.push(id.raw()));
        found.sort_unstable();
        assert_eq!(found, (45..=55).collect::<Vec<_>>());
    }

    #[test]
    fn moving_an_actor_leaves_no_stale_entry() {
        let mut index = SpatialIndex::new(10.0);
        let id = ActorId::from_raw(1);
        index.insert(id, [0.0, 0.0, 0.0]);
        index.insert(id, [100.0, 0.0, 0.0]);
        let mut near_origin = 0;
        index.for_each_within([0.0, 0.0, 0.0], 5.0, |_, _| near_origin += 1);
        let mut near_new = 0;
        index.for_each_within([100.0, 0.0, 0.0], 5.0, |_, _| near_new += 1);
        assert_eq!((near_origin, near_new), (0, 1));
    }
}
