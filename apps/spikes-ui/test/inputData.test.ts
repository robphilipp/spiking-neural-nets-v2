import type { Network } from "@spikes/client";
import { describe, expect, it } from "vitest";
import { addDataRef, dataName, newSource, removeDataRef } from "../src/inputEdit";
import { loadData, requestData } from "../src/inputData";
import { fromBase64 } from "../src/dataFile";
import { engineSources, fileFormula, InputFileError, parseInputFile, serializeInputFile } from "../src/inputFile";

const enc = (s: string) => new TextEncoder().encode(s);

const FILE = `
[[data]]
name = "odour"
file = "data/odour.csv"

[[data]]
name = "unused"
file = "unused.csv"

[[source]]
name = "smell"
kind = "file"
data = "odour"
gain = 2
sample = "2 ms"
channels = 2
neurons = { prefix = "n" }
encoding = { kind = "rate", max_hz = 100, mv = 12 }
start = "100 ms"

[[source]]
name = "mix"
kind = "signal"
neurons = { prefix = "n" }
formula = 'data("odour", t, c) + 1'
encoding = { kind = "current", gain_mv = 5 }
`.replace("'", "'");

const network = {
  neurons: Array.from({ length: 6 }, (_, i) => ({ index: i, name: `n${i}`, position: [i, 0, 0] })),
  sets: [],
} as unknown as Network;

const CSV = enc("t,a,b,c\n0,0,1,2\n100,1,1,2\n");

describe("data references in input files", () => {
  it("read [[data]] and file sources, and round trip", () => {
    const f = parseInputFile(FILE.replace("'data(\"odour\", t, c) + 1'", '"data(\\"odour\\", t, c) + 1"'));
    expect(f.data).toEqual([
      { name: "odour", file: "data/odour.csv" },
      { name: "unused", file: "unused.csv" },
    ]);
    expect(f.sources[0]).toMatchObject({ kind: "file", data: "odour", gain: 2, sample: 2, channels: 2, start: 100 });
    expect(parseInputFile(serializeInputFile(f))).toEqual(f);
    expect(serializeInputFile(f)).toMatch(/^\[\[data\]\]\nname = "odour"\nfile = "data\/odour.csv"/);
  });

  it("don't write a file source's defaults", () => {
    const f = parseInputFile(
      '[[data]]\nname = "d"\nfile = "d.csv"\n\n[[source]]\nname = "s"\nkind = "file"\ndata = "d"\nneurons = { prefix = "" }\nencoding = { kind = "rate", max_hz = 1, mv = 1 }\n',
    );
    expect(f.sources[0]).toMatchObject({ gain: 1, sample: 1 });
    const text = serializeInputFile(f);
    expect(text).not.toContain("gain");
    expect(text).not.toContain("sample");
  });

  it("reject paths outside the folder, duplicate names and unlisted data", () => {
    const d = (t: string) => `[[data]]\n${t}\n`;
    expect(() => parseInputFile(d('name = "a"\nfile = "../a.csv"'))).toThrow(/inside the project folder/);
    expect(() => parseInputFile(d('name = "a"\nfile = "/etc/a.csv"'))).toThrow(/inside the project folder/);
    expect(() => parseInputFile(d('name = "a"\nfile = "C:\\\\a.csv"'))).toThrow(/inside the project folder/);
    expect(() => parseInputFile(d('name = "a"'))).toThrow(/file must be a path/);
    expect(() => parseInputFile(d('file = "a.csv"'))).toThrow(/name must be text/);
    expect(() => parseInputFile(d('name = "a"\nfile = "a"') + d('name = "a"\nfile = "b"'))).toThrow(/two data files are named 'a'/);
    const src = '[[source]]\nname = "s"\nkind = "file"\ndata = "nope"\nneurons = { prefix = "" }\nencoding = { kind = "rate", max_hz = 1, mv = 1 }\n';
    expect(() => parseInputFile(src)).toThrow(/uses data 'nope', which isn't in a \[\[data\]\] table/);
    const sig = '[[source]]\nname = "s"\nkind = "signal"\nformula = "data(\\"zz\\", t)"\nneurons = { prefix = "" }\nencoding = { kind = "rate", max_hz = 1, mv = 1 }\n';
    expect(() => parseInputFile(sig)).toThrow(/uses data 'zz'/);
  });
});

describe("data for the engine", () => {
  const file = () => parseInputFile(FILE.replace("'data(\"odour\", t, c) + 1'", '"data(\\"odour\\", t, c) + 1"'));

  it("loads the files that are there, and says what's wrong with the others", () => {
    const loaded = loadData(file(), { "data/odour.csv": CSV, "unused.csv": enc("x,y\n1,a\n") });
    expect([...loaded.sets.keys()]).toEqual(["odour"]);
    expect(loaded.errors.get("unused")).toBe("data file 'unused': line 2, column 2: 'a' is not a number");
    expect(loadData(file(), {}).errors.get("odour")).toMatch(/data\/odour.csv isn't loaded/);
  });

  it("turns a file source into a signal over data(…), with the file's channels", () => {
    const loaded = loadData(file(), { "data/odour.csv": CSV });
    const sources = engineSources(file(), network, loaded.sets);
    expect(sources[0]).toEqual({
      name: "smell",
      startMs: 100,
      input: {
        type: "signal",
        channels: [[0], [1]], // limited to 2 of the file's 3
        formula: 'data("odour", t, c) * 2',
        sampleMs: 2,
        encoding: { type: "rate", maxHz: 100, mv: 12 },
        durationMs: null,
      },
    });
    // The signal that uses data() gets one channel per target neuron, as any signal.
    expect(sources[1]!.input).toMatchObject({ type: "signal", channels: [[0], [1], [2], [3], [4], [5]] });
    expect(fileFormula({ data: "x", gain: 1 })).toBe('data("x", t, c)');
  });

  it("names the source for missing data and too few targets", () => {
    expect(() => engineSources(file(), network, new Map())).toThrow(/input 'smell': the data file 'odour' isn't loaded/);
    const loaded = loadData(file(), { "data/odour.csv": CSV });
    const f = file();
    (f.sources[0] as { channels?: number }).channels = undefined;
    (f.sources[0] as { targets: unknown }).targets = { prefix: "n0" }; // one neuron for 3 channels
    expect(() => engineSources(f, network, loaded.sets)).toThrow(/input 'smell': file needs 3 target neurons, has 1/);
  });

  it("maps a file's channels by position", () => {
    const f = file();
    (f.sources[0] as { layout?: unknown }).layout = { kind: "axis", axis: "x" };
    const loaded = loadData(f, { "data/odour.csv": CSV });
    const input = engineSources(f, network, loaded.sets)[0]!.input;
    expect(input.type === "signal" && input.channels).toEqual([[0, 1, 2], [3, 4, 5]]); // 6 neurons in 2 slabs (channels = 2)
  });

  it("sends the data the sources use, base64, and not too much", () => {
    const bytes = { "data/odour.csv": CSV, "unused.csv": enc("t,a\n0,1\n") };
    const sent = requestData(file(), bytes);
    expect(sent.map((d) => d.name)).toEqual(["odour"]); // "unused" isn't read by any source
    expect(new TextDecoder().decode(fromBase64(sent[0]!.base64))).toBe("t,a,b,c\n0,0,1,2\n100,1,1,2\n");
    const off = file();
    off.sources.forEach((s) => (s.enabled = false));
    expect(requestData(off, bytes)).toEqual([]);
    const big = { "data/odour.csv": new Uint8Array(41 * 1024 * 1024) };
    expect(() => requestData(file(), big)).toThrow(/at most 40 MB/);
  });
});

describe("editing data references", () => {
  it("names a data file after its file, uniquely", () => {
    expect(dataName("sub/Odour Step.csv", [])).toBe("Odour Step");
    expect(dataName("a.csv", ["a"])).toBe("a 2");
    expect(dataName('we"ird.wav', [])).toBe("weird");
  });

  it("adds and removes", () => {
    const f = { description: "", sources: [] };
    const one = addDataRef(f, "odour.csv");
    expect(one).toEqual({ name: "odour", file: { description: "", sources: [], data: [{ name: "odour", file: "odour.csv" }] } });
    expect(addDataRef(one.file, "odour.csv").file).toBe(one.file); // already there
    const two = addDataRef(one.file, "x/odour.wav");
    expect(two.name).toBe("odour 2");
    expect(removeDataRef(two.file, "odour").data).toEqual([{ name: "odour 2", file: "x/odour.wav" }]);
    expect("data" in removeDataRef(one.file, "odour")).toBe(false);
  });

  it("makes a new file source with defaults", () => {
    expect(newSource("file", [])).toMatchObject({ kind: "file", data: "", gain: 1, sample: 1 });
  });
});

describe("InputFileError for data", () => {
  it("is thrown for a source without data", () => {
    expect(() => parseInputFile('[[source]]\nname = "s"\nkind = "file"\nneurons = { prefix = "" }\nencoding = { kind = "rate", max_hz = 1, mv = 1 }\n')).toThrow(InputFileError);
  });
});
