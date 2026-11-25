import { Lod, type Neuron, type SensorInfo } from "@spikes/client";
import { Hammer, Pause, Play, Save, SkipForward, Square, X, Zap } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
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
import { Choice, Field, Heading, Notice, Panel, Row, Stat, StateBadge, Toggle } from "@/components/kit";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Slider } from "@/components/ui/slider";
import { ClusterPanel } from "../components/ClusterPanel";
import { Inspector } from "../components/Inspector";
import { Modulators } from "../components/Modulators";
import { Network3D } from "../components/Network3D";
import { WeightsPanel } from "../components/WeightsPanel";
import { useConnection } from "../connection";
import { stopInput, useActiveInput } from "../inputStreams";
import { ChartControls } from "../components/ChartControls";
import { useChartSettings } from "../chartSettings";
import { connectLive, useLive } from "../live";
import { useBehavior } from "../useBehavior";
import { backgroundChanged, built, runSettingsChanged, simTime, useProject, useSession, useSettings } from "../store";
import { useChartColors } from "../theme";

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

export function RunPage() {
  const colors = useChartColors();
  const { client } = useConnection();
  const session = useSession();
  const { id, network, state } = session;

  // Run settings belong to the project (saved in project.toml).
  const run = useProject((s) => s.project.run);
  const background = useProject((s) => s.project.background);
  const { shards, seed } = run;
  const asFastAsPossible = run.speed === null;
  const [lastSpeedLog, setLastSpeedLog] = useState(0);
  const speedLog = run.speed ? Math.round(Math.log2(run.speed)) : lastSpeedLog;
  const setSpeedLog = (v: number) => {
    setLastSpeedLog(v);
    runSettingsChanged({ speed: speedOf(v) });
  };
  const setAsFastAsPossible = (on: boolean) => runSettingsChanged({ speed: on ? null : speedOf(speedLog) });
  const [durationText, setDurationText] = useState(run.durationS === null ? "" : String(run.durationS));
  useEffect(() => setDurationText(run.durationS === null ? "" : String(run.durationS)), [run.durationS]);
  const [filter, setFilter] = useState("");
  /** Neurons selected in the 3D view: the raster's rows, instead of the name filter. */
  const [region, setRegion] = useState<number[] | null>(null);
  const [picked, setPicked] = useState<Neuron[]>([]);
  const [show3d, setShow3d] = useState(true);
  const [stimulus, setStimulus] = useState({ prefix: "", mv: 10 });
  const [backgroundDraft, setBackgroundDraft] = useState(background ?? { rateHz: 5, mv: 12, prefix: "" });
  const [windowMs, setWindowMs] = useState(1000);
  const [error, setError] = useState<string | null>(null);
  /** The last save (see the Recordings page's saved runs). */
  const [savedNote, setSavedNote] = useState<string | null>(null);
  /** Building (and applying background input): nothing else may start meanwhile. */
  const [building, setBuilding] = useState(false);
  const nodesSetting = useSettings((s) => s.nodes);
  /** The node agents the network was last built on (empty: the engine alone). */
  const [builtNodes, setBuiltNodes] = useState<string[]>([]);
  const [lockstep, setLockstep] = useState(true);
  /** Record the next run (to the engine's recordings folder; see the Recordings page). */
  const [record, setRecord] = useState(false);
  const replay = session.replayEndMs !== null;
  const activeInput = useActiveInput();
  const [seekMs, setSeekMs] = useState(0);
  const [seeking, setSeeking] = useState(false);
  // The seek slider shows where the replay is, except while it's being dragged.
  useEffect(() => {
    if (!seeking) setSeekMs(session.simMs);
  }, [session.simMs, seeking]);
  /** The background input the current network was built with (it's applied at Build). */
  const [builtBackground, setBuiltBackground] = useState<string | null>(() => JSON.stringify(background));
  const needsRebuild = network !== null && JSON.stringify(background) !== builtBackground;
  const [rasterRef, width] = useWidth();
  // Connected sensors (from this page, the Inputs page or any other client), refreshed every second.
  const [sensors, setSensors] = useState<SensorInfo[]>([]);
  useEffect(() => {
    if (!client || id === null || !network) return setSensors([]);
    let live = true;
    const poll = () =>
      client
        .listSensors(id)
        .then((list) => live && setSensors(list))
        .catch(() => {});
    void poll();
    const timer = setInterval(poll, 1000);
    return () => {
      live = false;
      clearInterval(timer);
    };
  }, [client, id, network]);
  // Which sensors sent data since the last refresh.
  const lastSamples = useRef(new Map<number, number>());
  const sensorActive = useMemo(() => {
    const active = new Set<number>();
    for (const x of sensors) {
      const before = lastSamples.current.get(x.id);
      if (before !== undefined && x.samples > before) active.add(x.id);
      lastSamples.current.set(x.id, x.samples);
    }
    return active;
  }, [sensors]);

  const selection = useMemo(() => {
    const neurons = network?.neurons ?? [];
    if (region) return region.slice(0, MAX_ROWS).map((i) => neurons[i]!).filter(Boolean);
    return neurons.filter((n) => !filter || n.name.startsWith(filter)).slice(0, MAX_ROWS);
  }, [network, filter, region]);
  // A new network (rebuild, or connections changed): forget selections that may not fit it.
  useEffect(() => {
    setRegion(null);
    setPicked((p) => p.filter((n) => network?.neurons[n.index]?.name === n.name));
  }, [network]);

  // The session's live view (it lives in a store, so its data outlives this page): one per session and build.
  const build = useSession((s) => s.build);
  const built_ = network !== null;
  useEffect(() => {
    if (client && id !== null && built_) connectLive(client, id, build, useSession.getState().network!.neurons.length);
    else useLive.getState().disconnect();
  }, [client, id, build, built_]);
  const live = useLive((s) => s.view);
  const liveError = useLive((s) => s.error);
  useEffect(() => void (liveError && setError(liveError)), [liveError]);
  // The raster shows the selection; what it showed of neurons still selected stays.
  useEffect(() => {
    live?.select(selection).catch((e: Error) => setError(e.message));
  }, [live, selection]);
  const raster = useBehavior(live?.raster);
  const stats = useBehavior(live?.stats);
  useEffect(() => {
    if (stats) simTime(stats.simMs);
  }, [stats]);
  const { bufferMs, updateMs, cadence } = useChartSettings();
  // The 3D view's tooltip asks for a neuron's state (a replay's neurons have none).
  const queryState = useMemo(
    () => (client && id !== null && !replay ? (neuron: number) => client.queryNeuron(id, neuron) : undefined),
    [client, id, replay],
  );

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
    return <Notice>Open or write a network on the Network page and apply it first.</Notice>;
  }
  const running = state === "running" || state === "paused";
  // The raster's rows: its data source's series (the selection, once the source follows it).
  const rows = raster?.seriesList().map((s) => s.name) ?? [];
  // Row names are readable up to ~40 rows; beyond that they're hidden (filter the rows, or use the tooltip).
  const showNames = rows.length <= 100;
  const rowHeight = rows.length <= 40 ? 14 : showNames ? 12 : 3;
  // Room for the longest row name (≈ 7 px a character at the axis font), so both charts line up.
  const labelWidth = showNames ? 24 + 7 * Math.max(4, ...rows.map((n) => n.length)) : 48;
  const rasterHeight = Math.max(200, 60 + rows.length * rowHeight);
  const lineStyles = new Map<string, SeriesLineStyle>(
    [...rows, "rate"].map((name) => [name, { ...defaultLineStyle(), color: colors.series, highlightColor: colors.highlight }]),
  );
  const setBackground = (next: typeof backgroundDraft) => {
    setBackgroundDraft(next);
    if (background) backgroundChanged(next);
  };

  return (
    <div className="flex flex-col gap-3">
      <Panel>
        <Row className="items-end">
          <Field label="Shards" type="number" width={90} value={shards} onChange={(e) => runSettingsChanged({ shards: Math.max(1, Math.round(Number(e.target.value)) || 1) })} />
          <Field label="Seed" type="number" width={90} value={seed} onChange={(e) => runSettingsChanged({ seed: Math.round(Number(e.target.value)) || 0 })} />
          <Button
            variant={needsRebuild && !running ? "default" : "outline"}
            className={needsRebuild && !running ? "bg-warning text-black hover:bg-warning/80" : undefined}
            disabled={!client || running || building}
            data-testid="build"
            onClick={act(async () => {
              setBuilding(true);
              try {
                const nodes = replay ? [] : nodesSetting.split(",").map((n) => n.trim()).filter(Boolean);
                await client!.build(id, { shards, seed, nodes });
                setBuiltNodes(nodes);
                const net = await client!.getNetwork(id);
                if (background && !replay) {
                  const targets = net.neurons.filter((n) => n.name.startsWith(background.prefix)).map((n) => n.index);
                  if (!targets.length) throw new Error(`background input: no neuron names start with '${background.prefix}'`);
                  await client!.addInput(id, { type: "poisson", targets, rateHz: background.rateHz * targets.length, mv: background.mv, durationMs: null });
                }
                built(net);
                setBuiltBackground(JSON.stringify(background));
              } finally {
                setBuilding(false);
              }
            })}
          >
            <Hammer data-icon="inline-start" />
            {building ? "Building…" : replay ? "Load" : needsRebuild && network ? "Rebuild to apply" : "Build"}
          </Button>
          <div className="flex-1" />
          {network && (
            <span className="pb-1.5 text-sm text-muted-foreground">
              {network.neurons.length.toLocaleString()} neurons · {network.synapseCount.toLocaleString()} synapses
            </span>
          )}
          <div className="pb-1.5">
            <StateBadge state={state} data-testid="state" />
          </div>
        </Row>
      </Panel>

      <Panel>
        <Row>
          {state === "paused" ? (
            <Button onClick={act(() => client!.resume(id))}>
              <Play data-icon="inline-start" />
              Resume
            </Button>
          ) : (
            <Button
              disabled={!network || running || building}
              data-testid="start"
              onClick={act(() =>
                client!.start(id, {
                  speed,
                  durationMs: run.durationS === null ? null : run.durationS * 1000,
                  record: record && !replay,
                  ...(builtNodes.length > 0 ? { lockstep } : {}),
                }),
              )}
            >
              <Play data-icon="inline-start" />
              Start
            </Button>
          )}
          <Button variant="outline" disabled={state !== "running"} onClick={act(() => client!.pause(id))} data-testid="pause">
            <Pause data-icon="inline-start" />
            Pause
          </Button>
          <Button variant="outline" disabled={!network || state === "running" || building} onClick={act(() => client!.step(id, 10))}>
            <SkipForward data-icon="inline-start" />
            Step 10 ms
          </Button>
          <Button variant="outline" disabled={!running} onClick={act(() => client!.stop(id))} data-testid="stop">
            <Square data-icon="inline-start" />
            Stop
          </Button>
          {!replay && (
            <Button
              variant="outline"
              disabled={!network || building || !(state === "ready" || (state === "paused" && builtNodes.length === 0))}
              title={
                builtNodes.length > 0
                  ? "Save the run so far, to continue it later (from the Recordings page). On a cluster, stop the run first."
                  : "Save the run so far, to continue it later (from the Recordings page). Pause or stop the run first."
              }
              data-testid="save-run"
              onClick={act(async () => {
                const saved = await client!.saveRun(id, session.name);
                setSavedNote(`Saved at ${(saved.atMs / 1000).toFixed(2)} s. Continue it from the Recordings page.`);
              })}
            >
              <Save data-icon="inline-start" />
              Save
            </Button>
          )}
          <Field
            label="Run for (s)"
            width={110}
            value={durationText}
            placeholder="until stopped"
            onChange={(e) => setDurationText(e.target.value.replace(/[^\d.]/g, ""))}
            onBlur={() => runSettingsChanged({ durationS: Number(durationText) > 0 ? Number(durationText) : null })}
          />
          <div className="grid w-52 gap-2 pb-1.5">
            <span className="text-xs text-muted-foreground">Speed {asFastAsPossible ? "max" : speedLabel(speedLog)}</span>
            <Slider
              min={-6}
              max={6}
              step={1}
              value={[speedLog]}
              disabled={asFastAsPossible}
              aria-label="Speed"
              onValueChange={([v]) => setSpeedLog(v!)}
              onValueCommit={([v]) => running && void act(() => client!.setSpeed(id, speedOf(v!)))()}
            />
          </div>
          <Toggle
            label="As fast as possible"
            checked={asFastAsPossible}
            onChange={(on) => {
              setAsFastAsPossible(on);
              if (running) void act(() => client!.setSpeed(id, on ? null : speedOf(speedLog)))();
            }}
          />
          {!replay && (
            <Toggle
              label="Record"
              checked={record}
              disabled={running}
              onChange={setRecord}
              testId="record"
              title="Record the run (spikes, weights, membrane of inspected neurons); see the Recordings page"
            />
          )}
        </Row>
        {replay && (
          <div className="flex items-center gap-4" data-testid="seek">
            <span className="text-sm whitespace-nowrap">Replay · seek</span>
            <Slider
              className="flex-1"
              min={0}
              max={Math.max(session.replayEndMs ?? 0, 1)}
              value={[running ? session.simMs : seekMs]}
              disabled={running || !network}
              aria-label="Seek"
              onValueChange={([v]) => {
                setSeeking(true);
                setSeekMs(v!);
              }}
              onValueCommit={([v]) => {
                setSeeking(false);
                void act(() => client!.seek(id, v!))();
              }}
            />
            <span className="text-xs whitespace-nowrap text-muted-foreground">
              {running ? "stop to seek" : `${(seekMs / 1000).toFixed(2)} of ${((session.replayEndMs ?? 0) / 1000).toFixed(2)} s`}
            </span>
          </div>
        )}
        {!replay && (
          <Row>
            <Field label="Stimulate neurons starting with" width={240} value={stimulus.prefix} onChange={(e) => setStimulus({ ...stimulus, prefix: e.target.value })} />
            <Field label="mV" type="number" width={80} value={stimulus.mv} onChange={(e) => setStimulus({ ...stimulus, mv: Number(e.target.value) })} />
            <Button
              variant="outline"
              disabled={!network}
              data-testid="stimulate"
              onClick={act(async () => {
                const targets = network!.neurons.filter((n) => n.name.startsWith(stimulus.prefix)).map((n) => n.index);
                if (!targets.length) throw new Error(`no neuron names start with '${stimulus.prefix}'`);
                await client!.stimulate(id, targets, stimulus.mv);
              })}
            >
              <Zap data-icon="inline-start" />
              Stimulate
            </Button>
            <div className="flex-1" />
            <Toggle
              label="Background input"
              checked={background !== null}
              onChange={(on) => backgroundChanged(on ? backgroundDraft : null)}
              testId="background"
              title="Poisson input, applied when the network is built"
            />
            <Field label="Neurons starting with" width={170} placeholder="all" value={backgroundDraft.prefix} onChange={(e) => setBackground({ ...backgroundDraft, prefix: e.target.value })} />
            <Field label="Hz per neuron" type="number" width={120} value={backgroundDraft.rateHz} onChange={(e) => setBackground({ ...backgroundDraft, rateHz: Math.max(0.001, Number(e.target.value)) })} />
            <Field label="mV" type="number" width={80} value={backgroundDraft.mv} onChange={(e) => setBackground({ ...backgroundDraft, mv: Number(e.target.value) })} />
          </Row>
        )}
        {needsRebuild && !replay && (
          <p className="text-xs text-warning" data-testid="rebuild-hint">
            Background input is applied when the network is built: {running ? "stop and rebuild" : "press Rebuild"} to apply the change.
          </p>
        )}
      </Panel>

      {network && client && !replay && <Modulators client={client} session={id} modulators={network.modulators} onError={setError} />}

      {activeInput && activeInput.session === id && (
        <Notice
          action={
            <Button size="sm" variant="outline" onClick={stopInput} data-testid="stop-input-run">
              Stop
            </Button>
          }
        >
          {activeInput.kind === "microphone" ? "The microphone is streaming into this network." : "A designed signal is streaming into this network."}
        </Notice>
      )}
      {error && (
        <Notice tone="error" onClose={() => setError(null)}>
          {error}
        </Notice>
      )}
      {savedNote && (
        <Notice tone="success" onClose={() => setSavedNote(null)} data-testid="saved-note">
          {savedNote}
        </Notice>
      )}

      <Panel>
        <div className="flex flex-wrap gap-x-8 gap-y-2" data-testid="timing">
          <Stat label="Simulated time" value={`${((stats?.simMs ?? session.simMs) / 1000).toFixed(3)} s`} />
          <Stat label="Speed" value={!stats || stats.speed === 0 ? "—" : Number.isFinite(stats.speed) ? `×${stats.speed.toPrecision(3)}` : "max"} />
          <Stat label="Behind the clock" value={`${(stats?.lagMs ?? 0).toFixed(1)} ms`} warn={(stats?.lagMs ?? 0) > 20} />
          <Stat label="Frames" value={`${stats?.framesPerS ?? 0}/s`} />
          <Stat label="Spikes" value={`${(stats?.spikesPerS ?? 0).toLocaleString()}/s`} />
          <Stat label="Detail" value={stats ? (["exact", "counts", "cells"][stats.lod] ?? "?") : "—"} warn={!!stats && stats.lod !== Lod.Exact} />
          <Stat label="Dropped" value={`${stats?.droppedFrames ?? 0} frames · ${stats?.droppedEvents ?? 0} events`} warn={!!stats && stats.droppedFrames + stats.droppedEvents > 0} />
        </div>
      </Panel>

      {network && client && builtNodes.length > 0 && <ClusterPanel client={client} session={id} running={running} lockstep={lockstep} onLockstep={setLockstep} />}

      {network && live && (
        <Panel>
          <div className="flex items-center gap-3">
            <Heading className="flex-1">Network</Heading>
            <Toggle label="3D view" checked={show3d} onChange={setShow3d} />
          </div>
          {show3d && (
            <Network3D
              network={network}
              frames={live.frames}
              width={width}
              height={420}
              selected={picked.map((n) => n.index)}
              sensors={sensors}
              onPick={(i) => {
                const neuron = network.neurons[i];
                if (neuron) setPicked([neuron]);
              }}
              onRegion={setRegion}
              queryState={queryState}
            />
          )}
          {sensors.length > 0 && (
            <div className="flex flex-wrap items-center gap-1.5" data-testid="sensor-legend">
              <span className="text-xs text-muted-foreground">Sensors</span>
              {sensors.map((x) => (
                <Badge key={x.id} variant="outline" className="gap-1.5" title={`${x.encoding} encoding · ${x.samples.toLocaleString()} samples · ${x.inputs.toLocaleString()} inputs`}>
                  <span className={`size-2 rounded-full ${sensorActive.has(x.id) ? "bg-[#009e73]" : "bg-muted-foreground/40"}`} />
                  {x.name} · {x.channels.length} channel{x.channels.length === 1 ? "" : "s"}
                </Badge>
              ))}
            </div>
          )}
        </Panel>
      )}

      <Panel>
        <Row>
          <Heading className="flex-1 self-center">
            Spikes {selection.length < (network?.neurons.length ?? 0) ? `(${selection.length} of ${network?.neurons.length})` : ""}
          </Heading>
          {region && (
            <Badge variant="secondary" className="mb-1.5 gap-1 pr-0.5" data-testid="region">
              Region: {region.length.toLocaleString()} neurons{region.length > MAX_ROWS ? ` (first ${MAX_ROWS} shown)` : ""}
              <button type="button" aria-label="Clear the region" className="opacity-60 hover:opacity-100" onClick={() => setRegion(null)}>
                <X className="size-3" />
              </button>
            </Badge>
          )}
          <Field label="Show neurons starting with" width={240} value={filter} onChange={(e) => setFilter(e.target.value)} />
          <Choice
            label="Time window"
            width={120}
            value={windowMs}
            onChange={setWindowMs}
            options={[1000, 5000, 20_000, 60_000, 100_000].map((ms) => ({ value: ms, label: `${ms / 1000} s` }))}
          />
        </Row>
        <ChartControls />
        {running && stats !== null && stats.spikesPerS === 0 && (
          <Notice data-testid="no-spikes">
            No spikes in the last second. Most networks need input: stimulate their input neurons, or turn on background input (e.g. neurons
            starting with <code>input-</code>) and build again.
          </Notice>
        )}
        <div ref={rasterRef} data-testid="raster">
          {raster && rows.length > 0 && (
            <Chart
              key={`${windowMs}:${cadence}`}
              chartId={1}
              width={width}
              height={rasterHeight}
              dataSource={raster}
              color={colors.color}
              backgroundColor={colors.backgroundColor}
              seriesStyles={lineStyles}
              windowingTime={bufferMs}
              dataUpdatePeriod={updateMs}
              margin={{ left: labelWidth, right: 20, bottom: 48 }}
            >
              <ContinuousAxis axisId="x-axis-1" location={AxisLocation.Bottom} domain={[0, windowMs]} label="t (ms)" />
              <OrdinalAxis
                axisId="y-axis-1"
                location={AxisLocation.Left}
                categories={rows}
                label={showNames ? "" : `${rows.length} neurons`}
                font={showNames ? (rowHeight < 14 ? { size: 9 } : undefined) : { color: "transparent" }}
              />
              <RasterPlot spikeMargin={rowHeight >= 10 ? 1 : 0} withCadence={cadence} />
            </Chart>
          )}
        </div>
        <Heading className="mt-2">Population rate (Hz per neuron)</Heading>
        <div data-testid="rate">
          {live && (
            <Chart
              key={`${windowMs}:${cadence}`}
              chartId={2}
              width={width}
              height={180}
              dataSource={live.rate}
              color={colors.color}
              backgroundColor={colors.backgroundColor}
              seriesStyles={lineStyles}
              windowingTime={bufferMs}
              dataUpdatePeriod={16}
              margin={{ left: labelWidth, right: 20, bottom: 48 }}
            >
              <ContinuousAxis axisId="x-axis-1" location={AxisLocation.Bottom} domain={[0, windowMs]} label="t (ms)" />
              <ContinuousAxis axisId="y-axis-1" location={AxisLocation.Left} domain={[-1, 10]} label="Hz" autoScale autoScaleMin={-1} />
              <ScatterPlot withCadence={cadence} />
            </Chart>
          )}
        </div>
      </Panel>

      {network && client && (
        <WeightsPanel client={client} session={id} network={network} width={width} labelWidth={labelWidth} colors={colors} windowMs={windowMs} />
      )}

      {network && client && (
        <Inspector
          client={client}
          session={id}
          network={network}
          width={width}
          labelWidth={labelWidth}
          colors={colors}
          windowMs={windowMs}
          picked={picked}
          onPicked={setPicked}
          replay={replay}
        />
      )}
    </div>
  );
}
