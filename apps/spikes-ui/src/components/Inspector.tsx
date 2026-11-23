import AddLinkIcon from "@mui/icons-material/AddLink";
import LinkOffIcon from "@mui/icons-material/LinkOff";
import {
  Alert,
  Autocomplete,
  Box,
  Button,
  Chip,
  IconButton,
  MenuItem,
  Paper,
  Stack,
  Table,
  TableBody,
  TableCell,
  TableHead,
  TableRow,
  TextField,
  Typography,
} from "@mui/material";
import type { Network, Neuron, NeuronState, ProtocolClient } from "@spikes/client";
import { useEffect, useMemo, useState } from "react";
import { AxisLocation, Chart, ContinuousAxis, defaultLineStyle, ScatterPlot, type SeriesLineStyle } from "stream-charts";
import { InspectView, type WeightSeries } from "../live";
import type { ChartColors } from "../theme";

/** Okabe–Ito: distinguishable for colour-blind viewers. */
const PALETTE = ["#0072B2", "#E69F00", "#009E73", "#CC79A7", "#56B4E9", "#D55E00", "#F0E442", "#999999"];
const MAX_PICKED = 6;

function Legend({ names }: { names: string[] }) {
  return (
    <Stack direction="row" spacing={1} sx={{ flexWrap: "wrap", mb: 0.5 }} useFlexGap>
      {names.map((n, i) => (
        <Chip key={n} size="small" variant="outlined" label={n} sx={{ borderColor: PALETTE[i % PALETTE.length], borderWidth: 2 }} />
      ))}
    </Stack>
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
}) {
  const { client, session, network, width, labelWidth, colors, windowMs } = props;
  const [picked, setPicked] = useState<Neuron[]>([]);
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

  const incoming: WeightSeries[] = useMemo(() => {
    const picks = new Set(picked.map((n) => n.index));
    const seen = new Map<string, number>();
    return network.synapses
      .filter((s) => picks.has(s.post))
      .map((s) => {
        const base = `${names.get(s.pre) ?? s.pre} → ${names.get(s.post) ?? s.post}`;
        const count = (seen.get(base) ?? 0) + 1;
        seen.set(base, count);
        return { post: s.post, synapse: s.synapse, weight: s.weight, name: count > 1 ? `${base} #${count}` : base };
      });
  }, [network, picked, names]);

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

  // The first picked neuron's state, twice a second.
  useEffect(() => {
    if (!first) return setState(null);
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
  }, [client, session, first]);

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
    <Paper variant="outlined" sx={{ p: 1.5 }} data-testid="inspector">
      <Stack direction="row" spacing={1.5} sx={{ alignItems: "center", mb: 1.5 }}>
        <Typography variant="subtitle2">Inspect</Typography>
        <Autocomplete
          multiple
          size="small"
          sx={{ flex: 1 }}
          options={network.neurons}
          value={picked}
          getOptionLabel={(n) => n.name}
          isOptionEqualToValue={(a, b) => a.index === b.index}
          getOptionDisabled={() => picked.length >= MAX_PICKED}
          onChange={(_, value) => setPicked(value.slice(0, MAX_PICKED))}
          renderInput={(params) => <TextField {...params} label={`Neurons (up to ${MAX_PICKED})`} />}
          data-testid="inspect-picker"
        />
      </Stack>
      {error && (
        <Alert severity="error" sx={{ mb: 1 }} onClose={() => setError(null)}>
          {error}
        </Alert>
      )}
      {first && (
        <Stack spacing={1.5}>
          <Stack direction="row" spacing={3} sx={{ flexWrap: "wrap" }} useFlexGap data-testid="neuron-state">
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
              <Box key={label}>
                <Typography variant="caption" color="text.secondary">
                  {label}
                </Typography>
                <Typography variant="body2" sx={{ fontVariantNumeric: "tabular-nums" }}>
                  {value}
                </Typography>
              </Box>
            ))}
          </Stack>

          <Box>
            <Typography variant="subtitle2" gutterBottom>
              Membrane potential (mV)
            </Typography>
            <Legend names={membraneNames} />
            {view && (
              <Chart chartId={3} width={width} height={200} dataSource={view.membrane} color={colors.color} backgroundColor={colors.backgroundColor} seriesStyles={styles(membraneNames)} windowingTime={50} dataUpdatePeriod={16} margin={{ left: labelWidth, right: 20, bottom: 48 }}>
                <ContinuousAxis axisId="x-axis-1" location={AxisLocation.Bottom} domain={[0, windowMs]} label="t (ms)" />
                <ContinuousAxis axisId="y-axis-1" location={AxisLocation.Left} domain={[-1, 15]} label="mV" autoScale />
                <ScatterPlot />
              </Chart>
            )}
          </Box>

          <Box>
            <Typography variant="subtitle2" gutterBottom>
              Incoming weights
            </Typography>
            {weightNames.length ? <Legend names={weightNames} /> : <Typography variant="body2" color="text.secondary">No incoming connections.</Typography>}
            {view && weightNames.length > 0 && (
              <Chart chartId={4} width={width} height={180} dataSource={view.weights} color={colors.color} backgroundColor={colors.backgroundColor} seriesStyles={styles(weightNames)} windowingTime={50} dataUpdatePeriod={16} margin={{ left: labelWidth, right: 20, bottom: 48 }}>
                <ContinuousAxis axisId="x-axis-1" location={AxisLocation.Bottom} domain={[0, windowMs]} label="t (ms)" />
                <ContinuousAxis axisId="y-axis-1" location={AxisLocation.Left} domain={[0, 2]} label="weight" autoScale />
                <ScatterPlot />
              </Chart>
            )}
          </Box>

          <Table size="small" data-testid="synapses">
            <TableHead>
              <TableRow>
                <TableCell>From</TableCell>
                <TableCell>To</TableCell>
                <TableCell align="right">Weight</TableCell>
                <TableCell align="right">Eligibility</TableCell>
                <TableCell />
              </TableRow>
            </TableHead>
            <TableBody>
              {(state?.synapses ?? [])
                .map((s, i) => ({ ...s, index: i }))
                .filter((s) => names.has(s.pre))
                .map((s) => (
                  <TableRow key={s.index}>
                    <TableCell>{names.get(s.pre)}</TableCell>
                    <TableCell>{first.name}</TableCell>
                    <TableCell align="right" sx={{ fontVariantNumeric: "tabular-nums" }}>
                      {fmt(s.weight, 4)}
                    </TableCell>
                    <TableCell align="right" sx={{ fontVariantNumeric: "tabular-nums" }}>
                      {fmt(s.eligibility, 4)}
                    </TableCell>
                    <TableCell align="right">
                      <IconButton size="small" title="Prune this connection" onClick={act(() => client.prune(session, s.pre, first.index))}>
                        <LinkOffIcon fontSize="small" />
                      </IconButton>
                    </TableCell>
                  </TableRow>
                ))}
            </TableBody>
          </Table>

          <Stack direction="row" spacing={1.5} sx={{ alignItems: "center", flexWrap: "wrap" }} useFlexGap>
            <Typography variant="body2">Connect to {first.name} from</Typography>
            <Autocomplete
              size="small"
              sx={{ width: 220 }}
              options={network.neurons}
              value={from}
              getOptionLabel={(n) => n.name}
              isOptionEqualToValue={(a, b) => a.index === b.index}
              onChange={(_, value) => setFrom(value)}
              renderInput={(params) => <TextField {...params} label="Sender" />}
            />
            <TextField label="Weight" type="number" value={weight} sx={{ width: 100 }} onChange={(e) => setWeight(Number(e.target.value))} />
            <TextField select label="Learning" value={learning} sx={{ width: 150 }} onChange={(e) => setLearning(e.target.value)}>
              {(network.learningRules.length ? network.learningRules : ["flat"]).map((l) => (
                <MenuItem key={l} value={l}>
                  {l}
                </MenuItem>
              ))}
            </TextField>
            <Button startIcon={<AddLinkIcon />} disabled={!from} onClick={act(() => client.connect(session, from!.index, first.index, weight, { learning }))} data-testid="connect">
              Connect
            </Button>
          </Stack>
        </Stack>
      )}
    </Paper>
  );
}
