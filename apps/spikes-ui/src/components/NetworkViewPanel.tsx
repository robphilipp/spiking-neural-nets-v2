import type { NeuronState } from "@spikes/client";
import { ChevronDown, ChevronUp, Hammer } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { NEVER } from "rxjs";
import { Toggle } from "@/components/kit";
import { Button } from "@/components/ui/button";
import { buildNetwork } from "../buildNetwork";
import { useConnection } from "../connection";
import { useWidth } from "../hooks/useWidth";
import { connectLive, useLive } from "../live";
import { updateSettings, useProject, useSession, useSettings } from "../store";
import { Network3D } from "./Network3D";

const OPEN_KEY = "spikes.networkView.open";
const HEIGHT_KEY = "spikes.networkView.height";
const MIN_HEIGHT = 160;
/** Room for the 3D view's help line below it. */
const HINT_HEIGHT = 24;

function remembered<T>(key: string, fallback: T): T {
  try {
    const value = localStorage.getItem(key);
    return value === null ? fallback : (JSON.parse(value) as T);
  } catch {
    return fallback;
  }
}

function remember(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // Not remembered: fine.
  }
}

/** The Network page's bottom panel: the built network in 3D, as on the Run page. It expands and collapses, and its
 *  top edge drags to resize it. */
export function NetworkViewPanel() {
  const { client } = useConnection();
  const { id, network, state, build, description, applied, replayEndMs } = useSession();
  const run = useProject((s) => s.project.run);
  const background = useProject((s) => s.project.background);
  const nodesSetting = useSettings((s) => s.nodes);
  const tooltips = useSettings((s) => s.tooltips);
  const [open, setOpen] = useState(() => remembered(OPEN_KEY, false));
  const [height, setHeight] = useState(() => remembered(HEIGHT_KEY, 320));
  const [building, setBuilding] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<number[]>([]);
  const [ref, width] = useWidth();

  useEffect(() => remember(OPEN_KEY, open), [open]);
  useEffect(() => remember(HEIGHT_KEY, height), [height]);
  useEffect(() => setSelected([]), [network]);

  // Spikes flash while a run goes on: the Run page's live view (one per session and build, shared).
  useEffect(() => {
    if (open && client && id !== null && network) connectLive(client, id, build, network.neurons.length);
  }, [open, client, id, build, network]);
  const live = useLive((s) => s.view);
  const frames = live?.frames ?? NEVER;

  const queryState = useMemo(
    () => (client && id !== null && replayEndMs === null ? (neuron: number): Promise<NeuronState> => client.queryNeuron(id, neuron) : undefined),
    [client, id, replayEndMs],
  );

  // Dragging the top edge: the panel grows as the pointer moves up.
  const drag = useRef<{ y: number; height: number } | null>(null);
  const onPointerDown = (e: React.PointerEvent) => {
    drag.current = { y: e.clientY, height };
    e.currentTarget.setPointerCapture(e.pointerId);
  };
  const onPointerMove = (e: React.PointerEvent) => {
    if (!drag.current) return;
    const max = Math.max(MIN_HEIGHT, window.innerHeight - 320);
    setHeight(Math.round(Math.min(max, Math.max(MIN_HEIGHT, drag.current.height + drag.current.y - e.clientY))));
  };
  const onPointerUp = () => {
    drag.current = null;
  };

  const running = state === "running" || state === "paused";
  const stale = applied !== null && applied !== description;
  const doBuild = async () => {
    if (!client || id === null) return;
    setBuilding(true);
    setError(null);
    try {
      const replay = replayEndMs !== null;
      const nodes = replay ? [] : nodesSetting.split(",").map((n) => n.trim()).filter(Boolean);
      await buildNetwork(client, id, { shards: run.shards, seed: run.seed, nodes, background: replay ? null : background });
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBuilding(false);
    }
  };

  let status: string;
  if (network) status = `${network.neurons.length.toLocaleString()} neurons · ${network.synapseCount.toLocaleString()} synapses${stale ? " · not yet applied changes" : ""}`;
  else if (id === null) status = "Apply the network to see it";
  else status = "Not built yet";

  return (
    <div className="flex shrink-0 flex-col overflow-hidden rounded-lg border bg-card" data-testid="network-view-panel">
      {open && (
        <div
          role="separator"
          aria-orientation="horizontal"
          aria-label="Resize the 3D view"
          title="Drag to resize"
          className="h-1.5 shrink-0 cursor-row-resize touch-none bg-border/60 hover:bg-primary/40"
          onPointerDown={onPointerDown}
          onPointerMove={onPointerMove}
          onPointerUp={onPointerUp}
          onPointerCancel={onPointerUp}
          data-testid="network-view-resize"
        />
      )}
      <div className="flex items-center gap-3 px-3 py-1.5">
        <button
          type="button"
          className="flex items-center gap-1.5 text-sm font-medium"
          aria-expanded={open}
          onClick={() => setOpen(!open)}
          data-testid="network-view-toggle"
        >
          {open ? <ChevronDown className="size-4" /> : <ChevronUp className="size-4" />}
          3D view
        </button>
        <span className="flex-1 truncate text-xs text-muted-foreground" data-testid="network-view-status">
          {error ?? status}
        </span>
        {open && network && (
          <Toggle
            label="Show tooltips"
            checked={tooltips}
            onChange={(on) => updateSettings({ tooltips: on })}
            title="Show the name, position and state of the neuron under the pointer"
          />
        )}
        {id !== null && (
          <Button
            size="sm"
            variant="outline"
            disabled={!client || running || building || stale}
            title={running ? "Stop the run to rebuild" : stale ? "Apply the changes first" : "Build the applied network to see it"}
            onClick={doBuild}
            data-testid="network-view-build"
          >
            <Hammer data-icon="inline-start" />
            {building ? "Building…" : network ? "Rebuild" : "Build"}
          </Button>
        )}
      </div>
      {open && (
        <div ref={ref} className="overflow-hidden border-t px-1" style={{ height }}>
          {network ? (
            <Network3D
              network={network}
              frames={frames}
              width={width}
              height={height - HINT_HEIGHT}
              selected={selected}
              onPick={(i) => setSelected([i])}
              onRegion={setSelected}
              queryState={queryState}
              tooltips={tooltips}
              pickHint="click a neuron or shift-drag a region to show its connections."
            />
          ) : (
            <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
              {id === null ? "Apply the network, then build it to see it here." : "Build the network to see it here."}
            </div>
          )}
        </div>
      )}
    </div>
  );
}
