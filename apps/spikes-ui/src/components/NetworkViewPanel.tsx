import { type NetworkPreview, SpikesError } from "@spikes/client";
import { ChevronDown, ChevronUp, RefreshCw } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { NEVER } from "rxjs";
import { create } from "zustand";
import { Toggle } from "@/components/kit";
import { Button } from "@/components/ui/button";
import { useConnection } from "../connection";
import { useWidth } from "../hooks/useWidth";
import { parseFailed, updateSettings, useSession, useSettings } from "../store";
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

/** The last preview and the text it was made from (kept while the page is away). */
const usePreview = create<{ preview: NetworkPreview | null; text: string | null }>(() => ({ preview: null, text: null }));

const count = (n: number, inhibitory: number) =>
  n === 0 ? "none" : inhibitory === 0 ? `${n} excitatory` : inhibitory === n ? `${n} inhibitory` : `${n} (${inhibitory} inhibitory, ${n - inhibitory} excitatory)`;

/** The Network page's bottom panel: the network as written in the editor, in 3D. Update builds a preview of the
 *  editor's text (it needn't be applied; nothing runs, and the session isn't touched). The panel expands and
 *  collapses, and its top edge drags to resize it. */
export function NetworkViewPanel() {
  const { client } = useConnection();
  const description = useSession((s) => s.description);
  const tooltips = useSettings((s) => s.tooltips);
  const { preview, text } = usePreview();
  const [open, setOpen] = useState(() => remembered(OPEN_KEY, false));
  const [height, setHeight] = useState(() => remembered(HEIGHT_KEY, 320));
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [selected, setSelected] = useState<number[]>([]);
  const [ref, width] = useWidth();

  useEffect(() => remember(OPEN_KEY, open), [open]);
  useEffect(() => remember(HEIGHT_KEY, height), [height]);
  useEffect(() => setSelected([]), [preview]);

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

  const update = async () => {
    if (!client) return;
    setBusy(true);
    setError(null);
    setOpen(true);
    const from = description;
    try {
      const preview = await client.previewNetwork(from);
      usePreview.setState({ preview, text: from });
      parseFailed([]);
    } catch (e) {
      if (e instanceof SpikesError && e.error.code === "parse") {
        parseFailed(e.error.spans ?? []);
        setError(`${e.error.spans?.length ?? 0} error(s) in the description: see the editor`);
      } else {
        setError(e instanceof Error ? e.message : String(e));
      }
    } finally {
      setBusy(false);
    }
  };

  const network = preview?.network ?? null;
  let status = "Update to see the network as written";
  if (network) {
    status = `${network.neurons.length.toLocaleString()} neurons · ${network.synapseCount.toLocaleString()} connections`;
    if (text !== description) status += " · the text has changed since";
  }

  const describe = (i: number) => {
    if (!preview) return null;
    const neuron = preview.network.neurons[i]!;
    const pinned = preview.pinned[i];
    return (
      <div className="mt-1.5 grid grid-cols-[auto_1fr] gap-x-3 tabular-nums" data-testid="neuron-tooltip-preview">
        <span className="text-muted-foreground">Population</span>
        <span>{neuron.population || "—"}</span>
        <span className="text-muted-foreground">Pinned to</span>
        <span>{pinned ? `node ${pinned}` : "— (placed automatically)"}</span>
        <span className="text-muted-foreground">Incoming</span>
        <span data-testid="tooltip-incoming">{count(preview.incoming[i]!, preview.incomingInhibitory[i]!)}</span>
        <span className="text-muted-foreground">Outgoing</span>
        <span data-testid="tooltip-outgoing">{count(preview.outgoing[i]!, preview.outgoingInhibitory[i]!)}</span>
      </div>
    );
  };

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
        <span className={`flex-1 truncate text-xs ${error ? "text-destructive" : "text-muted-foreground"}`} data-testid="network-view-status">
          {error ?? status}
        </span>
        {open && network && (
          <Toggle
            label="Show tooltips"
            checked={tooltips}
            onChange={(on) => updateSettings({ tooltips: on })}
            title="Show the name, position and connections of the neuron under the pointer"
          />
        )}
        <Button
          size="sm"
          variant={network && text !== description ? "default" : "outline"}
          disabled={!client || busy || !description.trim()}
          title="Show the network as written in the editor (it needn't be applied)"
          onClick={update}
          data-testid="network-view-update"
        >
          <RefreshCw data-icon="inline-start" className={busy ? "animate-spin" : undefined} />
          {busy ? "Updating…" : "Update"}
        </Button>
      </div>
      {open && (
        <div ref={ref} className="overflow-hidden border-t px-1" style={{ height }}>
          {network ? (
            <Network3D
              network={network}
              frames={NEVER}
              width={width}
              height={height - HINT_HEIGHT}
              selected={selected}
              onPick={(i) => setSelected([i])}
              onRegion={setSelected}
              tooltips={tooltips}
              describe={describe}
              viewKey="network"
              pickHint="click a neuron or shift-drag a region to show its connections."
            />
          ) : (
            <div className="flex h-full items-center justify-center text-sm text-muted-foreground">
              Click Update to see the network as written in the editor.
            </div>
          )}
        </div>
      )}
    </div>
  );
}
