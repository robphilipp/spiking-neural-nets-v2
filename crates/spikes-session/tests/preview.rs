//! A description's network, built just to look at.

#[test]
fn a_preview_counts_each_neurons_connections_and_shows_its_pin() {
    let text = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../test-data/examples/pinned-nodes.boo"
    ))
    .unwrap();
    let p = spikes_session::preview(&text).unwrap();
    assert_eq!(p.network.neurons.len(), 220);
    let index = |name: &str| p.network.neurons.index(name).unwrap() as usize;
    // Each cortex neuron: 5 from the sensors and 10 from the cortex; all excitatory.
    assert_eq!(p.incoming[index("cortex-0")], (15, 0));
    assert_eq!(p.incoming[index("sensors-0")], (0, 0));
    let from_sensors: u32 = (0..20).map(|i| p.outgoing[index(&format!("sensors-{i}"))].0).sum();
    assert_eq!(from_sensors, 200 * 5);
    let total: u32 = p.incoming.iter().map(|c| c.0).sum();
    assert_eq!(u64::from(total), p.network.synapse_count);
    assert_eq!(p.network.synapses.len() as u64, p.network.synapse_count);
    assert_eq!(p.pinned[index("sensors-3")].as_deref(), Some("edge"));
    assert_eq!(p.pinned[index("cortex-3")], None);

    // Parse errors come back as such.
    assert!(matches!(
        spikes_session::preview("(neurons=[").unwrap_err(),
        spikes_session::SessionError::Parse(_)
    ));
}
