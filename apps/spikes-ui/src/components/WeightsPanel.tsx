import type { Network, ProtocolClient } from "@spikes/client";
import { useEffect, useState } from "react";
import { AxisLocation, Chart, ContinuousAxis, defaultLineStyle, ScatterPlot, type SeriesLineStyle } from "stream-charts";
import { Heading, Panel, Stat } from "@/components/kit";
import type { ChartColors } from "../theme";
import { WEIGHT_SERIES, type WeightSummary, WeightsView } from "../weights";

/** Okabe–Ito: mean, median, and the two percentiles (dashed, as a band). */
const SERIES_COLORS = ["#0072B2", "#009E73", "#E69F00", "#E69F00"];

function fmt(v: number) {
  return Math.abs(v) >= 100 ? v.toFixed(1) : v.toPrecision(3);
}

/** The current weights as a histogram. */
function Histogram({ s, width, height, colors }: { s: WeightSummary; width: number; height: number; colors: ChartColors }) {
  const margin = { left: 48, right: 16, top: 8, bottom: 36 };
  const w = width - margin.left - margin.right;
  const h = height - margin.top - margin.bottom;
  const peak = Math.max(1, ...s.counts);
  const bar = w / Math.max(1, s.counts.length);
  const ticks = [0, 0.25, 0.5, 0.75, 1].map((f) => s.lo + f * s.width * s.counts.length);
  return (
    <svg width={width} height={height} role="img" aria-label="Histogram of the current weights" data-testid="weight-histogram">
      <g transform={`translate(${margin.left},${margin.top})`}>
        {s.counts.map((c, i) => (
          <rect key={i} x={i * bar + 0.5} y={h - (h * c) / peak} width={Math.max(1, bar - 1)} height={(h * c) / peak} fill={colors.series} opacity={0.85}>
            <title>{`${fmt(s.lo + i * s.width)} – ${fmt(s.lo + (i + 1) * s.width)}: ${c.toLocaleString()} synapses`}</title>
          </rect>
        ))}
        <line x1={0} x2={w} y1={h} y2={h} stroke={colors.color} />
        {ticks.map((t, i) => (
          <text key={i} x={(i / 4) * w} y={h + 14} fontSize={10} textAnchor="middle" fill={colors.color}>
            {fmt(t)}
          </text>
        ))}
        <text x={w / 2} y={h + 30} fontSize={11} textAnchor="middle" fill={colors.color}>
          weight
        </text>
        <text x={-8} y={0} fontSize={10} textAnchor="end" dominantBaseline="hanging" fill={colors.color}>
          {peak.toLocaleString()}
        </text>
        <text x={-8} y={h} fontSize={10} textAnchor="end" fill={colors.color}>
          0
        </text>
      </g>
    </svg>
  );
}

/** Every synapse's weight, live: the distribution over time, and a histogram of the weights now. */
export function WeightsPanel(props: {
  client: ProtocolClient;
  session: number;
  network: Network;
  width: number;
  labelWidth: number;
  colors: ChartColors;
  windowMs: number;
}) {
  const { client, session, network, width, labelWidth, colors, windowMs } = props;
  const [view, setView] = useState<WeightsView | null>(null);
  const [summary, setSummary] = useState<WeightSummary | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    let v: WeightsView | null = null;
    let cancelled = false;
    const subscription = { unsubscribe: () => {} };
    WeightsView.attach(client, session, network)
      .then((created) => {
        if (cancelled) return void created.dispose();
        v = created;
        setView(created);
        const sub = created.summary.subscribe(setSummary);
        subscription.unsubscribe = () => sub.unsubscribe();
      })
      .catch((e: Error) => setError(e.message));
    return () => {
      cancelled = true;
      subscription.unsubscribe();
      setView(null);
      setSummary(null);
      void v?.dispose();
    };
  }, [client, session, network]);

  if (!network.synapseCount) return null;
  const listed = network.synapses.length === network.synapseCount;
  const styles = new Map<string, SeriesLineStyle>(
    WEIGHT_SERIES.map((name, i) => [name, { ...defaultLineStyle(), color: SERIES_COLORS[i]!, highlightColor: colors.highlight, lineWidth: i < 2 ? 2 : 1 }]),
  );
  const histogramWidth = Math.min(420, Math.max(240, Math.floor(width * 0.35)));
  const trendWidth = width > 760 ? width - histogramWidth - 16 : width;

  return (
    <Panel data-testid="weights">
      <div className="flex flex-wrap items-baseline gap-x-6 gap-y-1">
        <Heading>Synapse weights</Heading>
        {summary && (
          <>
            <Stat label={listed ? "Synapses" : "Synapses that changed"} value={summary.count.toLocaleString()} />
            <Stat label="Changed since build" value={summary.changed.toLocaleString()} />
            <Stat label="Mean" value={summary.count ? fmt(summary.mean) : "—"} />
            <Stat label="Median" value={summary.count ? fmt(summary.p50) : "—"} />
            <Stat label="10th – 90th percentile" value={summary.count ? `${fmt(summary.p10)} – ${fmt(summary.p90)}` : "—"} />
          </>
        )}
      </div>
      {error && <p className="text-sm text-destructive">{error}</p>}
      {!listed && (
        <p className="text-xs text-muted-foreground">
          This network has {network.synapseCount.toLocaleString()} synapses, too many to list: the chart follows the ones that learning changes.
        </p>
      )}
      {summary && summary.changed === 0 && listed && (
        <p className="text-xs text-muted-foreground">
          No weight has changed yet: weights change only through learning rules (<code>lrn=</code> on connections), and only while the network runs.
        </p>
      )}
      <div className="flex flex-wrap gap-4">
        <div data-testid="weight-trend">
          <div className="mb-1 flex flex-wrap gap-3 text-xs text-muted-foreground">
            {WEIGHT_SERIES.map((name, i) => (
              <span key={name} className="flex items-center gap-1">
                <span className="inline-block h-0.5 w-4" style={{ background: SERIES_COLORS[i] }} />
                {name}
              </span>
            ))}
          </div>
          {view && (
            <Chart
              chartId={5}
              width={trendWidth}
              height={220}
              dataSource={view.trend}
              color={colors.color}
              backgroundColor={colors.backgroundColor}
              seriesStyles={styles}
              windowingTime={100}
              dataUpdatePeriod={50}
              margin={{ left: labelWidth, right: 20, bottom: 48 }}
            >
              <ContinuousAxis axisId="x-axis-1" location={AxisLocation.Bottom} domain={[0, windowMs]} label="t (ms)" />
              <ContinuousAxis axisId="y-axis-1" location={AxisLocation.Left} domain={[0, 2]} label="weight" autoScale />
              <ScatterPlot />
            </Chart>
          )}
        </div>
        {summary && summary.count > 0 && <Histogram s={summary} width={histogramWidth} height={240} colors={colors} />}
      </div>
    </Panel>
  );
}
