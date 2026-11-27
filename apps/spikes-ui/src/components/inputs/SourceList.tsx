import type { Network } from "@spikes/client";
import { Plus } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Heading, Panel } from "@/components/kit";
import { cn } from "@/lib/utils";
import { scheduleText, type SourceState } from "../../inputEdit";
import { describeTargets, type InputFile, isRealtime, type Source, type SourceKind, targetNeurons } from "../../inputFile";
import { hasChannels } from "../../channelLayout";
import { type LoadedData } from "../../inputData";
import { mappingWarning, sourceChannels } from "../../sourceChannels";
import type { RealtimeStatus } from "../../realtimeManager";
import { FireButton } from "../FireButton";
import { useMemo } from "react";

const ADD: SourceKind[] = ["pulse", "poisson", "periodic", "signal", "file"];
/** A realtime source's live badge. */
const LIVE_TONE: Record<RealtimeStatus["state"], string> = {
  starting: "bg-warning text-black",
  streaming: "bg-success text-white",
  stopped: "bg-secondary text-muted-foreground",
  refused: "bg-warning text-black",
  error: "bg-destructive text-white",
};

/** What a realtime source's status says in a line: its state's detail, or the counters. */
export function liveDetail(r: RealtimeStatus): string {
  if (r.state === "streaming" || r.state === "stopped") {
    const rate = r.state === "streaming" ? `${Math.round(r.samplesPerSec).toLocaleString()} samples/s · ` : "";
    return `${rate}${r.samples.toLocaleString()} samples · ${r.late} late · ${r.dropped} dropped`;
  }
  return r.message;
}

const ADD_REALTIME: SourceKind[] = ["microphone", "camera", "sensor"];

/** How many neurons a source's targets match in the built network, or why they can't (an unknown set, a bad regex). */
function matchOf(s: Source, network: Network | null): { count: number } | { error: string } | null {
  if (!network) return null;
  try {
    return { count: targetNeurons(s.targets, network).length };
  } catch (e) {
    return { error: e instanceof Error ? e.message : String(e) };
  }
}

const TONE: Record<SourceState, string> = {
  off: "bg-secondary text-muted-foreground",
  live: "bg-secondary text-secondary-foreground",
  manual: "bg-secondary text-secondary-foreground",
  "no trials": "bg-secondary text-muted-foreground",
  scheduled: "bg-secondary text-secondary-foreground",
  on: "bg-success text-white",
  done: "bg-secondary text-muted-foreground",
};

/**
 * The file's sources: a row each (kind, name, targets with their count in the built network, schedule), a status
 * during a run, and Fire on pulse sources. Add buttons for each kind at the bottom.
 */
export function SourceList({
  file,
  selected,
  onSelect,
  onAdd,
  locked,
  network,
  states,
  realtime,
  canFire,
  onError,
  data,
}: {
  file: InputFile;
  selected: number;
  onSelect: (index: number) => void;
  onAdd: (kind: SourceKind) => void;
  locked: boolean;
  network: Network | null;
  /** Each source's state at the run's time (shown once the run has started); null before. */
  states: SourceState[] | null;
  /** The realtime sources' live status in the run (by source name). */
  realtime: Record<string, RealtimeStatus>;
  canFire: (source: string) => boolean;
  onError: (message: string) => void;
  /** The input file's data files, read (a file source's channels are the file's). */
  data: LoadedData;
}) {
  const matches = useMemo(() => file.sources.map((s) => matchOf(s, network)), [file.sources, network]);
  const plans = useMemo(
    () => file.sources.map((s) => (network && hasChannels(s) ? sourceChannels(s, network, data) : null)),
    [file.sources, network, data],
  );
  return (
    <Panel data-testid="source-list">
      <Heading>Sources</Heading>
      {file.sources.length === 0 && <p className="text-sm text-muted-foreground">No sources yet: add one below.</p>}
      <div className="flex flex-col gap-1.5">
        {file.sources.map((s, i) => {
          const match = matches[i];
          const live = realtime[s.name];
          // What a realtime source sends, or why it can't: known once the network says how many neurons it drives.
          const found = plans[i];
          const mapping = found && "result" in found ? found.result : null;
          const warn = mapping?.ok ? mappingWarning(mapping) : null;
          const shown = mapping && (isRealtime(s) || s.kind === "file" || ("layout" in s && s.layout));
          const sensor = s.kind === "sensor" && match && "count" in match ? match.count : null;
          return (
            <div
              key={i}
              className={cn("flex items-center gap-2 rounded-md border px-2.5 py-1.5", i === selected ? "border-primary bg-accent/50" : "hover:bg-muted/50", !s.enabled && "opacity-60")}
              data-testid={`source-row-${s.name}`}
              data-selected={i === selected}
            >
              <button type="button" className="min-w-0 flex-1 text-left" onClick={() => onSelect(i)} aria-label={`Select ${s.name}`} aria-current={i === selected}>
                <div className="flex items-center gap-2">
                  <Badge variant="outline" className="shrink-0 font-normal">
                    {s.kind}
                  </Badge>
                  <span className="truncate text-sm font-medium">{s.name}</span>
                  {s.scope === "trial" && (
                    <Badge variant="secondary" className="shrink-0 font-normal">
                      trial
                    </Badge>
                  )}
                </div>
                <div className="mt-0.5 text-xs break-words text-muted-foreground">
                  {describeTargets(s.targets)}
                  {match && "count" in match && (
                    <span className={cn("tabular-nums", match.count === 0 && "text-destructive")} data-testid={`source-count-${s.name}`}>
                      {" "}
                      · {match.count} neuron{match.count === 1 ? "" : "s"}
                    </span>
                  )}
                  {match && "error" in match && <span className="text-destructive"> · {match.error}</span>}
                  {" · "}
                  {isRealtime(s) ? "live, while the run goes" : scheduleText(s)}
                </div>
                {shown && mapping && (
                  <div className={cn("mt-0.5 text-xs", !mapping.ok ? "text-destructive" : "text-muted-foreground")} data-testid={`source-channels-${s.name}`}>
                    {mapping.ok ? `${mapping.channels.length} channel${mapping.channels.length === 1 ? "" : "s"}` : mapping.error}
                    {warn && (
                      <span className="text-warning" data-testid={`source-mapping-${s.name}`}>
                        {" "}
                        · {warn}
                      </span>
                    )}
                  </div>
                )}
                {sensor !== null && (
                  <div className={cn("mt-0.5 text-xs", sensor === 0 ? "text-destructive" : "text-muted-foreground")} data-testid={`source-channels-${s.name}`}>
                    {sensor === 0 ? "sensor has no target neurons" : `${sensor} channel${sensor === 1 ? "" : "s"}`}
                  </div>
                )}
                {live && (
                  <div className="mt-0.5 text-xs break-words text-muted-foreground tabular-nums" data-testid={`rt-detail-${s.name}`}>
                    {liveDetail(live)}
                  </div>
                )}
              </button>
              {live ? (
                <Badge className={cn("shrink-0 border-0 font-normal", LIVE_TONE[live.state])} data-testid={`rt-state-${s.name}`}>
                  {live.state}
                </Badge>
              ) : (
                states && (
                  <Badge className={cn("shrink-0 border-0 font-normal", TONE[states[i]!])} data-testid={`source-state-${s.name}`}>
                    {states[i]}
                  </Badge>
                )
              )}
              {s.kind === "pulse" && s.enabled && s.scope !== "trial" && <FireButton source={s.name} ready={canFire(s.name)} onError={onError} compact />}
            </div>
          );
        })}
      </div>
      <div className="flex flex-wrap items-center gap-1.5" data-testid="add-source">
        <span className="text-xs text-muted-foreground">Add</span>
        {ADD.map((k) => (
          <Button key={k} variant="outline" size="sm" disabled={locked} data-testid={`add-${k}`} aria-label={`Add ${k} source`} onClick={() => onAdd(k)}>
            <Plus data-icon="inline-start" />
            {k}
          </Button>
        ))}
        <span className="ml-1 text-xs text-muted-foreground" title="Realtime sources stream in wall-clock time while the run goes">
          realtime
        </span>
        {ADD_REALTIME.map((k) => (
          <Button key={k} variant="ghost" size="sm" disabled={locked} data-testid={`add-${k}`} aria-label={`Add ${k} source`} onClick={() => onAdd(k)}>
            <Plus data-icon="inline-start" />
            {k}
          </Button>
        ))}
      </div>
    </Panel>
  );
}
