//! Two actors passing a ball back and forth. Measures the bare cost of one message.

use tempo_runtime::{Actor, ActorId, Ctx, Runtime, SimDuration, SimTime};

/// The ball, carrying how many times it has been hit.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct Ball(pub u64);

/// A player returns the ball to its partner after `delay`, until `max_hits`.
#[derive(Debug)]
pub struct Player {
    /// Who to return the ball to.
    pub partner: ActorId,
    /// Delay before the ball reaches the partner.
    pub delay: SimDuration,
    /// Stop after this many hits in total.
    pub max_hits: u64,
    /// Hits this player has received.
    pub received: u64,
}

impl Actor<Ball, ()> for Player {
    fn on_message(&mut self, ctx: &mut Ctx<'_, Ball, ()>, _from: ActorId, Ball(hits): Ball) {
        self.received += 1;
        if hits < self.max_hits {
            ctx.send_after(self.partner, self.delay, Ball(hits + 1));
        }
    }
}

/// Creates two players and serves the ball at time 0. Running to completion delivers `max_hits + 1` messages.
pub fn build(rt: &mut Runtime<Ball, ()>, delay: SimDuration, max_hits: u64) -> (ActorId, ActorId) {
    let player = |partner| Player {
        partner,
        delay,
        max_hits,
        received: 0,
    };
    let a = rt.spawn(player(ActorId::WORLD));
    let b = rt.spawn(player(a));
    rt.actor_mut::<Player>(a).expect("just spawned").partner = b;
    rt.inject(a, SimTime::ZERO, Ball(0));
    (a, b)
}
