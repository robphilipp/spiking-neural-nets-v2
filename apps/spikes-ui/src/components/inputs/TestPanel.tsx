import { SpikesError, type Network, type TestRunResult } from "@spikes/client";
import { FlaskConical, Plus, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { Heading, NumberField, Notice, Panel } from "@/components/kit";
import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { useConnection } from "../../connection";
import type { InputFile, Trial } from "../../inputFile";
import type { NeuronMatch } from "../../neuronMatch";
import { useSession } from "../../store";
import { type Outcome, useTest } from "../../testStore";
import {
  conditionNames,
  customName,
  defaultDurationMs,
  type GroupSelection,
  MAX_DURATION_MS,
  rates,
  resolveGroups,
  summarize,
  testInputs,
  testKey,
  trialResponses,
} from "../../testRun";
import { NeuronMatchField } from "../NeuronMatchField";
import { type Band, colorOf, RateChart } from "./RateChart";

const message = (e: unknown) => (e instanceof SpikesError ? e.error.message : e instanceof Error ? e.message : String(e));

/** A choice that is on or off, as a chip. */
function Chip({ on, onClick, disabled, children, testId, title }: { on: boolean; onClick: () => void; disabled?: boolean; children: React.ReactNode; testId: string; title?: string }) {
  return (
    <button
      type="button"
      aria-pressed={on}
      disabled={disabled}
      title={title}
      data-testid={testId}
      onClick={onClick}
      className={cn(
        "rounded-full border px-2.5 py-0.5 text-xs transition-colors disabled:opacity-50",
        on ? "border-primary bg-primary text-primary-foreground" : "text-muted-foreground hover:bg-muted",
      )}
    >
      {children}
    </button>
  );
}

/**
 * Test: runs the network with the selected input file in a throwaway network of the engine's, for a few simulated
 * seconds (the real session isn't touched), and shows the spikes of chosen groups of neurons: their rates, over time
 * with the trials marked, and averaged per condition aligned to the trials' starts.
 */
export function TestPanel({ file, selected, seed }: { file: InputFile | null; selected: string | null; seed: number }) {
  const { client } = useConnection();
  const session = useSession();
  const { id, state, applied } = session;
  const replay = session.replayEndMs !== null;
  const busy = state === "running" || state === "paused";

  // The network groups and targets are resolved in: the built one, or else the description's as written (the
  // throwaway builds its own from the same description, so neurons are numbered alike).
  const [preview, setPreview] = useState<{ text: string; network: Network } | null>(null);
  const [previewError, setPreviewError] = useState<string | null>(null);
  useEffect(() => {
    if (session.network || !client || id === null || applied === null || replay || preview?.text === applied) return;
    let live = true;
    client
      .previewNetwork(applied)
      .then((p) => live && (setPreview({ text: applied, network: p.network }), setPreviewError(null)))
      .catch((e: unknown) => live && setPreviewError(message(e)));
    return () => {
      live = false;
    };
  }, [client, id, applied, replay, session.network, preview?.text]);
  const network = session.network ?? (preview?.text === applied ? preview.network : null);

  const { selection, durationS, binMs, outcome } = useTest();
  const setSelection = (f: (s: GroupSelection) => GroupSelection) => useTest.setState((t) => ({ selection: f(t.selection) }));
  const setDurationS = (durationS: number) => useTest.setState({ durationS });
  const setBinMs = (binMs: number) => useTest.setState({ binMs });
  const [draft, setDraft] = useState<NeuronMatch>({ mode: "prefix", pattern: "" });
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const trials = useMemo(() => (file && network ? safeTrials(file, seed, network) : []), [file, seed, network]);
  const defaultMs = defaultDurationMs(trials);
  const durationMs = durationS === null ? defaultMs : durationS * 1000;
  const key = testKey(selected, file, seed, id, applied);
  const stale = outcome !== null && outcome.key !== key;
  const realtime = file?.sources.filter((s) => s.enabled && ["microphone", "camera", "sensor"].includes(s.kind)).map((s) => s.name) ?? [];

  const toggleSet = (name: string) =>
    setSelection((s) => ({ ...s, sets: s.sets.includes(name) ? s.sets.filter((x) => x !== name) : [...s.sets, name] }));
  const addCustom = () => {
    if (!draft.pattern) return;
    setSelection((s) => ({ ...s, custom: [...s.custom, draft] }));
    setDraft({ ...draft, pattern: "" });
  };

  const blocked = !client || id === null ? "Apply the network on the Network page first: a test run is made from the session's description." : replay ? "A replay plays back what was recorded: it can't be test run." : !file ? "No input file is selected: select or create one above." : busy ? "A run is open: the machine is busy with it. Stop the run to test." : !network ? "Reading the network…" : null;

  const run = async () => {
    if (!client || id === null || !file || !network) return;
    setError(null);
    setRunning(true);
    try {
      const inputs = testInputs(file, seed, network);
      const { groups, problems } = resolveGroups(selection, file, network);
      if (problems.length) throw new Error(problems.join("; "));
      const result = await client.testRun(id, { sources: inputs.sources, trials: inputs.trials, durationMs, binMs, groups, seed });
      useTest.setState({ outcome: { key, result, trials: inputs.trials, size: network.neurons.length } });
    } catch (e) {
      setError(message(e));
    } finally {
      setRunning(false);
    }
  };

  return (
    <Panel data-testid="test-panel">
      <div className="flex flex-wrap items-center gap-2">
        <Heading>Test</Heading>
        <span className="text-xs text-muted-foreground">
          Runs the network with this file's inputs for a few simulated seconds in a throwaway copy, as fast as possible: the session itself isn't touched.
        </span>
      </div>
      {blocked && (
        <Notice tone={busy ? "warning" : "info"} data-testid="test-blocked">
          {blocked}
        </Notice>
      )}
      {previewError && <Notice tone="error">{previewError}</Notice>}
      {realtime.length > 0 && (
        <p className="text-xs text-muted-foreground" data-testid="test-realtime-note">
          Realtime sources are left out (they aren't reproducible): {realtime.join(", ")}.
        </p>
      )}
      <div className="flex flex-wrap items-end gap-3">
        <NumberField
          label="Test length (s)"
          width={100}
          value={durationMs / 1000}
          min={0.01}
          max={MAX_DURATION_MS / 1000}
          onChange={setDurationS}
          data-testid="test-duration"
          title={`Simulated time to run; the default is ${trials.length ? "the end of the trials" : "5 s"}. At most ${MAX_DURATION_MS / 1000} s.`}
        />
        <NumberField label="Bin (ms)" width={90} value={binMs} min={1} max={5000} integer onChange={setBinMs} data-testid="test-bin" />
        <Button onClick={() => void run()} disabled={blocked !== null || running} data-testid="test-run">
          <FlaskConical data-icon="inline-start" />
          {running ? "Running…" : "Run test"}
        </Button>
        {outcome && <span className="pb-1.5 text-xs text-muted-foreground">Seed {seed}: the same file and seed always give the same result.</span>}
      </div>
      <div className="grid gap-2" data-testid="test-groups">
        <div className="text-xs text-muted-foreground">Groups to count</div>
        <div className="flex flex-wrap items-center gap-1.5">
          <Chip on={selection.sources} onClick={() => setSelection((s) => ({ ...s, sources: !s.sources }))} testId="test-group-sources" title="One group per enabled simulated source: its target neurons">
            each source's targets
          </Chip>
          <Chip on={selection.all} onClick={() => setSelection((s) => ({ ...s, all: !s.all }))} testId="test-group-all">
            all neurons
          </Chip>
          {network?.sets.map((set) => (
            <Chip key={set.name} on={selection.sets.includes(set.name)} onClick={() => toggleSet(set.name)} testId={`test-group-set-${set.name}`} title={`${set.members.length} neurons`}>
              {set.name}
            </Chip>
          ))}
          {selection.custom.map((m, i) => (
            <span key={i} className="inline-flex items-center gap-1 rounded-full border border-primary bg-primary px-2.5 py-0.5 text-xs text-primary-foreground" data-testid="test-custom-chip">
              {customName(m)}
              <button type="button" aria-label={`Remove ${customName(m)}`} onClick={() => setSelection((s) => ({ ...s, custom: s.custom.filter((_, j) => j !== i) }))}>
                <X className="size-3" />
              </button>
            </span>
          ))}
        </div>
        {network && network.sets.length === 0 && <p className="text-xs text-muted-foreground">The network has no sets.</p>}
        <div className="flex flex-wrap items-end gap-2">
          <NeuronMatchField label="Group by name" value={draft} onChange={setDraft} neurons={network?.neurons} width={230} testId="test-custom" />
          <Button variant="outline" size="sm" onClick={addCustom} disabled={!draft.pattern || !network} data-testid="test-custom-add">
            <Plus data-icon="inline-start" />
            Add group
          </Button>
        </div>
      </div>
      {error && (
        <Notice tone="error" onClose={() => setError(null)} data-testid="test-error">
          {error}
        </Notice>
      )}
      {outcome && <Results outcome={outcome} stale={stale} />}
    </Panel>
  );
}

/** The run's trials as `expandTrials` draws them, or none if the inputs can't be expanded (the run says why). */
function safeTrials(file: InputFile, seed: number, network: Network): Trial[] {
  try {
    return testInputs(file, seed, network).trials;
  } catch {
    return [];
  }
}

function Results({ outcome, stale }: { outcome: Outcome; stale: boolean }) {
  const { result, trials } = outcome;
  const rows = useMemo(() => summarize(result, outcome.size), [result, outcome.size]);
  const names = result.groups.map((g) => g.name);
  const series = useMemo(
    () => result.groups.map((g, i) => ({ name: g.name, color: colorOf(i), rates: rates(result, g.counts, g.neurons) })),
    [result],
  );
  const conditions = conditionNames(trials);
  const bands: Band[] = trials
    .filter((t) => t.startMs < result.simMs)
    .map((t) => ({
      fromMs: t.startMs,
      toMs: t.endMs,
      label: t.condition ? `${t.number} ${t.condition}` : String(t.number),
      color: colorOf(conditions.indexOf(t.condition) + 3),
    }));
  const responses = useMemo(() => trialResponses(result, trials), [result, trials]);
  const horizon = Math.max(result.binMs, result.simMs);
  return (
    <div className={cn("grid gap-3 border-t pt-3", stale && "opacity-60")} data-testid="test-results" data-stale={stale}>
      <div className="flex flex-wrap items-center gap-x-4 gap-y-1 text-xs text-muted-foreground">
        <span data-testid="test-summary">
          {(result.simMs / 1000).toFixed(2)} s simulated in {(result.elapsedMs / 1000).toFixed(2)} s · {result.spikes.toLocaleString()} spikes in the network
        </span>
        {stale && (
          <span className="rounded bg-warning/20 px-1.5 py-0.5 text-warning" data-testid="test-stale">
            stale: the input file or seed has changed since: run the test again
          </span>
        )}
      </div>
      {!result.complete && (
        <Notice tone="warning" data-testid="test-incomplete">
          The test stopped at {(result.simMs / 1000).toFixed(2)} s of simulated time because it used its wall-clock budget ({(result.elapsedMs / 1000).toFixed(1)} s): what follows covers only that.
        </Notice>
      )}
      <table className="w-full max-w-xl text-xs" data-testid="test-table">
        <thead className="text-left text-muted-foreground">
          <tr>
            <th className="py-1 font-normal">Group</th>
            <th className="py-1 text-right font-normal">Neurons</th>
            <th className="py-1 text-right font-normal">Spikes</th>
            <th className="py-1 text-right font-normal">Mean rate (Hz per neuron)</th>
          </tr>
        </thead>
        <tbody>
          {rows.map((r, i) => (
            <tr key={r.name} className="border-t" data-testid={`test-row-${r.name}`}>
              <td className="py-1">
                {i < names.length && <span className="mr-1.5 inline-block size-2 rounded-full align-middle" style={{ background: colorOf(i) }} />}
                {r.name}
              </td>
              <td className="py-1 text-right tabular-nums">{r.neurons}</td>
              <td className="py-1 text-right tabular-nums" data-testid="test-spikes">
                {r.spikes.toLocaleString()}
              </td>
              <td className="py-1 text-right tabular-nums" data-testid="test-hz">
                {r.meanHz.toFixed(2)}
              </td>
            </tr>
          ))}
        </tbody>
      </table>
      {result.groups.length > 0 && (
        <div className="grid gap-1">
          <div className="text-xs text-muted-foreground">
            Rate per neuron over time, in bins of {result.binMs} ms{trials.length ? "; trials are shaded, in their conditions' colours" : ""}.
          </div>
          <RateChart series={series} binMs={result.binMs} domainMs={horizon} bands={bands} testId="test-rate-chart" />
          <Legend names={names} />
        </div>
      )}
      {responses.length > 0 && (
        <div className="grid gap-2" data-testid="test-responses">
          <div className="text-xs text-muted-foreground">Trial-averaged response: each condition's trials aligned to their starts and averaged.</div>
          <div className="grid gap-3 lg:grid-cols-2">
            {responses.map((c) => (
              <div key={c.condition} className="grid gap-1" data-testid={`test-response-${c.condition || "all"}`}>
                <div className="text-xs font-medium">
                  {c.condition || "all trials"} <span className="font-normal text-muted-foreground">· {c.trials} trial{c.trials === 1 ? "" : "s"}</span>
                </div>
                <RateChart
                  series={c.groups.map((g, i) => ({ name: g.name, color: colorOf(i), rates: g.rates }))}
                  binMs={result.binMs}
                  domainMs={c.bins * result.binMs}
                  height={170}
                  testId={`test-response-chart-${c.condition || "all"}`}
                />
              </div>
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

function Legend({ names }: { names: string[] }) {
  return (
    <div className="flex flex-wrap gap-x-3 gap-y-0.5 text-xs text-muted-foreground" data-testid="test-legend">
      {names.map((n, i) => (
        <span key={n}>
          <span className="mr-1 inline-block size-2 rounded-full align-middle" style={{ background: colorOf(i) }} />
          {n}
        </span>
      ))}
    </div>
  );
}
