import type { Network } from "@spikes/client";
import { Plus } from "lucide-react";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Heading, Panel } from "@/components/kit";
import { cn } from "@/lib/utils";
import { scheduleText, type SourceState } from "../../inputEdit";
import { describeTargets, type InputFile, type Source, SIMULATED, type SourceKind, targetNeurons } from "../../inputFile";
import { FireButton } from "../FireButton";
import { useMemo } from "react";

const ADD: SourceKind[] = ["pulse", "poisson", "periodic", "signal"];
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
  "not run yet": "bg-secondary text-muted-foreground",
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
  canFire,
  onError,
}: {
  file: InputFile;
  selected: number;
  onSelect: (index: number) => void;
  onAdd: (kind: SourceKind) => void;
  locked: boolean;
  network: Network | null;
  /** Each source's state at the run's time (shown once the run has started); null before. */
  states: SourceState[] | null;
  canFire: boolean;
  onError: (message: string) => void;
}) {
  const matches = useMemo(() => file.sources.map((s) => matchOf(s, network)), [file.sources, network]);
  return (
    <Panel data-testid="source-list">
      <Heading>Sources</Heading>
      {file.sources.length === 0 && <p className="text-sm text-muted-foreground">No sources yet: add one below.</p>}
      <div className="flex flex-col gap-1.5">
        {file.sources.map((s, i) => {
          const match = matches[i];
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
                  {scheduleText(s)}
                </div>
              </button>
              {states && (
                <Badge className={cn("shrink-0 border-0 font-normal", TONE[states[i]!])} data-testid={`source-state-${s.name}`}>
                  {states[i]}
                </Badge>
              )}
              {s.kind === "pulse" && s.enabled && s.scope !== "trial" && <FireButton source={s.name} ready={canFire} onError={onError} compact />}
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
        <span className="ml-1 text-xs text-muted-foreground" title="Realtime sources don't run from a file yet (phase I4)">
          realtime
        </span>
        {ADD_REALTIME.map((k) => (
          <Button key={k} variant="ghost" size="sm" disabled={locked} data-testid={`add-${k}`} aria-label={`Add ${k} source`} onClick={() => onAdd(k)}>
            <Plus data-icon="inline-start" />
            {k}
          </Button>
        ))}
      </div>
      {file.sources.some((s) => s.enabled && !SIMULATED.includes(s.kind)) && (
        <p className="text-xs text-muted-foreground">Realtime sources are kept in the file but don't run yet.</p>
      )}
    </Panel>
  );
}
