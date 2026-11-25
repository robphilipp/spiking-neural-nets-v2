//! A description's neurons, compactly: neurons listed one by one (`NRN`) as they are, and each population (`POP`)
//! as one template and its neurons' positions. A population of millions costs 24 bytes a neuron, not a full
//! description each, and every node of a cluster holds a copy.
//!
//! Neurons are numbered densely: listed ones first, in file order, then each population's in turn. A population's
//! neurons are named `<population>-<i>`.

use std::borrow::Cow;
use std::collections::HashMap;

use spikes_model::{NeuronParams, Receptor};

use crate::{NeuronDescription, Span, SynapseDefaults};

/// The neurons of a network (see the module documentation).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Neurons {
    listed: Vec<NeuronDescription>,
    populations: Vec<PopulationNeurons>,
    len: usize,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
struct PopulationNeurons {
    name: String,
    /// Everything but the name and position (its `id` and `location_um` are unused).
    template: NeuronDescription,
    locations: Vec<[f64; 3]>,
}

/// One neuron, as a view into [`Neurons`].
#[derive(Copy, Clone, Debug)]
pub struct NeuronView<'a> {
    template: &'a NeuronDescription,
    /// For a population's neuron: the population's name and its index in it.
    member: Option<(&'a str, u32)>,
    location: [f64; 3],
}

impl<'a> NeuronView<'a> {
    /// Its name (`nid`, or `<population>-<i>`).
    pub fn id(&self) -> Cow<'a, str> {
        match self.member {
            Some((population, i)) => Cow::Owned(format!("{population}-{i}")),
            None => Cow::Borrowed(&self.template.id),
        }
    }
    /// Soma position in µm.
    pub fn location_um(&self) -> [f64; 3] {
        self.location
    }
    /// Execution group (`grp`).
    pub fn group(&self) -> &'a str {
        &self.template.group
    }
    /// Default sign of its outgoing connections.
    pub fn inhibitory(&self) -> bool {
        self.template.inhibitory
    }
    /// Dynamics parameters.
    pub fn params(&self) -> &'a NeuronParams {
        &self.template.params
    }
    /// Axon conduction speed in m/s.
    pub fn conduction_m_per_s(&self) -> f64 {
        self.template.conduction_m_per_s
    }
    /// Synaptic delay of its outgoing connections in ms.
    pub fn synaptic_delay_ms(&self) -> f64 {
        self.template.synaptic_delay_ms
    }
    /// Settings for its incoming synapses.
    pub fn synapses(&self) -> &'a SynapseDefaults {
        &self.template.synapses
    }
    /// Neuromodulator receptors.
    pub fn receptors(&self) -> &'a [Receptor] {
        &self.template.receptors
    }
    /// Modulator released when it spikes, and how much.
    pub fn release(&self) -> Option<(u16, f64)> {
        self.template.release
    }
    /// Where it (or its population) is defined.
    pub fn span(&self) -> Span {
        self.template.span
    }
    /// Its population (by index in the order populations come), if it belongs to one; neurons of one population
    /// share everything but their names and positions.
    pub fn population(&self) -> Option<&'a str> {
        self.member.map(|(p, _)| p)
    }
    /// The template it shares with the rest of its population (or its own description), by address: neurons with
    /// the same template have the same parameters.
    pub fn template(&self) -> &'a NeuronDescription {
        self.template
    }
    /// As a full description.
    pub fn to_description(&self) -> NeuronDescription {
        NeuronDescription {
            id: self.id().into_owned(),
            location_um: self.location,
            ..self.template.clone()
        }
    }
}

impl Neurons {
    /// No neurons.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a neuron listed on its own. (Listed neurons come before populations.)
    ///
    /// # Panics
    /// After a population was added.
    pub fn push(&mut self, neuron: NeuronDescription) {
        assert!(self.populations.is_empty(), "listed neurons come before populations");
        self.listed.push(neuron);
        self.len += 1;
    }

    /// Adds a population: `locations.len()` neurons like `template`, named `<name>-<i>`.
    pub fn push_population(&mut self, name: String, template: NeuronDescription, locations: Vec<[f64; 3]>) {
        self.len += locations.len();
        self.populations.push(PopulationNeurons {
            name,
            template,
            locations,
        });
    }

    /// How many.
    pub fn len(&self) -> usize {
        self.len
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Neuron `i`.
    pub fn get(&self, i: usize) -> Option<NeuronView<'_>> {
        if let Some(n) = self.listed.get(i) {
            return Some(NeuronView {
                template: n,
                member: None,
                location: n.location_um,
            });
        }
        let mut i = i - self.listed.len();
        for p in &self.populations {
            if let Some(&location) = p.locations.get(i) {
                return Some(NeuronView {
                    template: &p.template,
                    member: Some((&p.name, i as u32)),
                    location,
                });
            }
            i -= p.locations.len();
        }
        None
    }

    /// Every neuron, in order.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = NeuronView<'_>> + Clone + '_ {
        let listed = self.listed.iter().map(|n| NeuronView {
            template: n,
            member: None,
            location: n.location_um,
        });
        let members = self.populations.iter().flat_map(|p| {
            p.locations.iter().zip(0u32..).map(move |(&location, i)| NeuronView {
                template: &p.template,
                member: Some((&p.name, i)),
                location,
            })
        });
        Counted {
            inner: listed.chain(members),
            left: self.len,
        }
    }

    /// The neurons listed one by one.
    pub fn listed(&self) -> &[NeuronDescription] {
        &self.listed
    }

    /// Finding neurons by name, and naming them, without a string per neuron.
    pub fn names(&self) -> NeuronNames {
        let mut start = self.listed.len() as u32;
        NeuronNames {
            listed_index: self.listed.iter().zip(0u32..).map(|(n, i)| (n.id.clone(), i)).collect(),
            listed: self.listed.iter().map(|n| n.id.clone()).collect(),
            populations: self
                .populations
                .iter()
                .map(|p| {
                    let first = start;
                    start += p.locations.len() as u32;
                    (p.name.clone(), first, p.locations.len() as u32)
                })
                .collect(),
        }
    }
}

impl FromIterator<NeuronDescription> for Neurons {
    /// Neurons listed one by one.
    fn from_iter<I: IntoIterator<Item = NeuronDescription>>(iter: I) -> Self {
        let mut neurons = Self::new();
        for n in iter {
            neurons.push(n);
        }
        neurons
    }
}

/// An iterator that knows its length.
#[derive(Clone)]
struct Counted<I> {
    inner: I,
    left: usize,
}

impl<I: Iterator> Iterator for Counted<I> {
    type Item = I::Item;
    fn next(&mut self) -> Option<I::Item> {
        let item = self.inner.next()?;
        self.left -= 1;
        Some(item)
    }
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.left, Some(self.left))
    }
}

impl<I: Iterator> ExactSizeIterator for Counted<I> {}

/// Neuron names and indices (see [`Neurons::names`]): a string for each listed neuron, none for population members.
#[derive(Clone, Debug, Default)]
pub struct NeuronNames {
    listed: Vec<String>,
    listed_index: HashMap<String, u32>,
    /// (name, first index, count).
    populations: Vec<(String, u32, u32)>,
}

impl NeuronNames {
    /// Neuron `i`'s name.
    pub fn name(&self, i: u32) -> Option<Cow<'_, str>> {
        if let Some(name) = self.listed.get(i as usize) {
            return Some(Cow::Borrowed(name));
        }
        self.populations
            .iter()
            .find(|&&(_, first, count)| (first..first + count).contains(&i))
            .map(|(name, first, _)| Cow::Owned(format!("{name}-{}", i - first)))
    }

    /// The index of the neuron with this name.
    pub fn index(&self, name: &str) -> Option<u32> {
        if let Some(&i) = self.listed_index.get(name) {
            return Some(i);
        }
        let (population, k) = name.rsplit_once('-')?;
        // Only the canonical spelling ("exc-7", not "exc-07" or "exc-+7").
        if k.is_empty() || !k.bytes().all(|b| b.is_ascii_digit()) || (k.len() > 1 && k.starts_with('0')) {
            return None;
        }
        let k: u32 = k.parse().ok()?;
        self.populations
            .iter()
            .find(|(p, _, count)| p == population && k < *count)
            .map(|(_, first, _)| first + k)
    }

    /// How many neurons.
    pub fn len(&self) -> usize {
        self.listed.len() + self.populations.iter().map(|p| p.2 as usize).sum::<usize>()
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}
