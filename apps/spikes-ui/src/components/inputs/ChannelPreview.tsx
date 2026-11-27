import { useMemo } from "react";
import { NEVER } from "rxjs";
import { AxisLocation, Chart, ContinuousAxis, type Datum, defaultLineStyle, ScatterPlot, type SeriesLineStyle, seriesFrom, TimeSeriesDataSource } from "stream-charts";
import { ZOOM, ZOOM_HELP, useChartSettings } from "../../chartSettings";
import { compile, noiseKey, sample, SignalError } from "../../signal";
import { useChartColors } from "../../theme";
import { useWidth } from "../../hooks/useWidth";

const PALETTE = ["#0072B2", "#E69F00", "#009E73", "#CC79A7", "#56B4E9", "#D55E00", "#F0E442", "#999999"];
/** Channels shown (the rest are summarised in the caption). */
const MAX_SHOWN = 8;
/** Channels assumed when there's no built network to count targets in. */
const UNKNOWN_CHANNELS = 4;

/**
 * The channels of a signal source over time: its formula sampled for the first `windowMs` of the source (t counts from
 * its start), one line per target neuron (offset for clarity). The noise is keyed as the engine keys it (the run's
 * seed and the source's name), so `rand()` and `gauss()` look as they will run.
 */
export function ChannelPreview({
  formula,
  name,
  seed,
  names,
  windowMs,
}: {
  formula: string;
  name: string;
  seed: number;
  /** The target neurons' names (channel i is the i-th), or null without a built network. */
  names: string[] | null;
  windowMs: number;
}) {
  const colors = useChartColors();
  const { zoomResets, resetZoom } = useChartSettings();
  const [ref, width] = useWidth();
  const channels = names ? names.length : UNKNOWN_CHANNELS;
  const result = useMemo(() => {
    try {
      const signal = compile(formula);
      if (channels === 0) return { empty: true as const };
      const shown = Math.min(channels, MAX_SHOWN);
      const dt = Math.max(1, windowMs / 200);
      const steps = Math.floor(windowMs / dt);
      // Every channel is sampled (the formula's `n` is the channel count), but only those shown are drawn.
      const values = sample(signal, channels, dt, 0, steps, noiseKey(seed, name));
      const series = Array.from({ length: shown }, (_, c) =>
        seriesFrom<Datum>(names?.[c] ?? `channel ${c + 1}`, Array.from({ length: steps }, (_, s) => ({ x: s * dt, y: values[s * channels + c]! + c * 1.2 }))),
      );
      const styles = new Map<string, SeriesLineStyle>(series.map((s, i) => [s.name, { ...defaultLineStyle(), color: PALETTE[i]!, lineWidth: 1.5 }]));
      return { source: new TimeSeriesDataSource({ initialData: series, generator: () => NEVER }), styles, shown };
    } catch (e) {
      return { error: e instanceof SignalError ? `${e.message} (at character ${e.position + 1})` : String(e) };
    }
  }, [formula, name, seed, names, channels, windowMs]);

  if ("error" in result) return <p className="text-xs text-destructive">The formula has a mistake: {result.error}</p>;
  if ("empty" in result) return <p className="text-xs text-muted-foreground">No neurons match this source's targets.</p>;
  return (
    <div ref={ref} data-testid="preview">
      <p className="mb-1 text-xs text-muted-foreground">
        First {windowMs / 1000} s of {result.shown < channels ? `the first ${result.shown} of ${channels}` : "all"} channels
        {names ? "" : " (no network built: showing 4)"}, offset for clarity. {ZOOM_HELP}{" "}
        <button type="button" className="underline" onClick={resetZoom} data-testid="reset-zoom">
          Reset zoom
        </button>
      </p>
      <Chart
        key={`${zoomResets}|${formula}|${channels}|${windowMs}|${seed}|${name}|${width}`}
        chartId={10}
        width={width}
        height={200}
        dataSource={result.source}
        color={colors.color}
        backgroundColor={colors.backgroundColor}
        seriesStyles={result.styles}
      >
        <ContinuousAxis axisId="x-axis-1" location={AxisLocation.Bottom} domain={[0, windowMs]} label="t (ms)" />
        <ContinuousAxis axisId="y-axis-1" location={AxisLocation.Left} domain={[-0.2, result.shown * 1.2]} label="value" />
        <ScatterPlot {...ZOOM} />
      </Chart>
    </div>
  );
}
