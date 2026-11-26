//! A network description, built just to look at: its neurons and connections, without a session or a run.

use spikes_model::Sign;
use spikes_network::{BuildOptions, NeuronActor, Runtime};
use tempo_runtime::RuntimeConfig;

use crate::SessionError;
use crate::network::{NetworkInfo, SYNAPSE_LIST_LIMIT};

/// A description's network, built in a runtime of its own (one shard) and dropped.
#[derive(Clone, Debug)]
pub struct Preview {
    /// Its neurons, and its synapses up to [`SYNAPSE_LIST_LIMIT`].
    pub network: NetworkInfo,
    /// Each neuron's incoming connections, and how many of them are inhibitory.
    pub incoming: Vec<(u32, u32)>,
    /// Each neuron's outgoing connections, and how many of them are inhibitory.
    pub outgoing: Vec<(u32, u32)>,
    /// The node each neuron is pinned to (`node=`; its own, its population's or a set's), if any.
    pub pinned: Vec<Option<String>>,
}

/// Parses and builds `text` (seed 0) just to look at it.
pub fn preview(text: &str) -> Result<Preview, SessionError> {
    let description = spikes_dna::parse(text).map_err(SessionError::Parse)?;
    let mut rt = Runtime::new(RuntimeConfig {
        shards: 1,
        min_remote_delay: Some(spikes_network::direct_delay(&description)),
        ..RuntimeConfig::default()
    });
    let net = spikes_network::build(&mut rt, &description, &BuildOptions::default());
    let n = net.neurons.len();
    let mut incoming = vec![(0u32, 0u32); n];
    let mut outgoing = vec![(0u32, 0u32); n];
    for (post, neuron) in net.neurons.iter().enumerate() {
        let Some(actor) = rt.actor::<NeuronActor>(neuron.id()) else {
            continue;
        };
        for (synapse, &from) in actor.synapses.iter().zip(&actor.pre) {
            let inhibitory = u32::from(synapse.sign == Sign::Inhibitory);
            incoming[post].0 += 1;
            incoming[post].1 += inhibitory;
            if let Some(pre) = net.index(from) {
                outgoing[pre as usize].0 += 1;
                outgoing[pre as usize].1 += inhibitory;
            }
        }
    }
    let rows = if net.synapse_count <= SYNAPSE_LIST_LIMIT {
        spikes_record::synapse_rows(&rt, &net)
    } else {
        Vec::new()
    };
    Ok(Preview {
        network: NetworkInfo::new(&net, &description, &rows),
        incoming,
        outgoing,
        pinned: description.pinned_nodes(),
    })
}
