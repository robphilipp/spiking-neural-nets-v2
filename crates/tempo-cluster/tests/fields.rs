//! Field updates and injected input reach actors on other nodes.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tempo_cluster::{MemoryNetwork, StrictLink, field_codec};
use tempo_runtime::{
    Actor, ActorId, Ctx, Field, NodePlacement, Partition, Runtime, RuntimeConfig, SimDuration, SimTime,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
enum Msg {
    Write(u32),
    Read,
}

/// The sum of everything written.
#[derive(Clone, Default)]
struct Total(u32);

impl Field for Total {
    type Update = u32;
    fn apply(&mut self, _: SimTime, v: u32) {
        self.0 += v;
    }
}

struct Writer;
impl Actor<Msg, u32> for Writer {
    fn on_message(&mut self, ctx: &mut Ctx<'_, Msg, u32>, _: ActorId, msg: Msg) {
        if let Msg::Write(v) = msg {
            ctx.update_field::<Total>(SimDuration::from_millis(1), v);
        }
    }
}

struct Reader;
impl Actor<Msg, u32> for Reader {
    fn on_message(&mut self, ctx: &mut Ctx<'_, Msg, u32>, _: ActorId, _: Msg) {
        let total = ctx.field::<Total>().map_or(0, |t| t.0);
        ctx.emit(total);
    }
}

fn ms(v: u64) -> SimTime {
    SimTime::from_start(SimDuration::from_millis(v))
}

#[test]
fn field_updates_and_injections_cross_nodes() {
    tempo_testkit::with_timeout(
        std::time::Duration::from_secs(120),
        "field_updates_and_injections_cross_nodes",
        || {
            let net = MemoryNetwork::new(2);
            let results: Vec<Vec<(SimTime, u32)>> = std::thread::scope(|s| {
                let handles: Vec<_> = (0..2u32)
                    .map(|node| {
                        let net = Arc::clone(&net);
                        s.spawn(move || {
                            let mut rt: Runtime<Msg, u32> = Runtime::new(RuntimeConfig {
                                shards: 1,
                                partitions: 4,
                                min_remote_delay: Some(SimDuration::from_millis(1)),
                                cluster: Some(NodePlacement {
                                    node,
                                    shards_per_node: vec![1, 1],
                                }),
                                ..RuntimeConfig::default()
                            });
                            rt.buffer_events();
                            rt.add_field(Total::default());
                            let writer = rt.spawn_in(Partition(0), Writer); // node 0
                            let reader = rt.spawn_in(Partition(3), Reader); // node 1
                            assert_eq!((rt.node_of(writer), rt.node_of(reader)), (Some(0), Some(1)));
                            let link = StrictLink::new(node, 2, 1, vec![field_codec::<Total>()]);
                            link.attach(net.transport(node, link.sink()));
                            rt.set_link(Arc::new(link));
                            if node == 0 {
                                // Only the coordinator injects; the reader's input crosses to node 1.
                                rt.inject(writer, ms(1), Msg::Write(7));
                                rt.inject(reader, ms(5), Msg::Read);
                            }
                            rt.run_until(ms(10));
                            // And while running: a live injection for an actor on the other node.
                            if node == 0 {
                                let control = rt.control();
                                let clock = rt.clock();
                                std::thread::spawn(move || {
                                    drop(control.inject(writer, ms(12), Msg::Write(5)));
                                    drop(control.inject(reader, ms(15), Msg::Read));
                                    std::thread::sleep(std::time::Duration::from_millis(50));
                                    clock.stop();
                                });
                                rt.clock().set_time_factor(1.0);
                            }
                            rt.run();
                            rt.take_events().into_iter().map(|e| (e.at, e.event)).collect()
                        })
                    })
                    .collect();
                handles.into_iter().map(|h| h.join().unwrap()).collect()
            });
            assert_eq!(results[0], []);
            assert_eq!(results[1], [(ms(5), 7), (ms(15), 12)]);
        },
    )
}
