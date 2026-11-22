//! Property tests for the invariants other crates rely on.

use proptest::prelude::*;
use tempo_core::{ActorId, BlockId, OrderKey, SimDuration, SimTime};

proptest! {
    /// Converting a float delay never errs by more than half a nanosecond (plus float precision at large values).
    #[test]
    fn float_micros_round_to_nearest_nanosecond(micros in 0.0f64..1e9) {
        let d = SimDuration::from_micros_f64(micros);
        let error_ns = (d.as_nanos() as f64 - micros * 1_000.0).abs();
        prop_assert!(error_ns <= 0.5 + micros * 1_000.0 * f64::EPSILON);
    }

    /// Adding then subtracting a duration gets back the same time.
    #[test]
    fn add_then_subtract_is_identity(t in 0u64..u64::MAX / 2, d in 0u64..u64::MAX / 2) {
        let t = SimTime::from_nanos(t);
        let d = SimDuration::from_nanos(d);
        prop_assert_eq!((t + d) - d, t);
        prop_assert_eq!((t + d) - t, d);
    }

    /// The event order is exactly lexicographic on (time, sender, sequence, sub).
    #[test]
    fn order_key_is_lexicographic(a in any::<(u64, u64, u64, u32)>(), b in any::<(u64, u64, u64, u32)>()) {
        let ka = OrderKey { at: SimTime::from_nanos(a.0), src: ActorId::from_raw(a.1), seq: a.2, sub: a.3 };
        let kb = OrderKey { at: SimTime::from_nanos(b.0), src: ActorId::from_raw(b.1), seq: b.2, sub: b.3 };
        prop_assert_eq!(ka.cmp(&kb), a.cmp(&b));
    }

    /// Splitting an ID into block and index and joining them again is lossless.
    #[test]
    fn block_split_round_trips(raw in any::<u64>()) {
        let id = ActorId::from_raw(raw);
        prop_assert_eq!(ActorId::from_parts(id.block(), id.index_in_block()), id);
        prop_assert_eq!(BlockId::new(id.block().raw()), id.block());
    }
}
