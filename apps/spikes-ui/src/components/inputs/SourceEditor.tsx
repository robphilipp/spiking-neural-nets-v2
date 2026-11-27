import type { Network } from "@spikes/client";
import { ChevronDown, ChevronUp, Copy, Trash2 } from "lucide-react";
import { useMemo } from "react";
import { Choice, Field, Heading, Panel, Row, Toggle } from "@/components/kit";
import { Button } from "@/components/ui/button";
import { changeSource, duplicateSource, moveSource, parseTimes, removeSource, timesText } from "../../inputEdit";
import { type InputFile, InputFileError, isRealtime, type Random, type RandomField, type Source } from "../../inputFile";
import { type LoadedData } from "../../inputData";
import { compile, SignalError } from "../../signal";
import { EncodingFields } from "./EncodingFields";
import { ParsedField, type Setting, SettingField } from "./ParsedField";
import { FileFields } from "./FileFields";
import { LayoutFields } from "./LayoutFields";
import { RealtimeFields } from "./RealtimeFields";
import { TargetsField } from "./TargetsField";

/** What a source can do, for the heading of its settings. */
const KINDS: Record<Source["kind"], string> = {
  pulse: "Pulse: one stimulus at each of its times",
  poisson: "Poisson: random events at a rate, like background input",
  periodic: "Periodic: a stimulus every period",
  signal: "Signal: a formula over time and channel, encoded as input",
  file: "File: a data file (CSV or WAV) played in simulated time, encoded as input",
  microphone: "Microphone: the audio spectrum in log-spaced bands, live",
  camera: "Camera: a luminance grid or frame differences, live",
  sensor: "Sensor: any program streaming samples to it by name, live",
};

/** The settings that are random per trial can change to or from a random value: set `field` of the source. */
function withRandom(s: Source, field: RandomField, value: number | null, random: Random | undefined): Partial<Source> {
  const rest = { ...s.random };
  if (random) rest[field] = random;
  else delete rest[field];
  const random_ = Object.keys(rest).length ? rest : undefined;
  return { [field]: value, random: random_ } as Partial<Source>;
}

/**
 * The editor of one source of the file: its name, targets, schedule and the settings of its kind. `locked` (a run is
 * open) makes everything read-only: a run is described by its input file and seed.
 */
export function SourceEditor({
  file,
  index,
  onChange,
  onSelect,
  network,
  locked,
  trials,
  data,
}: {
  file: InputFile;
  index: number;
  onChange: (file: InputFile) => void;
  /** After a move, duplicate or removal: the source to show next. */
  onSelect: (index: number) => void;
  network: Network | null;
  locked: boolean;
  trials: boolean;
  /** The input file's data files, read. */
  data: LoadedData;
}) {
  const s = file.sources[index]!;
  const set = (change: Partial<Source>) => onChange(changeSource(file, index, change));
  const setting = (field: RandomField, value: number | null): Setting => ({ value, random: s.random?.[field] });
  /** A setting that always has a value (the field can't be emptied): keeps `fallback` if it's somehow null. */
  const num = (field: RandomField, fallback: number) => (x: Setting) => set(withRandom(s, field, x.value ?? fallback, x.random));
  const formula = s.kind === "signal" ? s.formula : "";
  const formulaError = useMemo(() => {
    if (s.kind !== "signal") return null;
    try {
      const c = compile(formula);
      const missing = c.dataUses.find((u) => !data.sets.has(u.name));
      if (missing) return `data("${missing.name}", …): ${data.errors.get(missing.name) ?? `there's no data file called '${missing.name}'`}`;
      return null;
    } catch (e) {
      return e instanceof SignalError ? `${e.message} (at character ${e.position + 1})` : String(e);
    }
  }, [s.kind, formula, data]);
  const realtime = isRealtime(s);

  return (
    <Panel data-testid="source-editor">
      <div className="flex items-center gap-2">
        <Heading className="flex-1">{KINDS[s.kind]}</Heading>
        <Button variant="ghost" size="icon-sm" aria-label="Move up" disabled={locked || index === 0} onClick={() => { onChange(moveSource(file, index, -1)); onSelect(index - 1); }}>
          <ChevronUp />
        </Button>
        <Button variant="ghost" size="icon-sm" aria-label="Move down" disabled={locked || index === file.sources.length - 1} onClick={() => { onChange(moveSource(file, index, 1)); onSelect(index + 1); }}>
          <ChevronDown />
        </Button>
        <Button variant="ghost" size="icon-sm" aria-label="Duplicate source" disabled={locked} data-testid="duplicate-source" onClick={() => { onChange(duplicateSource(file, index)); onSelect(index + 1); }}>
          <Copy />
        </Button>
        <Button variant="ghost" size="icon-sm" aria-label="Remove source" disabled={locked} data-testid="remove-source" onClick={() => { onChange(removeSource(file, index)); onSelect(Math.max(0, index - 1)); }}>
          <Trash2 />
        </Button>
      </div>
      {/* A run is described by its input file and seed: while one is open, nothing here changes. */}
      <fieldset disabled={locked} className="m-0 flex min-w-0 flex-col gap-3 border-0 p-0">
        <Row>
          <ParsedField<string>
            label="Name"
            width={200}
            value={s.name}
            format={(v) => v}
            parse={(text) => {
              const name = text.trim();
              if (!name) throw new InputFileError("a source needs a name");
              if (file.sources.some((x, i) => i !== index && x.name === name)) throw new InputFileError(`there's already a source called ${name}`);
              return name;
            }}
            onChange={(name) => set({ name })}
            data-testid="source-name"
          />
          <Toggle label="Enabled" checked={s.enabled} onChange={(enabled) => set({ enabled })} testId="source-enabled" title="A disabled source stays in the file but isn't given to the run" />
          {trials && !realtime && (
            <Choice
              label="Runs"
              width={150}
              value={s.scope === "trial" ? "trial" : "run"}
              onChange={(v) => set({ scope: v === "trial" ? "trial" : undefined })}
              options={[
                { value: "run", label: "the whole run" },
                { value: "trial", label: "in each trial" },
              ]}
              testId="source-scope"
            />
          )}
        </Row>
        <TargetsField targets={s.targets} network={network} onChange={(targets) => set({ targets })} />
        {/* A realtime source has no schedule: it streams while the run goes. */}
        {!realtime && (
        <Row>
          <SettingField label={s.scope === "trial" ? "Start (in the trial)" : "Start"} time width={170} setting={setting("start", s.start)} onChange={num("start", s.start)} testId="source-start" />
          {s.kind !== "pulse" && (
            <SettingField
              label="Duration"
              time
              optional
              width={170}
              setting={setting("duration", s.duration)}
              onChange={(x) => set(withRandom(s, "duration", x.value, x.random))}
              placeholder={s.scope === "trial" ? "to the trial's end" : "for ever"}
              testId="source-duration"
            />
          )}
          {s.kind === "pulse" && (
            <ParsedField<number[]>
              label="Pulse times (from the start)"
              width={260}
              value={s.at}
              format={timesText}
              parse={(text) => parseTimes(text, "a pulse time")}
              onChange={(at) => set({ at })}
              placeholder="e.g. 1 s, 1.5 s, 2 s"
              data-testid="pulse-times"
            />
          )}
          {s.kind === "pulse" && <SettingField label="mV" width={90} setting={setting("mv", s.mv)} onChange={num("mv", s.mv)} />}
          {s.kind === "poisson" && (
            <>
              <SettingField label="Hz per neuron" min={0.001} width={130} setting={setting("hz", s.hz)} onChange={num("hz", s.hz)} />
              <SettingField label="mV" width={90} setting={setting("mv", s.mv)} onChange={num("mv", s.mv)} />
            </>
          )}
          {s.kind === "periodic" && (
            <>
              <SettingField label="Period" time width={130} setting={setting("period", s.period)} onChange={num("period", s.period)} />
              <SettingField label="mV" width={90} setting={setting("mv", s.mv)} onChange={num("mv", s.mv)} />
            </>
          )}
        </Row>
        )}
        {s.kind === "signal" && (
          <>
            <Field
              label="Value of channel c at time t (ms from the start); n channels"
              value={s.formula}
              onChange={(e) => set({ formula: e.target.value })}
              error={formulaError !== null}
              help={formulaError ?? "e.g. pulse(t, 200, 50) · sin(2*pi*t/500 + c) · floor(t/100) % 7 == c % 7 · t > 300 ? 1 : 0 · rand() · data(\"name\", t, c): a data file"}
              data-testid="formula"
              spellCheck={false}
              className="[&_input]:font-mono"
            />
            <Row>
              <ParsedField<number>
                label="Sample interval"
                width={130}
                value={s.sample}
                format={(v) => `${v} ms`}
                parse={(text) => {
                  const m = /^\s*(\d+(?:\.\d*)?|\.\d+)\s*(ms|s)?\s*$/.exec(text);
                  const v = m ? Number(m[1]) * (m[2] === "s" ? 1000 : 1) : NaN;
                  if (!(v > 0)) throw new InputFileError("the sample interval must be a positive time like 1 ms");
                  return v;
                }}
                onChange={(sample) => set({ sample })}
              />
              <EncodingFields encoding={s.encoding} onChange={(encoding) => set({ encoding } as Partial<Source>)} />
              <ParsedField<number | undefined>
                label="Channels"
                width={110}
                value={s.channels}
                format={(v) => (v === undefined ? "" : String(v))}
                parse={(text) => {
                  if (text.trim() === "") return undefined;
                  const n = Number(text);
                  if (!Number.isInteger(n) || n < 1) throw new InputFileError("channels must be a whole number from 1, or empty for one per target neuron");
                  return n;
                }}
                onChange={(channels) => set({ channels } as Partial<Source>)}
                placeholder="per target"
                data-testid="signal-channels"
              />
            </Row>
            <LayoutFields s={s} set={set} network={network} data={data} />
          </>
        )}
        {s.kind === "file" && <FileFields s={s} file={file} set={set} network={network} data={data} />}
        {realtime && <RealtimeFields s={s} set={set} network={network} />}
      </fieldset>
    </Panel>
  );
}
