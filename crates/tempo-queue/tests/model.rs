//! Model-based tests: the wheel must behave exactly like the heap for any sequence of operations.

use proptest::prelude::*;
use tempo_core::{ActorId, OrderKey, SimDuration, SimTime};
use tempo_queue::{HeapQueue, TimeQueue, WheelQueue};

#[derive(Clone, Debug)]
enum Op {
    /// Push an event this many nanoseconds after the last popped time (or before it, if negative).
    Push {
        offset_ns: i64,
        src: u8,
    },
    Pop,
    Peek,
}

fn op() -> impl Strategy<Value = Op> {
    // Offsets mix nanosecond ties, synaptic delays, timers, long delays, and occasional pushes into the past.
    let offset = prop_oneof![
        0i64..1_000,
        1_000i64..5_000_000,
        5_000_000i64..100_000_000_000,
        100_000_000_000i64..1_000_000_000_000_000,
        -1_000_000i64..0,
    ];
    prop_oneof![
        3 => (offset, any::<u8>()).prop_map(|(offset_ns, src)| Op::Push { offset_ns, src }),
        2 => Just(Op::Pop),
        1 => Just(Op::Peek),
    ]
}

fn run(ops: &[Op], bucket: SimDuration) {
    let mut heap = HeapQueue::new();
    let mut wheel = WheelQueue::with_bucket(bucket);
    let mut now: u64 = 0;
    for (seq, op) in ops.iter().enumerate() {
        match *op {
            Op::Push { offset_ns, src } => {
                let at = now.saturating_add_signed(offset_ns);
                let key = OrderKey {
                    at: SimTime::from_nanos(at),
                    src: ActorId::from_raw(src.into()),
                    seq: seq as u64,
                    sub: 0,
                };
                heap.push(key, seq);
                wheel.push(key, seq);
            }
            Op::Pop => {
                let (h, w) = (heap.pop(), wheel.pop());
                assert_eq!(h, w);
                if let Some((key, _)) = h {
                    now = key.at.as_nanos();
                }
            }
            Op::Peek => assert_eq!(heap.peek_key(), wheel.peek_key()),
        }
        assert_eq!(heap.len(), wheel.len());
    }
    while let Some(h) = heap.pop() {
        assert_eq!(Some(h), wheel.pop());
    }
    assert!(wheel.pop().is_none());
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn wheel_matches_heap_with_1us_buckets(ops in prop::collection::vec(op(), 0..400)) {
        run(&ops, SimDuration::MICROSECOND);
    }

    #[test]
    fn wheel_matches_heap_with_odd_buckets(
        ops in prop::collection::vec(op(), 0..400),
        bucket_ns in 1u64..10_000,
    ) {
        run(&ops, SimDuration::from_nanos(bucket_ns));
    }
}
