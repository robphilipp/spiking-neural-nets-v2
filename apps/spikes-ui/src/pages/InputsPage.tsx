// The Inputs view (app plan 05): the project's input files, the selected file's sources and trials with editors, and
// a preview (a timeline of when each source is on, a selected signal's channels, its targets in 3D). While a run is
// open the file is read-only: sources show their status, and pulse sources can be fired.

import { useEffect, useMemo, useState } from "react";
import { Heading, Notice, Panel, Stat } from "@/components/kit";
import { Button } from "@/components/ui/button";
import { ChannelPreview } from "../components/inputs/ChannelPreview";
import { FilesPanel } from "../components/inputs/FilesPanel";
import { SourceEditor } from "../components/inputs/SourceEditor";
import { SourceList } from "../components/inputs/SourceList";
import { Timeline } from "../components/inputs/Timeline";
import { TargetsView } from "../components/inputs/TargetsView";
import { TestPanel } from "../components/inputs/TestPanel";
import { TrialsEditor } from "../components/inputs/TrialsEditor";
import { useCanFire } from "../components/FireButton";
import { addSource, newSource, sourceState, timeline, trialAt } from "../inputEdit";
import { emptyInputFile, expandTrials, InputFileError, targetNeurons } from "../inputFile";
import { inputsChanged } from "../buildNetwork";
import { useLive } from "../live";
import {
  inputFileAdded,
  inputFileChanged,
  inputFileDeleted,
  inputFileDuplicated,
  inputFileRenamed,
  inputsSelected,
  simTime,
  useProject,
  useSession,
} from "../store";
import { useRealtime } from "../realtime";
import { useBehavior } from "../useBehavior";

export function InputsPage() {
  const names = useProject((s) => s.project.inputs);
  const selected = useProject((s) => s.project.selectedInputs);
  const file = useProject((s) => (s.project.selectedInputs === null ? null : (s.inputFiles[s.project.selectedInputs] ?? emptyInputFile())));
  const { seed, durationS } = useProject((s) => s.project.run);
  const session = useSession();
  const { network, state } = session;
  const locked = state === "running" || state === "paused";
  const [index, setIndex] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const canFire = useCanFire(selected, file);
  const statuses = useRealtime((s) => s.statuses);
  const realtime = useMemo(() => Object.fromEntries(statuses.map((x) => [x.name, x])), [statuses]);

  // The run's time: the live view's, while it goes (the Run page keeps the session's up to date when it's showing).
  const stats = useBehavior(useLive((s) => s.view)?.stats);
  useEffect(() => {
    if (stats && locked) simTime(stats.simMs);
  }, [stats, locked]);
  const simMs = locked ? (stats?.simMs ?? session.simMs) : session.simMs;

  const current = Math.min(index, Math.max(0, (file?.sources.length ?? 1) - 1));
  const source = file?.sources[current] ?? null;
  const edit = (f: NonNullable<typeof file>) => selected !== null && inputFileChanged(selected, f);

  // The file as the run gets it: trials expanded with the seed (each trial source's instances, the trials).
  const expanded = useMemo(() => (file ? expandTrials(file, seed) : null), [file, seed]);
  const data = useMemo(() => (file ? timeline(file, durationS === null ? null : durationS * 1000, seed) : null), [file, durationS, seed]);
  // A run has the file's sources if the network was built with this file as it is.
  const built = network !== null && !inputsChanged(session.builtInputs, selected, file);
  const showStatus = built && (locked || simMs > 0);
  const states = useMemo(
    () => (showStatus && file && expanded ? file.sources.map((s) => sourceState(s, expanded.file, simMs)) : null),
    [showStatus, file, expanded, simMs],
  );

  const targets = useMemo(() => {
    if (!network || !source) return null;
    try {
      return targetNeurons(source.targets, network);
    } catch {
      return null;
    }
  }, [network, source]);
  const channelNames = useMemo(() => (targets && network ? targets.map((i) => network.neurons[i]!.name) : null), [targets, network]);

  const attempt = (f: () => void) => {
    setError(null);
    try {
      f();
    } catch (e) {
      if (!(e instanceof InputFileError)) throw e;
      setError(e.message);
    }
  };

  return (
    <div className="flex flex-col gap-3" data-testid="inputs-page">
      <FilesPanel
        names={names}
        selected={selected}
        file={file}
        locked={locked}
        onSelect={inputsSelected}
        onNew={() => {
          inputFileAdded();
          setIndex(0);
        }}
        onDuplicate={() => selected !== null && (inputFileDuplicated(selected), setIndex(0))}
        onRename={(label) => selected !== null && inputFileRenamed(selected, label)}
        onDelete={() => selected !== null && (inputFileDeleted(selected), setIndex(0))}
        onDescription={(description) => file && edit({ ...file, description })}
      />
      {locked && (
        <Notice data-testid="locked-note">
          A run is open, and a run is fully described by its input file and seed, so the file can't be edited now. Stop the run to edit; Fire sends a
          pulse source's pulse now.
        </Notice>
      )}
      {error && (
        <Notice tone="error" onClose={() => setError(null)}>
          {error}
        </Notice>
      )}
      {!file || !expanded || !data ? (
        <Notice>No input file is selected: runs get no input. Pick or create one above.</Notice>
      ) : (
        <>
          {showStatus && expanded.trials.length > 0 && (
            <Panel>
              <div className="flex flex-wrap gap-x-8" data-testid="inputs-run">
                <Stat label="Simulated time" value={`${(simMs / 1000).toFixed(3)} s`} />
                <Stat label="Trial" value={trialAt(expanded.trials, simMs)} testId="inputs-trial" />
              </div>
            </Panel>
          )}
          <div className="grid items-start gap-3 lg:grid-cols-[minmax(0,26rem)_minmax(0,1fr)]">
            <SourceList
              file={file}
              selected={current}
              onSelect={setIndex}
              locked={locked}
              network={network}
              states={states}
              realtime={realtime}
              canFire={canFire}
              onError={setError}
              onAdd={(kind) => {
                edit(addSource(file, newSource(kind, file.sources.map((s) => s.name))));
                setIndex(file.sources.length);
              }}
            />
            {source ? (
              <SourceEditor file={file} index={current} onChange={(f) => attempt(() => edit(f))} onSelect={setIndex} network={network} locked={locked} trials={file.trials !== undefined} />
            ) : (
              <Panel>
                <p className="text-sm text-muted-foreground">Add a source to start: each says what to drive (its targets), how and when.</p>
              </Panel>
            )}
          </div>
          <TrialsEditor file={file} onChange={edit} locked={locked} />
          <Panel>
            <Heading>Timeline</Heading>
            <p className="text-xs text-muted-foreground">
              When each source is on over the run, with trials drawn from the seed ({seed}) as they'll run{durationS === null ? "" : `, to the run's ${durationS} s`}. Realtime sources
              (microphone, camera, sensor) have no schedule: they stream while the run goes.
            </p>
            <Timeline data={data} selected={source?.name ?? null} onSelect={(name) => setIndex(Math.max(0, file.sources.findIndex((s) => s.name === name)))} nowMs={showStatus ? simMs : null} />
          </Panel>
          <div className="grid items-start gap-3 lg:grid-cols-2">
            <Panel>
              <Heading>Channels</Heading>
              {source?.kind === "signal" ? (
                <ChannelPreview formula={source.formula} name={source.name} seed={seed} names={channelNames} windowMs={Math.min(source.duration ?? 1000, 1000)} />
              ) : (
                <p className="text-xs text-muted-foreground">Select a signal source to see its channels over time.</p>
              )}
            </Panel>
            <Panel>
              <Heading>Targets</Heading>
              {network ? (
                <TargetsView network={network} targets={targets} />
              ) : (
                <p className="text-sm text-muted-foreground" data-testid="targets-hint">
                  Build the network (on the Run page) to see the selected source's targets.
                </p>
              )}
            </Panel>
          </div>
          <TestPanel file={file} selected={selected} seed={seed} />
        </>
      )}
    </div>
  );
}
