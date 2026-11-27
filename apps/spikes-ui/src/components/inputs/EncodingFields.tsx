import type { Encoding } from "@spikes/client";
import { Choice, NumberField } from "@/components/kit";

const DEFAULTS: Record<Encoding["type"], Encoding> = {
  current: { type: "current", gainMv: 10 },
  rate: { type: "rate", maxHz: 100, mv: 12 },
  threshold: { type: "threshold", level: 0.1, mv: 12 },
  spikes: { type: "spikes", mv: 12 },
};

/** How samples become input to the neurons: the encoding's kind and its settings (the engine's four; a formula's
 *  values can't be spikes, so `spikes` is offered to realtime sources only). */
export function EncodingFields({ encoding, onChange, spikes = false }: { encoding: Encoding; onChange: (e: Encoding) => void; spikes?: boolean }) {
  return (
    <>
      <Choice
        label="Encoding"
        width={170}
        value={encoding.type}
        onChange={(type) => onChange(DEFAULTS[type])}
        options={[
          { value: "rate", label: "rate (Poisson)" },
          { value: "current", label: "current" },
          { value: "threshold", label: "threshold (delta)" },
          ...(spikes ? [{ value: "spikes" as const, label: "spikes" }] : []),
        ]}
        testId="encoding"
      />
      {encoding.type === "current" && <NumberField label="mV per unit" width={110} value={encoding.gainMv} onChange={(gainMv) => onChange({ ...encoding, gainMv })} />}
      {encoding.type === "rate" && <NumberField label="Hz at 1" width={100} min={0.001} value={encoding.maxHz} onChange={(maxHz) => onChange({ ...encoding, maxHz })} />}
      {encoding.type === "threshold" && <NumberField label="Change per event" width={130} min={0.000001} value={encoding.level} onChange={(level) => onChange({ ...encoding, level })} />}
      {encoding.type !== "current" && <NumberField label="mV per event" width={110} value={encoding.mv} onChange={(mv) => onChange({ ...encoding, mv })} />}
    </>
  );
}
