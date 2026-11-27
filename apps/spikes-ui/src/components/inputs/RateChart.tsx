import { useMemo, useRef } from "react";
import { NEVER } from "rxjs";
import { AxisLocation, Chart, ContinuousAxis, type Datum, defaultLineStyle, ScatterPlot, type SeriesLineStyle, seriesFrom, TimeSeriesDataSource } from "stream-charts";
import { useWidth } from "../../hooks/useWidth";
import { useChartColors } from "../../theme";

/** Colours of the groups (and conditions), the same in every chart of a test. */
export const PALETTE = ["#0072B2", "#E69F00", "#009E73", "#CC79A7", "#56B4E9", "#D55E00", "#F0E442", "#999999"];
export const colorOf = (i: number) => PALETTE[i % PALETTE.length]!;

/** The chart's margins around its plot, px: fixed, so overlays can sit exactly over the plot. */
let charts = 0;

const MARGIN = { top: 8, right: 14, bottom: 42, left: 58 };

export interface RateSeries {
  name: string;
  color: string;
  /** Rate per bin, spikes per neuron per second. */
  rates: number[];
}

/** A stretch of the time axis to mark (a trial). */
export interface Band {
  fromMs: number;
  toMs: number;
  /** Shown at the top of the band. */
  label: string;
  color: string;
}

/**
 * Rates over time, a line per series (a point per bin, at the bin's middle), with bands marked behind the lines
 * (trials, in their condition's colour). The chart can't be zoomed, so the bands, drawn over the plot area, stay
 * aligned with it.
 */
export function RateChart({
  series,
  binMs,
  domainMs,
  bands = [],
  height = 220,
  testId,
}: {
  series: RateSeries[];
  binMs: number;
  domainMs: number;
  bands?: Band[];
  height?: number;
  testId: string;
}) {
  const colors = useChartColors();
  const chartId = useRef(100 + charts++).current;
  const [ref, width] = useWidth();
  const top = Math.max(1, ...series.flatMap((s) => s.rates)) * 1.1;
  const data = useMemo(() => {
    const lines = series.map((s) =>
      seriesFrom<Datum>(
        s.name,
        s.rates.map((y, i) => ({ x: Math.min(domainMs, (i + 0.5) * binMs), y })),
      ),
    );
    const styles = new Map<string, SeriesLineStyle>(series.map((s) => [s.name, { ...defaultLineStyle(), color: s.color, lineWidth: 1.5 }]));
    return { source: new TimeSeriesDataSource({ initialData: lines, generator: () => NEVER }), styles };
  }, [series, binMs, domainMs]);
  const plotWidth = Math.max(1, width - MARGIN.left - MARGIN.right);
  const x = (ms: number) => MARGIN.left + (Math.min(Math.max(ms, 0), domainMs) / domainMs) * plotWidth;
  return (
    <div ref={ref} className="relative" style={{ height }} data-testid={testId}>
      {bands.length > 0 && (
        <div className="pointer-events-none absolute inset-0" aria-hidden>
          {bands.map((b, i) => (
            <div
              key={i}
              className="absolute overflow-hidden border-l text-[10px] leading-3 whitespace-nowrap text-muted-foreground"
              style={{
                left: x(b.fromMs),
                width: Math.max(1, x(b.toMs) - x(b.fromMs)),
                top: MARGIN.top,
                height: height - MARGIN.top - MARGIN.bottom,
                background: `color-mix(in srgb, ${b.color} 14%, transparent)`,
                borderColor: `color-mix(in srgb, ${b.color} 55%, transparent)`,
              }}
              data-testid={`${testId}-band`}
              title={b.label}
            >
              <span className="px-0.5">{b.label}</span>
            </div>
          ))}
        </div>
      )}
      <Chart
        key={`${width}|${domainMs}|${binMs}|${series.length}|${series.map((s) => s.rates.join(",")).join(";")}`}
        chartId={chartId}
        width={width}
        height={height}
        margin={MARGIN}
        dataSource={data.source}
        color={colors.color}
        backgroundColor="transparent"
        seriesStyles={data.styles}
      >
        <ContinuousAxis axisId="x-axis-1" location={AxisLocation.Bottom} domain={[0, domainMs]} label="t (ms)" />
        <ContinuousAxis axisId="y-axis-1" location={AxisLocation.Left} domain={[0, top]} label="Hz per neuron" />
        <ScatterPlot />
      </Chart>
    </div>
  );
}
