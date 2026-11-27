import type { Network } from "@spikes/client";
import { Choice, NumberField, Notice, Row } from "@/components/kit";
import { type LoadedData } from "../../inputData";
import { type InputFile, InputFileError, type Source } from "../../inputFile";
import { EncodingFields } from "./EncodingFields";
import { LayoutFields } from "./LayoutFields";
import { ParsedField } from "./ParsedField";

type FileSource = Extract<Source, { kind: "file" }>;

/** "3 channels · 2.5 s · 1 kHz" for a data set. */
export function describeData(d: { channels: unknown[]; durationMs: number; rateHz: number | null; length: number }): string {
  const rate = d.rateHz === null ? `${d.length} rows` : `${Number((d.rateHz / 1000).toPrecision(4))} kHz`;
  return `${d.channels.length} channel${d.channels.length === 1 ? "" : "s"} · ${Number((d.durationMs / 1000).toPrecision(4))} s · ${rate}`;
}

/**
 * The settings of a `file` source: which of the input file's data files it plays (in simulated time, from the source's
 * start), what the file holds, gain, sample interval, channel limit, encoding and the position mapping.
 */
export function FileFields({
  s,
  file,
  set,
  network,
  data,
}: {
  s: FileSource;
  file: InputFile;
  set: (change: Partial<Source>) => void;
  network: Network | null;
  data: LoadedData;
}) {
  const refs = file.data ?? [];
  const listed = refs.some((d) => d.name === s.data);
  const found = data.sets.get(s.data);
  const problem = data.errors.get(s.data) ?? (listed ? null : `there's no data file called '${s.data}': add one in the input file's data files above`);
  return (
    <>
      <Row>
        <Choice<string>
          label="Data file"
          width={200}
          value={listed ? s.data : ""}
          onChange={(name) => set({ data: name } as Partial<Source>)}
          options={[...(listed ? [] : [{ value: "", label: s.data || "(choose)" }]), ...refs.map((d) => ({ value: d.name, label: d.name }))]}
          testId="file-data"
        />
        <NumberField label="Gain" width={90} value={s.gain} onChange={(gain) => set({ gain } as Partial<Source>)} />
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
          onChange={(sample) => set({ sample } as Partial<Source>)}
        />
        <ParsedField<number | undefined>
          label="Channels (at most)"
          width={140}
          value={s.channels}
          format={(v) => (v === undefined ? "" : String(v))}
          parse={(text) => {
            if (text.trim() === "") return undefined;
            const n = Number(text);
            if (!Number.isInteger(n) || n < 1) throw new InputFileError("channels must be a whole number from 1, or empty for all the file's");
            return n;
          }}
          onChange={(channels) => set({ channels } as Partial<Source>)}
          placeholder="all"
          data-testid="file-channels"
        />
      </Row>
      <Row>
        <EncodingFields encoding={s.encoding} onChange={(encoding) => set({ encoding } as Partial<Source>)} />
      </Row>
      {problem ? (
        <Notice tone="error" data-testid="file-problem">
          {problem}
        </Notice>
      ) : (
        found && (
          <p className="text-xs text-muted-foreground" data-testid="file-info">
            {s.data}: {describeData(found)}. Played from the source's start, linearly interpolated at each sample, 0 after its end.
          </p>
        )
      )}
      <LayoutFields s={s} set={set} network={network} data={data} />
    </>
  );
}
