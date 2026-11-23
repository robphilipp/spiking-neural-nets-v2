//! Runs one tempo simulation over several processes or hosts. See [`tempo_runtime::cluster`] for how a runtime
//! splits its actors over nodes; this crate supplies the [`Link`](tempo_runtime::Link) between them:
//!
//! - [`StrictLink`]: the exact cross-node window protocol (results identical to a single process);
//! - transports: [`MemoryNetwork`] (several nodes in one process, for tests) and [`TcpTransport`];
//! - [`field_codec`]: how field updates cross nodes;
//! - [`encode_events`] / [`decode_events`]: emitted events, for forwarding to the node that shows or records them.

mod link;
mod transport;
mod wire;

pub use link::{AppHandler, Arrivals, LinkStats, StrictLink};
pub use transport::{FrameSink, MemoryNetwork, TcpTransport, Transport};
pub use wire::{FieldCodec, field_codec};

use serde::Serialize;
use serde::de::DeserializeOwned;
use tempo_core::{ActorId, SimTime};
use tempo_runtime::Emitted;

/// Encodes a batch of emitted events.
pub fn encode_events<E: Serialize>(batch: &[Emitted<E>]) -> Vec<u8> {
    let wire: Vec<(u64, u64, &E)> = batch.iter().map(|e| (e.at.as_nanos(), e.src.raw(), &e.event)).collect();
    postcard::to_stdvec(&wire).expect("events encode")
}

/// Decodes a batch of emitted events ([`encode_events`]).
pub fn decode_events<E: DeserializeOwned>(bytes: &[u8]) -> Option<Vec<Emitted<E>>> {
    let wire: Vec<(u64, u64, E)> = postcard::from_bytes(bytes).ok()?;
    Some(
        wire.into_iter()
            .map(|(at, src, event)| Emitted {
                at: SimTime::from_nanos(at),
                src: ActorId::from_raw(src),
                event,
            })
            .collect(),
    )
}
