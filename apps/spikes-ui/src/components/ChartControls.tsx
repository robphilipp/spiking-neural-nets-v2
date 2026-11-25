import { RotateCcw, ZoomOut } from "lucide-react";
import { Choice, NumberField, Row, Toggle } from "@/components/kit";
import { Button } from "@/components/ui/button";
import { KEEP_CHOICES, useChartSettings, ZOOM_HELP } from "../chartSettings";

/**
 * How the live charts take in and keep data (as in stream-charts' examples): buffering (how long a chart gathers
 * data before drawing it), the update rate (how often the engine sends it), cadence (smooth scrolling between
 * updates) and how much simulated time is kept. They apply to every live chart, at once. Reset zoom puts every
 * chart's pan and zoom back.
 */
export function ChartControls() {
  const {
    bufferMs,
    updateMs,
    cadence,
    keepMs,
    setBufferMs,
    setUpdateMs,
    setCadence,
    setKeepMs,
    reset,
    resetZoom,
  } = useChartSettings();
  return (
    <Row data-testid="chart-controls">
      <NumberField
        label="Buffering (ms)"
        width={110}
        integer
        min={1}
        value={bufferMs}
        onChange={setBufferMs}
        title="How long a chart gathers incoming data before drawing it"
        data-testid="buffering"
      />
      <NumberField
        label="Update rate (ms)"
        width={110}
        integer
        min={10}
        value={updateMs}
        onChange={setUpdateMs}
        title="How often the engine sends the charts' data"
        data-testid="update-rate"
      />
      <Choice
        label="Keep"
        width={110}
        value={keepMs}
        onChange={setKeepMs}
        testId="keep"
        options={KEEP_CHOICES.map((ms) => ({
          value: ms,
          label: `${ms / 1000} s`,
        }))}
      />
      <Toggle
        label="Cadence"
        checked={cadence}
        onChange={setCadence}
        testId="cadence"
        title="Move the time axes on every animation frame, so the charts scroll smoothly between updates (the membrane potential chart always does)"
      />
      <Button
        variant="ghost"
        size="icon-sm"
        title="Reset the chart settings"
        aria-label="Reset the chart settings"
        onClick={reset}
      >
        <RotateCcw />
      </Button>
      <Button
        variant="outline"
        size="sm"
        title={`${ZOOM_HELP} This puts every chart back.`}
        onClick={resetZoom}
        data-testid="reset-zoom"
      >
        <ZoomOut /> Reset zoom
      </Button>
      <span className="pb-1.5 text-xs text-muted-foreground">{ZOOM_HELP}</span>
    </Row>
  );
}
