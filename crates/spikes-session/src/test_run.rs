//! Test runs (app plan 05, I5): a session's network built again in a throwaway engine, run with some inputs for a
//! few simulated seconds, and its spikes counted per time bin. Nothing of the session itself is touched.

use std::time::{Duration, Instant};

use spikes_network::Event;
use tempo_runtime::SimDuration;

use crate::{BuildSettings, Engine, InputSource, RunSettings, Session, SessionError, Speed, Trial};

/// The wall-clock time a test run may take unless the plan says otherwise.
pub const DEFAULT_BUDGET: Duration = Duration::from_secs(30);

/// The most bins a test run may count (so a tiny bin can't make a huge answer).
const MAX_BINS: u64 = 200_000;

/// What to run.
#[derive(Clone, Debug)]
pub struct TestPlan {
    /// The input sources, as for [`Session::set_inputs`].
    pub sources: Vec<InputSource>,
    /// The trials, as for [`Session::set_inputs`].
    pub trials: Vec<Trial>,
    /// How long to run, in simulated time.
    pub duration: SimDuration,
    /// The width of the bins spikes are counted in.
    pub bin: SimDuration,
    /// Named groups of neurons (indices of the session's network).
    pub groups: Vec<(String, Vec<u32>)>,
    /// The wall-clock limit (default [`DEFAULT_BUDGET`]).
    pub budget: Option<Duration>,
    /// The seed, instead of the session's last build's.
    pub seed: Option<u64>,
}

/// One group's spikes.
#[derive(Clone, Debug, PartialEq)]
pub struct TestGroupOutcome {
    /// Its name.
    pub name: String,
    /// How many distinct neurons of the network it has.
    pub neurons: u32,
    /// Its spikes within the time reached.
    pub spikes: u64,
    /// Its spikes per bin.
    pub counts: Vec<u32>,
}

/// What a test run counted.
#[derive(Clone, Debug, PartialEq)]
pub struct TestOutcome {
    /// The simulated time reached.
    pub sim: SimDuration,
    /// The wall-clock time taken.
    pub elapsed: Duration,
    /// False if the budget stopped the run before its duration.
    pub complete: bool,
    /// The whole network's spikes within the time reached.
    pub spikes: u64,
    /// The whole network's spikes per bin.
    pub counts: Vec<u32>,
    /// The groups, in order.
    pub groups: Vec<TestGroupOutcome>,
}

/// Runs `session`'s description, built with its last build's settings (weights not recorded), in an engine of its
/// own, which nobody else sees: the session, and its engine's list of sessions, are untouched. It runs on this
/// engine alone, whatever nodes the session uses. The inputs are applied as [`Session::set_inputs`] does; the run goes
/// as fast as possible for `plan.duration`, or until the budget is spent.
///
/// # Errors
/// For a replay; for an invalid plan (a zero duration or bin, too many bins) or invalid inputs; if the network
/// can't be built or run.
pub fn test_run(session: &Session, plan: TestPlan) -> Result<TestOutcome, SessionError> {
    if session.is_replay() {
        return Err(SessionError::Invalid(
            "a replay plays back what was recorded: it can't be test run".into(),
        ));
    }
    let duration = plan.duration.as_nanos();
    let bin = plan.bin.as_nanos();
    if duration == 0 || bin == 0 {
        return Err(SessionError::Invalid(
            "a test run needs a duration and a bin above 0".into(),
        ));
    }
    let bins = duration.div_ceil(bin);
    if bins > MAX_BINS {
        return Err(SessionError::Invalid(format!(
            "{bins} bins is too many (at most {MAX_BINS}): use wider bins or a shorter run"
        )));
    }
    let budget = plan.budget.unwrap_or(DEFAULT_BUDGET);
    let started = Instant::now();

    let engine = Engine::new();
    let probe = engine.create("test run", session.description_text())?;
    let mut settings: BuildSettings = session.build_settings();
    settings.record_weights = false;
    if let Some(seed) = plan.seed {
        settings.seed = seed;
    }
    let network = probe.build(settings)?;
    probe.set_inputs(plan.sources, plan.trials)?;

    // Which groups each neuron is in.
    let n = network.neurons.len();
    let mut member_of: Vec<Vec<u32>> = vec![Vec::new(); n];
    let mut sizes = vec![0u32; plan.groups.len()];
    for (g, (_, neurons)) in plan.groups.iter().enumerate() {
        let mut sorted: Vec<u32> = neurons.iter().copied().filter(|&i| (i as usize) < n).collect();
        sorted.sort_unstable();
        sorted.dedup();
        sizes[g] = sorted.len() as u32;
        for i in sorted {
            member_of[i as usize].push(g as u32);
        }
    }
    let mut total = vec![0u32; bins as usize];
    let mut per_group = vec![vec![0u32; bins as usize]; plan.groups.len()];

    let events = probe
        .subscribe_events()
        .ok_or_else(|| SessionError::Invalid("the test network wasn't built".into()))?;
    let mut count = |batch: Vec<tempo_runtime::Emitted<Event>>| {
        for e in batch {
            let Event::Spike { at } = e.event else { continue };
            let t = at.since_start().as_nanos();
            if t >= duration {
                continue;
            }
            let Some(neuron) = network.index(e.src) else { continue };
            let b = (t / bin) as usize;
            total[b] = total[b].saturating_add(1);
            for &g in &member_of[neuron as usize] {
                let c = &mut per_group[g as usize][b];
                *c = c.saturating_add(1);
            }
        }
    };
    // The run goes in slices of simulated time, sized to take about a tenth of a second each: a run can only be
    // stopped between synchronization windows, and with one shard (or nothing to bound the windows) a window can be
    // the whole run, so the budget is checked between slices.
    let mut complete = true;
    let mut done = SimDuration::ZERO;
    let mut slice = SimDuration::from_millis(10);
    while done < plan.duration {
        if started.elapsed() >= budget {
            complete = false;
            break;
        }
        let began = Instant::now();
        probe.start(RunSettings {
            speed: Speed::AsFastAsPossible,
            duration: Some(slice.min(plan.duration - done)),
            record: None,
        })?;
        while !probe.wait_for_run(Duration::from_millis(20)) {
            count(events.drain());
            if started.elapsed() >= budget {
                probe.stop()?;
            }
        }
        // Everything emitted before the slice ended is queued now.
        count(events.drain());
        let reached = probe.sim_time().since_start();
        if reached < done + slice.min(plan.duration - done) {
            // Stopped inside the slice.
            done = reached;
            complete = false;
            break;
        }
        done = reached;
        let wall = began.elapsed();
        if wall < Duration::from_millis(100) {
            slice = slice.saturating_add(slice);
        } else if wall > Duration::from_millis(300) {
            slice = SimDuration::from_nanos((slice.as_nanos() / 2).max(1_000_000));
        }
    }
    let sim = done.min(plan.duration);
    let id = probe.id();
    drop(events);
    engine.close(id)?;

    // Counts only as far as the time reached.
    let reached = if complete {
        bins
    } else {
        sim.as_nanos().div_ceil(bin).min(bins)
    } as usize;
    total.truncate(reached);
    let groups = plan
        .groups
        .into_iter()
        .zip(per_group)
        .zip(sizes)
        .map(|(((name, _), mut counts), neurons)| {
            counts.truncate(reached);
            TestGroupOutcome {
                name,
                neurons,
                spikes: counts.iter().map(|&c| u64::from(c)).sum(),
                counts,
            }
        })
        .collect();
    Ok(TestOutcome {
        sim,
        elapsed: started.elapsed(),
        complete,
        spikes: total.iter().map(|&c| u64::from(c)).sum(),
        counts: total,
        groups,
    })
}
