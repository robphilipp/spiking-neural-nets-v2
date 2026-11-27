import type { Network, ProtocolClient } from "@spikes/client";
import * as d3 from "d3";
import { useEffect, useId, useMemo, useRef, useState } from "react";
import {
  AxisLocation,
  Chart,
  ContinuousAxis,
  defaultLineStyle,
  ScatterPlot,
  type SeriesLineStyle,
} from "stream-charts";
import { Heading, Panel, Stat } from "@/components/kit";
import type { ChartColors } from "../theme";
import { useCadence, useChartSettings, useChartKey, useTimeDomain, ZOOM } from "../chartSettings";
import { useSession } from "../store";
import { useBehavior } from "../useBehavior";
import {
  connectWeights,
  useWeights,
  WEIGHT_SERIES,
  type WeightSummary,
} from "../weights";

/** Okabe–Ito: mean, median, and the two percentiles (dashed, as a band). */
const SERIES_COLORS = ["#0072B2", "#009E73", "#E69F00", "#E69F00"];

function fmt(v: number) {
  return Math.abs(v) >= 100 ? v.toFixed(1) : v.toPrecision(3);
}

/** The current weights as a histogram. */
/** The weights now, as a histogram; its weight axis pans (drag) and zooms (shift + scroll) like the charts. */
function Histogram({
  s,
  width,
  height,
  colors,
  zoomResets,
}: {
  s: WeightSummary;
  width: number;
  height: number;
  colors: ChartColors;
  zoomResets: number;
}) {
  const margin = { left: 48, right: 16, top: 8, bottom: 36 };
  const w = width - margin.left - margin.right;
  const h = height - margin.top - margin.bottom;
  const peak = Math.max(1, ...s.counts);
  const svgRef = useRef<SVGSVGElement>(null);
  const [transform, setTransform] = useState(d3.zoomIdentity);
  const zoom = useMemo(
    () =>
      d3
        .zoom<SVGSVGElement, unknown>()
        .scaleExtent([1, 200])
        // Scrolling zooms only with shift or ctrl held (as the charts do), so the page still scrolls.
        .filter((e: Event) =>
          e.type === "wheel"
            ? (e as WheelEvent).shiftKey || (e as WheelEvent).ctrlKey
            : !(e as MouseEvent).button,
        )
        .on("zoom", (e: d3.D3ZoomEvent<SVGSVGElement, unknown>) =>
          setTransform(e.transform),
        ),
    [],
  );
  useEffect(() => {
    zoom
      .extent([
        [0, 0],
        [w, h],
      ])
      .translateExtent([
        [0, 0],
        [w, h],
      ]);
    const svg = d3.select(svgRef.current!);
    svg.call(zoom);
    return () => void svg.on(".zoom", null);
  }, [zoom, w, h]);
  useEffect(() => {
    if (svgRef.current)
      d3.select(svgRef.current).call(zoom.transform, d3.zoomIdentity);
  }, [zoom, zoomResets]);

  const hi = s.lo + s.width * s.counts.length;
  const x = transform.rescaleX(
    d3.scaleLinear().domain([s.lo, hi]).range([0, w]),
  );
  const clip = `histogram-clip-${useId().replace(/:/g, "")}`;
  return (
    <svg
      ref={svgRef}
      width={width}
      height={height}
      role="img"
      aria-label="Histogram of the current weights"
      data-testid="weight-histogram"
      style={{ cursor: "grab" }}
    >
      <defs>
        <clipPath id={clip}>
          <rect width={w} height={h} />
        </clipPath>
      </defs>
      <g transform={`translate(${margin.left},${margin.top})`}>
        {/* Catches drags over the empty parts of the plot. */}
        <rect width={w} height={h} fill="transparent" />
        <g clipPath={`url(#${clip})`}>
          {s.counts.map((c, i) => {
            const x0 = x(s.lo + i * s.width);
            const x1 = x(s.lo + (i + 1) * s.width);
            if (x1 < 0 || x0 > w) return null;
            return (
              <rect
                key={i}
                x={x0 + 0.5}
                y={h - (h * c) / peak}
                width={Math.max(1, x1 - x0 - 1)}
                height={(h * c) / peak}
                fill={colors.series}
                opacity={0.85}
              >
                <title>{`${fmt(s.lo + i * s.width)} – ${fmt(s.lo + (i + 1) * s.width)}: ${c.toLocaleString()} synapses`}</title>
              </rect>
            );
          })}
        </g>
        <line x1={0} x2={w} y1={h} y2={h} stroke={colors.color} />
        {x.ticks(5).map((t) => (
          <text
            key={t}
            x={x(t)}
            y={h + 14}
            fontSize={10}
            textAnchor="middle"
            fill={colors.color}
          >
            {fmt(t)}
          </text>
        ))}
        <text
          x={w / 2}
          y={h + 30}
          fontSize={11}
          textAnchor="middle"
          fill={colors.color}
        >
          weight
        </text>
        <text
          x={-8}
          y={0}
          fontSize={10}
          textAnchor="end"
          dominantBaseline="hanging"
          fill={colors.color}
        >
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
  const { client, session, network, width, labelWidth, colors, windowMs } =
    props;
  const timeDomain = useTimeDomain(windowMs);
  const chartKey = useChartKey(windowMs);
  // The view lives in a store (one per session and build), so adding a connection doesn't restart it.
  const build = useSession((s) => s.build);
  useEffect(
    () =>
      connectWeights(
        client,
        session,
        build,
        useSession.getState().network ?? network,
      ),
    [client, session, build],
  );
  const view = useWeights((s) => s.view);
  const error = useWeights((s) => s.error);
  const summary = useBehavior(view?.summary);
  const { bufferMs, updateMs, zoomResets } = useChartSettings();
  const { cadence } = useCadence();

  if (!network.synapseCount) return null;
  const listed = network.synapses.length === network.synapseCount;
  const styles = new Map<string, SeriesLineStyle>(
    WEIGHT_SERIES.map((name, i) => [
      name,
      {
        ...defaultLineStyle(),
        color: SERIES_COLORS[i]!,
        highlightColor: colors.highlight,
        lineWidth: i < 2 ? 2 : 1,
      },
    ]),
  );
  const histogramWidth = Math.min(420, Math.max(240, Math.floor(width * 0.35)));
  const trendWidth = width > 760 ? width - histogramWidth - 16 : width;

  return (
    <Panel data-testid="weights">
      <div className="flex flex-wrap items-baseline gap-x-6 gap-y-1">
        <Heading>Synapse weights</Heading>
        {summary && (
          <>
            <Stat
              label={listed ? "Synapses" : "Synapses that changed"}
              value={summary.count.toLocaleString()}
            />
            <Stat
              label="Changed since build"
              value={summary.changed.toLocaleString()}
            />
            <Stat
              label="Mean"
              value={summary.count ? fmt(summary.mean) : "—"}
            />
            <Stat
              label="Median"
              value={summary.count ? fmt(summary.p50) : "—"}
            />
            <Stat
              label="10th – 90th percentile"
              value={
                summary.count
                  ? `${fmt(summary.p10)} – ${fmt(summary.p90)}`
                  : "—"
              }
            />
          </>
        )}
      </div>
      {error && <p className="text-sm text-destructive">{error}</p>}
      {!listed && (
        <p className="text-xs text-muted-foreground">
          This network has {network.synapseCount.toLocaleString()} synapses, too
          many to list: the chart follows the ones that learning changes.
        </p>
      )}
      {summary && summary.changed === 0 && listed && (
        <p className="text-xs text-muted-foreground">
          No weight has changed yet: weights change only through learning rules
          (<code>lrn=</code> on connections), and only while the network runs.
        </p>
      )}
      <div className="flex flex-wrap gap-4">
        <div data-testid="weight-trend" data-time-domain={timeDomain.join(",")}>
          <div className="mb-1 flex flex-wrap gap-3 text-xs text-muted-foreground">
            {WEIGHT_SERIES.map((name, i) => (
              <span key={name} className="flex items-center gap-1">
                <span
                  className="inline-block h-0.5 w-4"
                  style={{ background: SERIES_COLORS[i] }}
                />
                {name}
              </span>
            ))}
          </div>
          {view && (
            <Chart
              key={chartKey}
              chartId={5}
              width={trendWidth}
              height={220}
              dataSource={view.trend}
              color={colors.color}
              backgroundColor={colors.backgroundColor}
              seriesStyles={styles}
              windowingTime={bufferMs}
              dataUpdatePeriod={updateMs}
              margin={{ left: labelWidth, right: 20, bottom: 48 }}
            >
              <ContinuousAxis
                axisId="x-axis-1"
                location={AxisLocation.Bottom}
                domain={timeDomain}
                label="t (ms)"
              />
              <ContinuousAxis
                axisId="y-axis-1"
                location={AxisLocation.Left}
                domain={[0, 2]}
                label="weight"
                autoScale
              />
              <ScatterPlot withCadence={cadence} {...ZOOM} />
            </Chart>
          )}
        </div>
        {summary && summary.count > 0 && (
          <Histogram
            s={summary}
            width={histogramWidth}
            height={240}
            colors={colors}
            zoomResets={zoomResets}
          />
        )}
      </div>
    </Panel>
  );
}
