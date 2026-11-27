import type { Network } from "@spikes/client";
import { Eye, EyeOff } from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { Notice } from "@/components/kit";
import { Button } from "@/components/ui/button";
import { type Capture, openCapture } from "../../capture";
import { type RealtimeSource, targetNeurons } from "../../inputFile";
import { cellsOf, channelPlan, previewChannels } from "../../realtimeChannels";

/** The pixel size of a camera preview's side (the grid is scaled to fit). */
const SIZE = 192;

/** The channel values (sample-major) of the last sample of a batch. */
const lastSample = (values: Float32Array, channels: number) => values.slice(values.length - channels);

/** Draws a camera's grid: brightness as grey, or for frame differences the "on" cells green and the "off" cells red. */
function draw(canvas: HTMLCanvasElement, s: Extract<RealtimeSource, { kind: "camera" }>, values: Float32Array) {
  const { cols, rows } = s.grid;
  const cells = cellsOf(s.grid);
  const ctx = canvas.getContext("2d")!;
  const image = ctx.createImageData(cols, rows);
  for (let i = 0; i < cells; i++) {
    const [r, g, b] =
      s.mode === "luminance"
        ? [values[i]! * 255, values[i]! * 255, values[i]! * 255]
        : [Math.min(1, (values[cells + i] ?? 0) * 2) * 230, Math.min(1, values[i]! * 2) * 200, 20];
    image.data.set([r, g, b, 255], i * 4);
  }
  ctx.putImageData(image, 0, 0);
}

/**
 * A live look at what a microphone or camera source will send: the microphone's bands as bars, the camera's grid (for
 * frame differences, the "on" cells green and the "off" cells red). It opens the same capture the run uses, from the
 * same channel code, only when switched on (the browser may ask for the device), and lets go when switched off or
 * when the source's settings change.
 */
export function RealtimePreview({ source, network }: { source: Exclude<RealtimeSource, { kind: "sensor" }>; network: Network | null }) {
  const [on, setOn] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [frame, setFrame] = useState<Float32Array | null>(null);
  const canvas = useRef<HTMLCanvasElement>(null);
  // Channels as the run would send them: by the targets in the built network, else as many as the settings say.
  const channels = useMemo(() => {
    if (network) {
      try {
        const plan = channelPlan(source, targetNeurons(source.targets, network).length);
        if ("channels" in plan) return plan.channels;
      } catch {
        // an unknown set or bad pattern: the settings alone decide
      }
    }
    return previewChannels(source);
  }, [source, network]);
  const settings = JSON.stringify([source.kind, "mode" in source && source.mode, "grid" in source && source.grid, "level" in source && source.level, channels]);

  useEffect(() => {
    if (!on) return;
    let capture: Capture | undefined;
    let cancelled = false;
    let pending = 0;
    let latest: Float32Array | null = null;
    setError(null);
    openCapture(source, channels).then(
      (c) => {
        if (cancelled) return c.stop();
        capture = c;
        c.start((_, __, values) => {
          latest = lastSample(values, c.channels);
          pending ||= requestAnimationFrame(() => {
            pending = 0;
            setFrame(latest);
          });
        });
      },
      (e: Error) => {
        if (cancelled) return;
        setError(e.message);
        setOn(false);
      },
    );
    return () => {
      cancelled = true;
      capture?.stop();
      cancelAnimationFrame(pending);
      setFrame(null);
    };
    // The capture follows the settings that shape it (not the whole source: its encoding doesn't change what it sees).
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [on, settings]);

  useEffect(() => {
    if (frame && canvas.current && source.kind === "camera") draw(canvas.current, source, frame);
  }, [frame, source]);

  return (
    <div className="grid gap-2" data-testid="realtime-preview">
      <div className="flex items-center gap-2">
        <Button variant="outline" size="sm" onClick={() => setOn(!on)} data-testid="preview-toggle" aria-pressed={on}>
          {on ? <EyeOff data-icon="inline-start" /> : <Eye data-icon="inline-start" />}
          {on ? "Stop preview" : "Preview capture"}
        </Button>
        <span className="text-xs text-muted-foreground">
          {on ? `what the ${source.kind} sends: ${channels} channel${channels === 1 ? "" : "s"}` : `shows what the ${source.kind} will send, using the device`}
        </span>
      </div>
      {error && (
        <Notice tone="error" onClose={() => setError(null)} data-testid="preview-error">
          {error}
        </Notice>
      )}
      {on && source.kind === "microphone" && (
        <div className="flex h-24 items-end gap-px rounded border bg-muted/30 p-1" data-testid="preview-bars" aria-label="Microphone band levels">
          {Array.from({ length: channels }, (_, c) => (
            <div key={c} className="min-w-px flex-1 rounded-t-sm bg-primary" style={{ height: `${Math.max(1, (frame?.[c] ?? 0) * 100)}%` }} />
          ))}
        </div>
      )}
      {on && source.kind === "camera" && (
        <canvas
          ref={canvas}
          width={source.grid.cols}
          height={source.grid.rows}
          className="rounded border bg-black"
          style={{ width: SIZE, height: (SIZE * source.grid.rows) / source.grid.cols, imageRendering: "pixelated" }}
          data-testid="preview-canvas"
          aria-label="Camera grid"
        />
      )}
    </div>
  );
}
