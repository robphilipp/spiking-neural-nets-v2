import { cn } from "@/lib/utils";
import { formatTime } from "../../inputFile";
import type { Timeline as TimelineData } from "../../inputEdit";

/** Tick labels: seconds, with decimals only when needed ("0", "0.5 s", "2 s"). */
const tick = (ms: number) => (ms === 0 ? "0" : ms >= 1000 ? `${ms / 1000} s` : `${ms} ms`);

/** Trial boundaries are drawn on every row up to this many trials (beyond that, only in the trials row). */
const MAX_BOUNDARIES = 150;

/**
 * When each source is on across the run: a row per source (bars for its on-periods, ticks for pulses; an arrow-edged
 * bar is on for ever, faded when the source is disabled; a realtime source has a "live" row without a schedule), the trials above them (numbered, with their
 * conditions) and a marker at the run's current time. Clicking a row selects its source.
 */
export function Timeline({
  data,
  selected,
  onSelect,
  nowMs,
}: {
  data: TimelineData;
  selected: string | null;
  onSelect: (name: string) => void;
  nowMs: number | null;
}) {
  const h = data.horizonMs;
  const x = (ms: number) => `${(Math.min(ms, h) / h) * 100}%`;
  const boundaries = data.trials.length <= MAX_BOUNDARIES;
  const now = nowMs !== null && nowMs > 0 && nowMs <= h ? x(nowMs) : null;
  return (
    <div className="grid grid-cols-[9rem_1fr] gap-x-3 text-xs" data-testid="timeline" data-horizon-ms={h}>
      {data.trials.length > 0 && (
        <>
          <div className="self-center text-muted-foreground">trials</div>
          <div className="relative h-6 rounded bg-muted/40" data-testid="timeline-trials">
            {data.trials.map((t) => (
              <div
                key={t.number}
                className="absolute top-0 flex h-full items-center justify-center overflow-hidden border-l border-background bg-muted text-[10px] text-muted-foreground"
                style={{ left: x(t.startMs), width: `calc(${x(t.endMs)} - ${x(t.startMs)})` }}
                title={`trial ${t.number}${t.condition ? ` · ${t.condition}` : ""}: ${formatTime(Math.round(t.startMs))} to ${formatTime(Math.round(t.endMs))}`}
                data-testid="timeline-trial"
              >
                {t.number}
              </div>
            ))}
          </div>
        </>
      )}
      {data.rows.map((r) => (
        <div key={r.name} className="contents" data-testid={`timeline-row-${r.name}`}>
          <button
            type="button"
            className={cn("truncate py-1 text-left", r.name === selected ? "font-medium text-foreground" : "text-muted-foreground hover:text-foreground")}
            onClick={() => onSelect(r.name)}
            title={r.name}
          >
            {r.name}
          </button>
          <div
            className={cn("relative my-0.5 h-5 cursor-pointer rounded bg-muted/40", r.name === selected && "ring-1 ring-primary")}
            onClick={() => onSelect(r.name)}
          >
            {boundaries &&
              data.trials.map((t) => <div key={t.number} className="absolute top-0 h-full w-px bg-border" style={{ left: x(t.startMs) }} />)}
            {r.spans.map((p, i) => (
              <div
                key={i}
                data-testid="timeline-span"
                className={cn(
                  "absolute top-1 h-3 rounded-sm bg-primary",
                  !r.enabled && "opacity-30",
                  p.open && "rounded-r-none bg-gradient-to-r from-primary to-primary/25",
                )}
                style={{ left: x(p.from), width: `max(2px, calc(${x(p.to)} - ${x(p.from)}))` }}
              />
            ))}
            {r.live && (
              <div className={cn("absolute inset-0 flex items-center px-2 text-[10px] text-muted-foreground", !r.enabled && "opacity-30")} data-testid="timeline-live">
                live: streams in real time while the run goes
              </div>
            )}
            {r.marks.map((t, i) => (
              <div
                key={i}
                data-testid="timeline-mark"
                className={cn("absolute top-0.5 h-4 w-0.5 rounded bg-primary", !r.enabled && "opacity-30")}
                style={{ left: x(t) }}
              />
            ))}
            {now && <div className="absolute top-0 h-full w-px bg-destructive" style={{ left: now }} />}
          </div>
        </div>
      ))}
      <div />
      <div className="relative mt-0.5 h-4 border-t text-[10px] text-muted-foreground">
        {data.ticks.map((t) => (
          <span key={t} className="absolute top-0.5 -translate-x-1/2 whitespace-nowrap tabular-nums first:translate-x-0" style={{ left: x(t) }}>
            {tick(t)}
          </span>
        ))}
      </div>
    </div>
  );
}
