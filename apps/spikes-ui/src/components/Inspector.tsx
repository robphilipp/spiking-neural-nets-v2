import type { Network, Neuron, NeuronState, ProtocolClient } from "@spikes/client";
import { Link2, Unlink } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { AxisLocation, Chart, ContinuousAxis, defaultLineStyle, ScatterPlot, type SeriesLineStyle } from "stream-charts";
import { Choice, Field, Heading, NeuronPicker, Notice, Panel, Row, Stat } from "@/components/kit";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { CADENCE_MS, InspectView, type WeightSeries } from "../live";
import type { ChartColors } from "../theme";

/** Okabe–Ito: distinguishable for colour-blind viewers. */
const PALETTE = ["#0072B2", "#E69F00", "#009E73", "#CC79A7", "#56B4E9", "#D55E00", "#F0E442", "#999999"];
const MAX_PICKED = 6;

function Legend({ names }: { names: string[] }) {
  return (
    <div className="mb-1 flex flex-wrap gap-1">
      {names.map((n, i) => (
        <Badge key={n} variant="outline" className="border-2" style={{ borderColor: PALETTE[i % PALETTE.length] }}>
          {n}
        </Badge>
      ))}
    </div>
  );
}

function fmt(v: number | null | undefined, digits = 2): string {
  return v === null || v === undefined || !Number.isFinite(v) ? "—" : v.toFixed(digits);
}

/** Inspect neurons: their state, membrane potential and incoming weights, live; add and prune connections. */
export function Inspector(props: {
  client: ProtocolClient;
  session: number;
  network: Network;
  width: number;
  labelWidth: number;
  colors: ChartColors;
  windowMs: number;
  /** The neurons being inspected (shared with the 3D view). */
  picked: Neuron[];
  onPicked: (neurons: Neuron[]) => void;
  /** A replay: recorded traces only (no neuron state, no connecting). */
  replay?: boolean;
}) {
  const { client, session, network, width, labelWidth, colors, windowMs, picked } = props;
  const setPicked = (value: Neuron[] | ((p: Neuron[]) => Neuron[])) =>
    props.onPicked(typeof value === "function" ? value(props.picked) : value);
  const [view, setView] = useState<InspectView | null>(null);
  const [state, setState] = useState<NeuronState | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [from, setFrom] = useState<Neuron | null>(null);
  const [weight, setWeight] = useState(1);
  const [learning, setLearning] = useState("flat");
  const first = picked[0];
  const names = useMemo(() => new Map(network.neurons.map((n) => [n.index, n.name])), [network]);

  // Keep the picked neurons when the network changes (connect/prune), dropping any that no longer exist.
  useEffect(() => setPicked((p) => p.filter((n) => names.get(n.index) === n.name)), [names]);

  // A big network doesn't list its synapses: then the first picked neuron's own (from its state) are charted.
  const listed = network.synapses.length === network.synapseCount;
  const ownSynapses = !listed && first && state ? state.synapses.length : 0;
  const incoming: WeightSeries[] = useMemo(() => {
    const picks = new Set(picked.map((n) => n.index));
    const seen = new Map<string, number>();
    const source = listed
      ? network.synapses
      : first && state
        ? state.synapses.map((s, i) => ({ post: first.index, synapse: i, pre: s.pre, weight: s.weight }))
        : [];
    return source
      .filter((s) => picks.has(s.post))
      .map((s) => {
        const base = `${names.get(s.pre) ?? s.pre} → ${names.get(s.post) ?? s.post}`;
        const count = (seen.get(base) ?? 0) + 1;
        seen.set(base, count);
        return { post: s.post, synapse: s.synapse, weight: s.weight, name: count > 1 ? `${base} #${count}` : base };
      });
    // (Only when the set of synapses changes, not at every state refresh.)
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [network, picked, names, listed, first?.index, ownSynapses]);

  // The live view follows the picked neurons and their synapses.
  useEffect(() => {
    if (!picked.length) return;
    let v: InspectView | null = null;
    let cancelled = false;
    InspectView.attach(client, session, picked, incoming)
      .then((created) => {
        if (cancelled) return void created.dispose();
        v = created;
        setView(created);
      })
      .catch((e: Error) => setError(e.message));
    return () => {
      cancelled = true;
      setView(null);
      void v?.dispose();
    };
  }, [client, session, picked, incoming]);

  // The first picked neuron's state, twice a second (a replay has none: its neurons were recorded).
  useEffect(() => {
    if (!first || props.replay) return setState(null);
    let stop = false;
    const poll = () =>
      client
        .queryNeuron(session, first.index)
        .then((s) => !stop && setState(s))
        .catch(() => {});
    void poll();
    const timer = setInterval(poll, 500);
    return () => {
      stop = true;
      clearInterval(timer);
    };
  }, [client, session, first, props.replay]);

  const act = (f: () => Promise<unknown>) => async () => {
    setError(null);
    try {
      await f();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };

  const styles = (series: string[]) =>
    new Map<string, SeriesLineStyle>(
      series.map((name, i) => [name, { ...defaultLineStyle(), color: PALETTE[i % PALETTE.length]!, highlightColor: colors.highlight, lineWidth: 1.5 }]),
    );
  const membraneNames = picked.map((n) => n.name);
  const weightNames = incoming.map((s) => s.name);

  return (
    <Panel data-testid="inspector">
      <div className="flex items-end gap-3">
        <Heading className="pb-2">Inspect</Heading>
        <NeuronPicker
          className="flex-1"
          label={`Neurons (up to ${MAX_PICKED})`}
          options={network.neurons}
          value={picked}
          max={MAX_PICKED}
          onChange={(value) => setPicked(value.slice(0, MAX_PICKED))}
          testId="inspect-picker"
        />
      </div>
      {error && (
        <Notice tone="error" onClose={() => setError(null)}>
          {error}
        </Notice>
      )}
      {first && (
        <div className="flex flex-col gap-3">
          {!props.replay && (
            <div className="flex flex-wrap gap-x-8 gap-y-2" data-testid="neuron-state">
              {[
                ["Neuron", first.name],
                ["Kind", first.kind.replace("_", " ")],
                ["Membrane", `${fmt(state?.membraneMv)} mV`],
                ["Threshold shift", `${fmt(state?.intrinsicMv)} mV`],
                // The rate estimate is kept only for homeostatic plasticity (HIP); otherwise it stays 0.
                ["Rate (homeostasis)", state?.rateHz ? `${fmt(state.rateHz, 1)} Hz` : "—"],
                ["Spikes", String(state?.spikes ?? "—")],
                ["Last spike", state?.lastSpikeMs == null ? "—" : `${fmt(state.lastSpikeMs, 1)} ms`],
                ["Tonic", state?.tonic ? "yes" : "no"],
              ].map(([label, value]) => (
                <Stat key={label} label={label!} value={value} />
              ))}
            </div>
          )}

          <div>
            <Heading className="mb-1">Membrane potential (mV)</Heading>
            <Legend names={membraneNames} />
            {view && (
              <Chart key={windowMs} chartId={3} width={width} height={200} dataSource={view.membrane} color={colors.color} backgroundColor={colors.backgroundColor} seriesStyles={styles(membraneNames)} windowingTime={50} dataUpdatePeriod={16} margin={{ left: labelWidth, right: 20, bottom: 48 }}>
                <ContinuousAxis axisId="x-axis-1" location={AxisLocation.Bottom} domain={[0, windowMs]} label="t (ms)" />
                <ContinuousAxis axisId="y-axis-1" location={AxisLocation.Left} domain={[-1, 15]} label="mV" autoScale />
                <ScatterPlot />
              </Chart>
            )}
          </div>

          <div>
            <Heading className="mb-1">Incoming weights</Heading>
            {weightNames.length ? <Legend names={weightNames} /> : <p className="text-sm text-muted-foreground">No incoming connections.</p>}
            {view && weightNames.length > 0 && (
              <Chart key={windowMs} chartId={4} width={width} height={180} dataSource={view.weights} color={colors.color} backgroundColor={colors.backgroundColor} seriesStyles={styles(weightNames)} windowingTime={CADENCE_MS} dataUpdatePeriod={CADENCE_MS} margin={{ left: labelWidth, right: 20, bottom: 48 }}>
                <ContinuousAxis axisId="x-axis-1" location={AxisLocation.Bottom} domain={[0, windowMs]} label="t (ms)" />
                <ContinuousAxis axisId="y-axis-1" location={AxisLocation.Left} domain={[0, 2]} label="weight" autoScale />
                <ScatterPlot />
              </Chart>
            )}
          </div>

          {!props.replay && (
            <>
              <Table data-testid="synapses">
                <TableHeader>
                  <TableRow>
                    <TableHead>From</TableHead>
                    <TableHead>To</TableHead>
                    <TableHead className="text-right">Weight</TableHead>
                    <TableHead className="text-right">Eligibility</TableHead>
                    <TableHead />
                  </TableRow>
                </TableHeader>
                <TableBody>
                  {(state?.synapses ?? [])
                    .map((s, i) => ({ ...s, index: i }))
                    .filter((s) => names.has(s.pre))
                    .map((s) => (
                      <TableRow key={s.index}>
                        <TableCell>{names.get(s.pre)}</TableCell>
                        <TableCell>{first.name}</TableCell>
                        <TableCell className="text-right tabular-nums">{fmt(s.weight, 4)}</TableCell>
                        <TableCell className="text-right tabular-nums">{fmt(s.eligibility, 4)}</TableCell>
                        <TableCell className="text-right">
                          <Button variant="ghost" size="icon-xs" title="Prune this connection" aria-label="Prune this connection" onClick={act(() => client.prune(session, s.pre, first.index))}>
                            <Unlink />
                          </Button>
                        </TableCell>
                      </TableRow>
                    ))}
                </TableBody>
              </Table>

              <Row>
                <span className="pb-2 text-sm">Connect to {first.name} from</span>
                <NeuronPicker label="Sender" width={220} options={network.neurons} value={from ? [from] : []} max={1} onChange={(v) => setFrom(v[0] ?? null)} />
                <Field label="Weight" type="number" width={100} value={weight} onChange={(e) => setWeight(Number(e.target.value))} />
                <Choice
                  label="Learning"
                  width={150}
                  value={learning}
                  onChange={setLearning}
                  options={(network.learningRules.length ? network.learningRules : ["flat"]).map((l) => ({ value: l, label: l }))}
                />
                <Button variant="outline" disabled={!from} onClick={act(() => client.connect(session, from!.index, first.index, weight, { learning }))} data-testid="connect">
                  <Link2 data-icon="inline-start" />
                  Connect
                </Button>
              </Row>
            </>
          )}
        </div>
      )}
    </Panel>
  );
}
