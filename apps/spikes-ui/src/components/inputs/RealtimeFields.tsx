import type { Network } from "@spikes/client";
import { Choice, NumberField, Notice, Row } from "@/components/kit";
import { InputFileError, type OnPause, type RealtimeSource, type Source, type SpeedRule, formatGrid, parseGrid } from "../../inputFile";
import { sourceChannels } from "../../sourceChannels";
import { LayoutFields } from "./LayoutFields";
import { EncodingFields } from "./EncodingFields";
import { ParsedField } from "./ParsedField";
import { RealtimePreview } from "./RealtimePreview";

/**
 * The settings of a realtime source (microphone, camera or sensor): what it captures (a microphone's bands, a camera's
 * mode, grid and level), its encoding, and what to do while the run is paused and at other speeds than ×1. Below
 * them, how many channels it sends to how many target neurons (or why that can't be), and a live preview of the
 * capture. Edits go to `set`.
 */
export function RealtimeFields({ s, set, network }: { s: RealtimeSource; set: (change: Partial<Source>) => void; network: Network | null }) {
  return (
    <>
      <Row>
        {s.kind === "microphone" && (
          <ParsedField<number | undefined>
            label="Bands (at most)"
            width={130}
            value={s.bands}
            format={(v) => (v === undefined ? "" : String(v))}
            parse={(text) => {
              if (text.trim() === "") return undefined;
              const n = Number(text);
              if (!Number.isInteger(n) || n < 1) throw new InputFileError("bands must be a whole number from 1, or empty for one per target neuron");
              return n;
            }}
            onChange={(bands) => set({ bands } as Partial<Source>)}
            placeholder="one per target"
            data-testid="mic-bands"
          />
        )}
        {s.kind === "camera" && (
          <>
            <Choice
              label="Mode"
              width={190}
              value={s.mode}
              onChange={(mode) => set({ mode } as Partial<Source>)}
              options={[
                { value: "luminance", label: "luminance (brightness grid)" },
                { value: "changes", label: "changes (on and off)" },
              ]}
              testId="camera-mode"
            />
            <ParsedField
              label="Grid (columns×rows)"
              width={150}
              value={s.grid}
              format={formatGrid}
              parse={(text) => parseGrid(text, "the grid")}
              onChange={(grid) => set({ grid } as Partial<Source>)}
              data-testid="camera-grid"
            />
            {s.mode === "changes" && (
              <NumberField label="Level (brightness change)" width={170} min={0.001} max={1} value={s.level} onChange={(level) => set({ level } as Partial<Source>)} />
            )}
          </>
        )}
      </Row>
      <Row>
        <EncodingFields encoding={s.encoding} spikes={s.kind === "sensor"} onChange={(encoding) => set({ encoding } as Partial<Source>)} />
      </Row>
      <Row>
        <Choice<OnPause>
          label="While paused"
          width={190}
          value={s.onPause}
          onChange={(onPause) => set({ onPause } as Partial<Source>)}
          options={[
            { value: "drop", label: "drop what arrives" },
            { value: "buffer", label: "buffer it for the resume" },
          ]}
          testId="on-pause"
        />
        <Choice<SpeedRule>
          label="At other speeds than ×1"
          width={230}
          value={s.speed}
          onChange={(speed) => set({ speed } as Partial<Source>)}
          options={[
            { value: "resample", label: "scale its time by the speed" },
            { value: "refuse", label: "refuse: don't start" },
          ]}
          testId="speed-rule"
        />
      </Row>
      {s.kind !== "sensor" && <LayoutFields s={s} set={set} network={network} data={null} />}
      {s.kind === "sensor" && network && <SensorPlan s={s} network={network} />}
      {s.kind === "sensor" ? (
        <Notice data-testid="sensor-note">
          A program streams into this source: it finds the sensor named <code>{s.name}</code> once the run is going (the engine's <code>listSensors</code>, or{" "}
          <code>claimSensor</code>) and sends samples to it, one channel per target neuron. See docs/inputs-sensors.md.
        </Notice>
      ) : (
        <RealtimePreview source={s} network={network} />
      )}
    </>
  );
}

/** A sensor has one channel per target neuron. */
function SensorPlan({ s, network }: { s: RealtimeSource; network: Network }) {
  const r = sourceChannels(s, network, null);
  if (!("targets" in r)) return null;
  const n = r.targets.length;
  return (
    <p className={n === 0 ? "text-xs text-destructive" : "text-xs text-muted-foreground"} data-testid="channel-plan">
      {n === 0 ? "sensor has no target neurons. The source won't start." : `${n} channel${n === 1 ? "" : "s"} for ${n} target neuron${n === 1 ? "" : "s"}`}
    </p>
  );
}
