//! Actor identity and location.
//!
//! Every actor has an [`ActorId`] that stays the same wherever the actor runs. IDs are allocated in **blocks** of
//! [`ActorId::BLOCK_SIZE`] (2¹⁶). Each block is assigned to one node and shard, and the directory maps
//! `BlockId → (node, shard)`. That keeps the directory tiny (about 2.4 MB for 10¹⁰ actors) so every node can hold a
//! copy, which is what lets the design scale to billions of neurons. See `docs/plans/actors/08-scaling-to-billions.md`.
//!
//! The last block is reserved for special IDs such as [`ActorId::WORLD`].

use core::fmt;

/// A globally unique, location-independent actor identifier.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize), serde(transparent))]
pub struct ActorId(u64);

/// A block of [`ActorId::BLOCK_SIZE`] consecutive actor IDs, the unit of placement in the directory.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize), serde(transparent))]
pub struct BlockId(u64);

/// Identifies a node: one runtime process, normally one per host.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize), serde(transparent))]
pub struct NodeId(pub u32);

/// Identifies a shard (a single-threaded event loop, normally one per core) within a node.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize), serde(transparent))]
pub struct ShardId(pub u16);

/// Where an actor physically runs.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Location {
    /// The node (process) hosting the actor.
    pub node: NodeId,
    /// The shard within that node.
    pub shard: ShardId,
    /// The actor's slot in the shard's storage.
    pub slot: u32,
}

impl ActorId {
    /// Number of low bits that index an actor within its block.
    pub const BLOCK_BITS: u32 = 16;
    /// Number of IDs per block.
    pub const BLOCK_SIZE: u64 = 1 << Self::BLOCK_BITS;

    /// The outside world: the sender of sensor input, UI injections and other external messages.
    pub const WORLD: Self = Self(u64::MAX);
    /// The cluster coordinator.
    pub const COORDINATOR: Self = Self(u64::MAX - 1);

    /// Creates an ID from its raw value.
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// Creates the ID at `index` within `block`.
    pub const fn from_parts(block: BlockId, index: u16) -> Self {
        Self((block.0 << Self::BLOCK_BITS) | index as u64)
    }

    /// The raw value.
    pub const fn raw(self) -> u64 {
        self.0
    }

    /// The block this ID belongs to.
    pub const fn block(self) -> BlockId {
        BlockId(self.0 >> Self::BLOCK_BITS)
    }

    /// The ID's position within its block.
    pub const fn index_in_block(self) -> u16 {
        // Truncation keeps exactly the low BLOCK_BITS (16) bits.
        self.0 as u16
    }

    /// `true` for IDs in the reserved block (such as [`Self::WORLD`]), which are never allocated to actors.
    pub const fn is_reserved(self) -> bool {
        self.block().is_reserved()
    }
}

impl BlockId {
    /// The highest block number (IDs are 64 bits, of which [`ActorId::BLOCK_BITS`] index within the block).
    pub const MAX: Self = Self(u64::MAX >> ActorId::BLOCK_BITS);
    /// The last block, reserved for special IDs. Never allocate it.
    pub const RESERVED: Self = Self::MAX;

    /// Creates a block from its number.
    ///
    /// # Panics
    /// If `raw` is larger than [`Self::MAX`].
    pub const fn new(raw: u64) -> Self {
        assert!(raw <= Self::MAX.0, "BlockId out of range");
        Self(raw)
    }

    /// The block number.
    pub const fn raw(self) -> u64 {
        self.0
    }

    /// The first ID in this block.
    pub const fn first_id(self) -> ActorId {
        ActorId::from_parts(self, 0)
    }

    /// `true` for the reserved block.
    pub const fn is_reserved(self) -> bool {
        self.0 == Self::RESERVED.0
    }
}

impl fmt::Display for ActorId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            Self::WORLD => f.write_str("world"),
            Self::COORDINATOR => f.write_str("coordinator"),
            Self(raw) => write!(f, "#{raw}"),
        }
    }
}

impl fmt::Display for BlockId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "block {}", self.0)
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "node {}", self.0)
    }
}

impl fmt::Display for ShardId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "shard {}", self.0)
    }
}

impl fmt::Display for Location {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}/slot {}", self.node, self.shard, self.slot)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn block_and_index_split() {
        let id = ActorId::from_raw(3 * ActorId::BLOCK_SIZE + 7);
        assert_eq!(id.block(), BlockId::new(3));
        assert_eq!(id.index_in_block(), 7);
        assert_eq!(ActorId::from_parts(BlockId::new(3), 7), id);
        assert_eq!(BlockId::new(3).first_id(), ActorId::from_raw(3 * ActorId::BLOCK_SIZE));
    }

    #[test]
    fn last_block_is_reserved() {
        assert!(ActorId::WORLD.is_reserved());
        assert!(ActorId::COORDINATOR.is_reserved());
        assert!(!ActorId::from_raw(0).is_reserved());
        assert!(!BlockId::new(BlockId::MAX.raw() - 1).first_id().is_reserved());
        assert_eq!(ActorId::WORLD.block(), BlockId::RESERVED);
    }

    #[test]
    #[should_panic(expected = "BlockId out of range")]
    fn block_out_of_range_panics() {
        let _ = BlockId::new(BlockId::MAX.raw() + 1);
    }

    #[test]
    fn display() {
        assert_eq!(ActorId::from_raw(42).to_string(), "#42");
        assert_eq!(ActorId::WORLD.to_string(), "world");
        let loc = Location {
            node: NodeId(2),
            shard: ShardId(5),
            slot: 9,
        };
        assert_eq!(loc.to_string(), "node 2/shard 5/slot 9");
    }
}
