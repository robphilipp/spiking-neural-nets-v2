//! A ring of actors passing tokens, each hop with its own delay (like the ring networks in the v1 wiki).
//! Every arrival time is known exactly in advance, so this checks timing precision.

use tempo_runtime::{Actor, ActorId, Ctx, Runtime, SimDuration, SimTime};

/// A token: which one it is, and how many hops it has made.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Token {
    /// Token number.
    pub id: u32,
    /// Hops completed so far.
    pub hops: u64,
}

/// Emitted when a node receives a token.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Arrival {
    /// The token.
    pub token: u32,
    /// Hops completed when it arrived.
    pub hops: u64,
}

/// A ring node: forwards each token to `next` after `delay`.
#[derive(Debug)]
pub struct Node {
    /// The next node in the ring.
    pub next: ActorId,
    /// Delay of the hop to `next`.
    pub delay: SimDuration,
    /// Stop forwarding a token after this many hops.
    pub max_hops: u64,
    /// Emit an [`Arrival`] for each token received.
    pub emit: bool,
}

impl Actor<Token, Arrival> for Node {
    fn on_message(&mut self, ctx: &mut Ctx<'_, Token, Arrival>, _from: ActorId, token: Token) {
        if self.emit {
            ctx.emit(Arrival {
                token: token.id,
                hops: token.hops,
            });
        }
        if token.hops < self.max_hops {
            ctx.send_after(
                self.next,
                self.delay,
                Token {
                    id: token.id,
                    hops: token.hops + 1,
                },
            );
        }
    }
}

/// Builds a ring with one node per entry of `delays` (node `i` forwards after `delays[i]`), and injects `tokens`
/// tokens at time 0, spread evenly around the ring. Returns the node IDs.
pub fn build(
    rt: &mut Runtime<Token, Arrival>,
    delays: &[SimDuration],
    tokens: u32,
    max_hops: u64,
    emit: bool,
) -> Vec<ActorId> {
    let nodes: Vec<ActorId> = delays
        .iter()
        .map(|&delay| {
            rt.spawn(Node {
                next: ActorId::WORLD,
                delay,
                max_hops,
                emit,
            })
        })
        .collect();
    for (i, &id) in nodes.iter().enumerate() {
        rt.actor_mut::<Node>(id).expect("just spawned").next = nodes[(i + 1) % nodes.len()];
    }
    for t in 0..tokens {
        let start = nodes[t as usize * nodes.len() / tokens.max(1) as usize];
        rt.inject(start, SimTime::ZERO, Token { id: t, hops: 0 });
    }
    nodes
}
