//! Message envelopes and the global event order.
//!
//! Every data-plane message travels in an [`Envelope`] that carries its delivery time. Events are processed in
//! [`OrderKey`] order: by time, then by sender, then by the sender's sequence number. Because the key depends only
//! on what the sender did, not on which thread or host delivered the message or when it physically arrived, ties
//! at the same instant are always broken the same way.

use crate::{ActorId, SimTime};

/// The total order of events: `(at, src, seq)`, compared lexicographically.
///
/// The field order matters: the derived `Ord` compares `at` first, then `src`, then `seq`.
#[derive(Copy, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct OrderKey {
    /// Intended delivery time.
    pub at: SimTime,
    /// The sender.
    pub src: ActorId,
    /// The sender's sequence number for this message.
    pub seq: u64,
}

/// A data-plane message with its delivery time, sender, recipient and ordering information.
#[derive(Clone, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct Envelope<M> {
    /// Intended delivery time.
    pub at: SimTime,
    /// The sender ([`ActorId::WORLD`] for external input).
    pub src: ActorId,
    /// The sender's sequence number (from its [`SeqCounter`]).
    pub seq: u64,
    /// The recipient.
    pub dst: ActorId,
    /// The message.
    pub msg: M,
}

impl<M> Envelope<M> {
    /// The key that positions this envelope in the global event order.
    pub fn key(&self) -> OrderKey {
        OrderKey {
            at: self.at,
            src: self.src,
            seq: self.seq,
        }
    }

    /// Replaces the message, keeping the timing and addressing.
    pub fn map<N>(self, f: impl FnOnce(M) -> N) -> Envelope<N> {
        Envelope {
            at: self.at,
            src: self.src,
            seq: self.seq,
            dst: self.dst,
            msg: f(self.msg),
        }
    }
}

/// Hands out a sender's sequence numbers: 0, 1, 2, … Each sender (actor, or ingress point for the world) owns one.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(serde::Serialize, serde::Deserialize),
    serde(transparent)
)]
pub struct SeqCounter(u64);

impl SeqCounter {
    /// A counter starting at 0.
    pub const fn new() -> Self {
        Self(0)
    }

    /// Returns the next sequence number.
    ///
    /// # Panics
    /// After 2⁶⁴ messages from one sender, which can't happen in practice.
    pub fn next_seq(&mut self) -> u64 {
        let seq = self.0;
        self.0 = seq.checked_add(1).expect("SeqCounter exhausted");
        seq
    }

    /// The number that [`Self::next_seq`] will return next.
    pub const fn peek(&self) -> u64 {
        self.0
    }
}

// Queue entries hold keys, so keep them compact.
const _: () = assert!(size_of::<OrderKey>() == 24);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::SimDuration;

    fn key(at_us: u64, src: u64, seq: u64) -> OrderKey {
        OrderKey {
            at: SimTime::from_start(SimDuration::from_micros(at_us)),
            src: ActorId::from_raw(src),
            seq,
        }
    }

    #[test]
    fn time_dominates_then_sender_then_sequence() {
        assert!(key(1, 9, 9) < key(2, 0, 0));
        assert!(key(5, 1, 9) < key(5, 2, 0));
        assert!(key(5, 1, 3) < key(5, 1, 4));
    }

    #[test]
    fn sorting_is_independent_of_arrival_order() {
        let mut a = vec![
            key(3, 1, 0),
            key(1, 2, 0),
            key(3, 0, 5),
            key(3, 0, 4),
            key(1, 1, 7),
        ];
        let mut b = a.clone();
        b.reverse();
        a.sort();
        b.sort();
        assert_eq!(a, b);
        assert_eq!(
            a,
            vec![
                key(1, 1, 7),
                key(1, 2, 0),
                key(3, 0, 4),
                key(3, 0, 5),
                key(3, 1, 0)
            ]
        );
    }

    #[test]
    fn envelope_key_and_map() {
        let env = Envelope {
            at: SimTime::from_nanos(10),
            src: ActorId::from_raw(1),
            seq: 2,
            dst: ActorId::from_raw(3),
            msg: 4_u32,
        };
        assert_eq!(
            env.key(),
            OrderKey {
                at: SimTime::from_nanos(10),
                src: ActorId::from_raw(1),
                seq: 2
            }
        );
        let mapped = env.map(|m| m.to_string());
        assert_eq!(mapped.msg, "4");
        assert_eq!(mapped.dst, ActorId::from_raw(3));
    }

    #[test]
    fn seq_counter_counts_from_zero() {
        let mut c = SeqCounter::new();
        assert_eq!(c.next_seq(), 0);
        assert_eq!(c.next_seq(), 1);
        assert_eq!(c.peek(), 2);
    }
}
