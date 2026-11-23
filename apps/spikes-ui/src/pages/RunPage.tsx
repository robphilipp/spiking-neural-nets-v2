import PauseIcon from "@mui/icons-material/Pause";
import PlayArrowIcon from "@mui/icons-material/PlayArrow";
import SkipNextIcon from "@mui/icons-material/SkipNext";
import StopIcon from "@mui/icons-material/Stop";
import BuildIcon from "@mui/icons-material/Build";
import BoltIcon from "@mui/icons-material/Bolt";
import {
  Alert,
  Box,
  Button,
  Chip,
  FormControlLabel,
  Paper,
  Slider,
  Stack,
  Switch,
  TextField,
  Typography,
} from "@mui/material";
import { useTheme } from "@mui/material/styles";
import { Lod } from "@spikes/client";
import { type ReactNode, useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  AxisLocation,
  Chart,
  ContinuousAxis,
  defaultLineStyle,
  OrdinalAxis,
  RasterPlot,
  ScatterPlot,
  type SeriesLineStyle,
} from "stream-charts";
import { useConnection } from "../connection";
import { useAppDispatch, useAppSelector } from "../hooks";
import { LiveView, type LiveStats } from "../live";
import { backgroundChanged, built, runSettingsChanged, simTime } from "../store";
import { chartColors } from "../theme";

/** The width of an element, following resizes. A callback ref, so it starts measuring whenever the element appears. */
function useWidth(): [(element: HTMLElement | null) => void, number] {
  const [width, setWidth] = useState(600);
  const observer = useRef<ResizeObserver | null>(null);
  const ref = useCallback((element: HTMLElement | null) => {
    observer.current?.disconnect();
    if (!element) return;
    observer.current = new ResizeObserver(([entry]) => entry && setWidth(Math.max(200, Math.floor(entry.contentRect.width))));
    observer.current.observe(element);
  }, []);
  return [ref, width];
}

/** Speed on a log scale: slider value v means 2^v simulated seconds per wall second. */
const speedOf = (v: number) => 2 ** v;
const speedLabel = (v: number) => (v >= 0 ? `×${speedOf(v)}` : `×1/${2 ** -v}`);

/** How many raster rows at most. */
const MAX_ROWS = 300;

function Stat({ label, value, warn }: { label: string; value: ReactNode; warn?: boolean }) {
  return (
    <Box>
      <Typography variant="caption" color="text.secondary">
        {label}
      </Typography>
      <Typography variant="body2" color={warn ? "warning.main" : "text.primary"} sx={{ fontVariantNumeric: "tabular-nums" }}>
        {value}
      </Typography>
    </Box>
  );
}

export function RunPage() {
  const theme = useTheme();
  const colors = chartColors(theme);
  const { client } = useConnection();
  const dispatch = useAppDispatch();
  const session = useAppSelector((s) => s.session);
  const { id, network, state } = session;

  // Run settings belong to the project (saved in project.toml).
  const run = useAppSelector((s) => s.project.project.run);
  const background = useAppSelector((s) => s.project.project.background);
  const { shards, seed } = run;
  const asFastAsPossible = run.speed === null;
  const [lastSpeedLog, setLastSpeedLog] = useState(0);
  const speedLog = run.speed ? Math.round(Math.log2(run.speed)) : lastSpeedLog;
  const setSpeedLog = (v: number) => {
    setLastSpeedLog(v);
    dispatch(runSettingsChanged({ speed: speedOf(v) }));
  };
  const setAsFastAsPossible = (on: boolean) => dispatch(runSettingsChanged({ speed: on ? null : speedOf(speedLog) }));
  const [durationText, setDurationText] = useState(run.durationS === null ? "" : String(run.durationS));
  useEffect(() => setDurationText(run.durationS === null ? "" : String(run.durationS)), [run.durationS]);
  const [filter, setFilter] = useState("");
  const [stimulus, setStimulus] = useState({ prefix: "", mv: 10 });
  const [backgroundDraft, setBackgroundDraft] = useState(background ?? { rateHz: 5, mv: 12 });
  const [error, setError] = useState<string | null>(null);
  const [live, setLive] = useState<LiveView | null>(null);
  const [stats, setStats] = useState<LiveStats | null>(null);
  const [rasterRef, width] = useWidth();

  const selection = useMemo(() => {
    const neurons = network?.neurons ?? [];
    return neurons.filter((n) => !filter || n.name.startsWith(filter)).slice(0, MAX_ROWS);
  }, [network, filter]);

  // Live views follow the built network and the raster selection.
  useEffect(() => {
    if (!client || id === null || !network) return;
    let view: LiveView | null = null;
    let cancelled = false;
    LiveView.attach(client, id, selection, network.neurons.length)
      .then((v) => {
        if (cancelled) return void v.dispose();
        view = v;
        setLive(v);
        v.stats.subscribe((s) => {
          setStats(s);
          dispatch(simTime(s.simMs));
        });
      })
      .catch((e: Error) => setError(e.message));
    return () => {
      cancelled = true;
      setLive(null);
      void view?.dispose();
    };
  }, [client, id, network, selection, dispatch]);

  const act = (f: () => Promise<unknown>) => async () => {
    setError(null);
    try {
      await f();
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    }
  };
  const speed = asFastAsPossible ? null : speedOf(speedLog);

  if (id === null) {
    return <Alert severity="info">Open or write a network on the Network page and apply it first.</Alert>;
  }
  const running = state === "running" || state === "paused";
  // Row names are readable up to ~40 rows; beyond that they're hidden (filter the rows, or use the tooltip).
  const showNames = selection.length <= 40;
  // Room for the longest row name (≈ 7 px a character at the axis font), so both charts line up.
  const labelWidth = showNames ? 24 + 7 * Math.max(4, ...selection.map((n) => n.name.length)) : 48;
  const rasterHeight = Math.min(640, Math.max(200, 60 + selection.length * (showNames ? 14 : 3)));
  const lineStyles = new Map<string, SeriesLineStyle>(
    [...selection.map((n) => n.name), "rate"].map((name) => [
      name,
      { ...defaultLineStyle(), color: colors.series, highlightColor: colors.highlight },
    ]),
  );

  return (
    <Stack spacing={1.5}>
      <Paper variant="outlined" sx={{ p: 1.5 }}>
        <Stack sx={{ alignItems: "center", flexWrap: "wrap" }} direction="row" spacing={1.5} useFlexGap>
          <TextField label="Shards" type="number" value={shards} sx={{ width: 90 }} onChange={(e) => dispatch(runSettingsChanged({ shards: Math.max(1, Math.round(Number(e.target.value)) || 1) }))} />
          <TextField label="Seed" type="number" value={seed} sx={{ width: 90 }} onChange={(e) => dispatch(runSettingsChanged({ seed: Math.round(Number(e.target.value)) || 0 }))} />
          <Button
            variant="outlined"
            startIcon={<BuildIcon />}
            disabled={!client || running}
            data-testid="build"
            onClick={act(async () => {
              await client!.build(id, { shards, seed });
              const net = await client!.getNetwork(id);
              if (background) {
                const targets = net.neurons.map((n) => n.index);
                await client!.addInput(id, { type: "poisson", targets, rateHz: background.rateHz * targets.length, mv: background.mv, durationMs: null });
              }
              dispatch(built(net));
            })}
          >
            Build
          </Button>
          <Box sx={{ flex: 1 }} />
          {network && (
            <Typography variant="body2" color="text.secondary">
              {network.neurons.length.toLocaleString()} neurons · {network.synapses.length.toLocaleString()} synapses
            </Typography>
          )}
          <Chip size="small" label={state ?? "—"} color={state === "running" ? "success" : state === "paused" ? "warning" : "default"} data-testid="state" />
        </Stack>
      </Paper>

      <Paper variant="outlined" sx={{ p: 1.5 }}>
        <Stack sx={{ alignItems: "center", flexWrap: "wrap" }} direction="row" spacing={1.5} useFlexGap>
          {state === "paused" ? (
            <Button variant="contained" startIcon={<PlayArrowIcon />} onClick={act(() => client!.resume(id))}>
              Resume
            </Button>
          ) : (
            <Button
              variant="contained"
              startIcon={<PlayArrowIcon />}
              disabled={!network || running}
              data-testid="start"
              onClick={act(() => client!.start(id, { speed, durationMs: run.durationS === null ? null : run.durationS * 1000 }))}
            >
              Start
            </Button>
          )}
          <Button startIcon={<PauseIcon />} disabled={state !== "running"} onClick={act(() => client!.pause(id))} data-testid="pause">
            Pause
          </Button>
          <Button startIcon={<SkipNextIcon />} disabled={!network || state === "running"} onClick={act(() => client!.step(id, 10))}>
            Step 10 ms
          </Button>
          <Button startIcon={<StopIcon />} disabled={!running} onClick={act(() => client!.stop(id))} data-testid="stop">
            Stop
          </Button>
          <TextField
            label="Run for (s)"
            value={durationText}
            sx={{ width: 110 }}
            placeholder="until stopped"
            onChange={(e) => setDurationText(e.target.value.replace(/[^\d.]/g, ""))}
            onBlur={() => dispatch(runSettingsChanged({ durationS: Number(durationText) > 0 ? Number(durationText) : null }))}
          />
          <Box sx={{ width: 220, px: 1 }}>
            <Typography variant="caption" color="text.secondary">
              Speed {asFastAsPossible ? "max" : speedLabel(speedLog)}
            </Typography>
            <Slider
              size="small"
              min={-6}
              max={6}
              step={1}
              value={speedLog}
              disabled={asFastAsPossible}
              valueLabelDisplay="auto"
              valueLabelFormat={speedLabel}
              onChange={(_, v) => setSpeedLog(v as number)}
              onChangeCommitted={(_, v) => running && void act(() => client!.setSpeed(id, speedOf(v as number)))()}
            />
          </Box>
          <FormControlLabel
            control={
              <Switch
                checked={asFastAsPossible}
                onChange={(e) => {
                  setAsFastAsPossible(e.target.checked);
                  if (running) void act(() => client!.setSpeed(id, e.target.checked ? null : speedOf(speedLog)))();
                }}
              />
            }
            label="As fast as possible"
          />
        </Stack>
        <Stack direction="row" spacing={1.5} sx={{ alignItems: "center", mt: 1.5 }}>
          <TextField label="Stimulate neurons starting with" value={stimulus.prefix} sx={{ width: 240 }} onChange={(e) => setStimulus({ ...stimulus, prefix: e.target.value })} />
          <TextField label="mV" type="number" value={stimulus.mv} sx={{ width: 80 }} onChange={(e) => setStimulus({ ...stimulus, mv: Number(e.target.value) })} />
          <Button
            startIcon={<BoltIcon />}
            disabled={!network}
            data-testid="stimulate"
            onClick={act(async () => {
              const targets = network!.neurons.filter((n) => n.name.startsWith(stimulus.prefix)).map((n) => n.index);
              if (!targets.length) throw new Error(`no neuron names start with '${stimulus.prefix}'`);
              await client!.stimulate(id, targets, stimulus.mv);
            })}
          >
            Stimulate
          </Button>
          <Box sx={{ flex: 1 }} />
          <FormControlLabel
            control={
              <Switch
                checked={background !== null}
                onChange={(e) => dispatch(backgroundChanged(e.target.checked ? backgroundDraft : null))}
                data-testid="background"
              />
            }
            label="Background input"
            title="Poisson input to every neuron, applied when the network is built"
          />
          <TextField
            label="Hz per neuron"
            type="number"
            value={backgroundDraft.rateHz}
            sx={{ width: 120 }}
            onChange={(e) => {
              const next = { ...backgroundDraft, rateHz: Math.max(0.001, Number(e.target.value)) };
              setBackgroundDraft(next);
              if (background) dispatch(backgroundChanged(next));
            }}
          />
          <TextField
            label="mV"
            type="number"
            value={backgroundDraft.mv}
            sx={{ width: 80 }}
            onChange={(e) => {
              const next = { ...backgroundDraft, mv: Number(e.target.value) };
              setBackgroundDraft(next);
              if (background) dispatch(backgroundChanged(next));
            }}
          />
        </Stack>
      </Paper>

      {error && (
        <Alert severity="error" onClose={() => setError(null)}>
          {error}
        </Alert>
      )}

      <Paper variant="outlined" sx={{ p: 1.5 }}>
        <Stack sx={{ flexWrap: "wrap" }} direction="row" spacing={3} useFlexGap data-testid="timing">
          <Stat label="Simulated time" value={`${((stats?.simMs ?? session.simMs) / 1000).toFixed(3)} s`} />
          <Stat label="Speed" value={!stats || stats.speed === 0 ? "—" : Number.isFinite(stats.speed) ? `×${stats.speed.toPrecision(3)}` : "max"} />
          <Stat label="Behind the clock" value={`${(stats?.lagMs ?? 0).toFixed(1)} ms`} warn={(stats?.lagMs ?? 0) > 20} />
          <Stat label="Frames" value={`${stats?.framesPerS ?? 0}/s`} />
          <Stat label="Spikes" value={`${(stats?.spikesPerS ?? 0).toLocaleString()}/s`} />
          <Stat label="Detail" value={stats ? (["exact", "counts", "cells"][stats.lod] ?? "?") : "—"} warn={!!stats && stats.lod !== Lod.Exact} />
          <Stat label="Dropped" value={`${stats?.droppedFrames ?? 0} frames · ${stats?.droppedEvents ?? 0} events`} warn={!!stats && stats.droppedFrames + stats.droppedEvents > 0} />
        </Stack>
      </Paper>

      <Paper variant="outlined" sx={{ p: 1.5 }}>
        <Stack direction="row" spacing={1.5} sx={{ alignItems: "center", mb: 1 }}>
          <Typography variant="subtitle2" sx={{ flex: 1 }}>
            Spikes {selection.length < (network?.neurons.length ?? 0) ? `(${selection.length} of ${network?.neurons.length})` : ""}
          </Typography>
          <TextField label="Show neurons starting with" value={filter} sx={{ width: 240 }} onChange={(e) => setFilter(e.target.value)} />
        </Stack>
        <Box ref={rasterRef} data-testid="raster">
          {live && selection.length > 0 && (
            <Chart
              chartId={1}
              width={width}
              height={rasterHeight}
              dataSource={live.raster}
              color={colors.color}
              backgroundColor={colors.backgroundColor}
              seriesStyles={lineStyles}
              windowingTime={50}
              dataUpdatePeriod={16}
              margin={{ left: labelWidth, right: 20, bottom: 48 }}
            >
              <ContinuousAxis axisId="x-axis-1" location={AxisLocation.Bottom} domain={[0, 5000]} label="t (ms)" />
              <OrdinalAxis axisId="y-axis-1" location={AxisLocation.Left} categories={selection.map((n) => n.name)}
                label={showNames ? "" : `${selection.length} neurons`}
                font={showNames ? undefined : { color: "transparent" }}
              />
              <RasterPlot spikeMargin={showNames ? 1 : 0} />
            </Chart>
          )}
        </Box>
        <Typography variant="subtitle2" sx={{ mt: 2, mb: 1 }}>
          Population rate (Hz per neuron)
        </Typography>
        <Box data-testid="rate">
          {live && (
            <Chart
              chartId={2}
              width={width}
              height={180}
              dataSource={live.rate}
              color={colors.color}
              backgroundColor={colors.backgroundColor}
              seriesStyles={lineStyles}
              windowingTime={50}
              dataUpdatePeriod={16}
              margin={{ left: labelWidth, right: 20, bottom: 48 }}
            >
              <ContinuousAxis axisId="x-axis-1" location={AxisLocation.Bottom} domain={[0, 5000]} label="t (ms)" />
              <ContinuousAxis axisId="y-axis-1" location={AxisLocation.Left} domain={[0, 10]} label="Hz" autoScale autoScaleMin={0} />
              <ScatterPlot />
            </Chart>
          )}
        </Box>
      </Paper>
    </Stack>
  );
}
