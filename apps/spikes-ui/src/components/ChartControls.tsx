import { RotateCcw } from "lucide-react";
import { useState } from "react";
import { Choice, Field, Row, Toggle } from "@/components/kit";
import { Button } from "@/components/ui/button";
import { KEEP_CHOICES, useChartSettings } from "../chartSettings";

/** A whole number of milliseconds, typed freely: the setting changes once the text is a valid value. */
function MillisField({
  label,
  value,
  onChange,
  min,
  help,
  testId,
}: {
  label: string;
  value: number;
  onChange: (ms: number) => void;
  min: number;
  help: string;
  testId: string;
}) {
  const [text, setText] = useState(String(value));
  // Follow changes made elsewhere (e.g. a reset), while keeping what's being typed.
  const [shown, setShown] = useState(value);
  if (value !== shown) {
    setShown(value);
    setText(String(value));
  }
  const valid = (t: string) => Number.isInteger(Number(t)) && Number(t) >= min && t.trim() !== "";
  return (
    <Field
      label={label}
      width={110}
      inputMode="numeric"
      value={text}
      title={help}
      error={!valid(text)}
      data-testid={testId}
      onChange={(e) => {
        setText(e.target.value);
        if (valid(e.target.value)) onChange(Number(e.target.value));
      }}
      onBlur={() => setText(String(value))}
    />
  );
}

/**
 * How the live charts take in and keep data (as in stream-charts' examples): buffering (how long a chart gathers
 * data before drawing it), the update rate (how often the engine sends it), cadence (smooth scrolling between
 * updates) and how much simulated time is kept. They apply to every live chart, at once.
 */
export function ChartControls() {
  const { bufferMs, updateMs, cadence, keepMs, setBufferMs, setUpdateMs, setCadence, setKeepMs, reset } = useChartSettings();
  return (
    <Row data-testid="chart-controls">
      <MillisField
        label="Buffering (ms)"
        value={bufferMs}
        min={1}
        onChange={setBufferMs}
        help="How long a chart gathers incoming data before drawing it"
        testId="buffering"
      />
      <MillisField
        label="Update rate (ms)"
        value={updateMs}
        min={10}
        onChange={setUpdateMs}
        help="How often the engine sends the charts' data"
        testId="update-rate"
      />
      <Choice
        label="Keep"
        width={110}
        value={keepMs}
        onChange={setKeepMs}
        testId="keep"
        options={KEEP_CHOICES.map((ms) => ({ value: ms, label: `${ms / 1000} s` }))}
      />
      <Toggle
        label="Cadence"
        checked={cadence}
        onChange={setCadence}
        testId="cadence"
        title="Move the time axes on every animation frame, so the charts scroll smoothly between updates"
      />
      <Button variant="ghost" size="icon-sm" title="Reset the chart settings" aria-label="Reset the chart settings" onClick={reset}>
        <RotateCcw />
      </Button>
    </Row>
  );
}
