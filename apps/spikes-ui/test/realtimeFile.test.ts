import { describe, expect, it } from "vitest";
import { type InputFile, InputFileError, parseGrid, parseInputFile, serializeInputFile } from "../src/inputFile";

const one = (body: string) => `[[source]]\nname = "s"\nneurons = { prefix = "in-" }\n${body}`;
const parse = (body: string) => parseInputFile(one(body)).sources[0]!;

describe("realtime sources in an input file", () => {
  it("read their defaults", () => {
    expect(parse('kind = "microphone"\nencoding = { kind = "rate", max_hz = 200, mv = 12 }')).toMatchObject({
      kind: "microphone",
      onPause: "drop",
      speed: "resample",
      start: 0,
      duration: null,
      encoding: { type: "rate", maxHz: 200, mv: 12 },
    });
    expect(parse('kind = "microphone"\nencoding = { kind = "rate", max_hz = 200, mv = 12 }')).not.toHaveProperty("bands");
    expect(parse('kind = "camera"\nencoding = { kind = "current", gain_mv = 10 }')).toMatchObject({
      kind: "camera",
      mode: "luminance",
      grid: { cols: 16, rows: 16 },
      level: 0.1,
    });
    expect(parse('kind = "sensor"\nencoding = { kind = "spikes", mv = 12 }')).toMatchObject({ kind: "sensor", encoding: { type: "spikes", mv: 12 } });
  });

  it("read every setting", () => {
    const s = parse(
      'kind = "camera"\nmode = "changes"\ngrid = "8x6"\nlevel = 0.25\non_pause = "buffer"\nspeed = "refuse"\nencoding = { kind = "threshold", level = 0.5, mv = 3 }',
    );
    expect(s).toMatchObject({ mode: "changes", grid: { cols: 8, rows: 6 }, level: 0.25, onPause: "buffer", speed: "refuse", encoding: { type: "threshold" } });
    expect(parse('kind = "microphone"\nbands = 24\nencoding = { kind = "current", gain_mv = 1 }')).toMatchObject({ bands: 24 });
  });

  it("write only what differs from the defaults, and read back what they wrote", () => {
    const text = `
[[source]]
name = "eye"
kind = "camera"
neurons = { prefix = "retina" }
encoding = { kind = "rate", max_hz = 100, mv = 12 }

[[source]]
name = "event eye"
kind = "camera"
neurons = { prefix = "retina" }
mode = "changes"
grid = "32x24"
level = 0.2
on_pause = "buffer"
speed = "refuse"
encoding = { kind = "rate", max_hz = 100, mv = 12 }

[[source]]
name = "voice"
kind = "microphone"
neurons = { prefix = "AW" }
bands = 12
encoding = { kind = "current", gain_mv = 4 }
enabled = false

[[source]]
name = "arm"
kind = "sensor"
neurons = { set = "motor" }
encoding = { kind = "spikes", mv = 15 }
`;
    const file = parseInputFile(text);
    const written = serializeInputFile(file);
    expect(parseInputFile(written)).toEqual(file);
    // The first camera is all defaults: no mode level, on_pause or speed lines for it.
    const first = written.split("[[source]]")[1]!;
    expect(first).toContain('grid = "16x16"');
    expect(first).not.toMatch(/level|on_pause|speed|start|duration/);
    expect(written).toContain('on_pause = "buffer"');
    expect(written).toContain('speed = "refuse"');
    expect(written).toContain("bands = 12");
  });

  it("keep files from before realtime sources run (a bare microphone source) parsing", () => {
    const f = parseInputFile('[[source]]\nname = "voice"\nkind = "microphone"\nneurons = { prefix = "AW" }\nencoding = { kind = "rate", max_hz = 200, mv = 12 }\non_pause = "drop"\nenabled = false');
    expect(f.sources[0]).toMatchObject({ kind: "microphone", enabled: false, onPause: "drop" });
  });

  it("say what is wrong", () => {
    const bad = (body: string, message: RegExp) => expect(() => parse(body)).toThrow(message);
    const enc = 'encoding = { kind = "rate", max_hz = 1, mv = 1 }';
    bad(`kind = "microphone"\non_pause = "keep"\n${enc}`, /on_pause must be "drop" or "buffer"/);
    bad(`kind = "microphone"\nspeed = "fast"\n${enc}`, /speed must be "resample" or "refuse"/);
    bad(`kind = "camera"\nmode = "colour"\n${enc}`, /mode must be "luminance" or "changes"/);
    bad(`kind = "camera"\ngrid = "16"\n${enc}`, /grid must be columns×rows/);
    bad(`kind = "camera"\ngrid = "0x4"\n${enc}`, /grid must be columns×rows/);
    bad(`kind = "camera"\nlevel = 0\n${enc}`, /level isn't valid/);
    bad(`kind = "microphone"\nbands = 0\n${enc}`, /bands isn't valid/);
    bad('kind = "microphone"\nencoding = { kind = "spikes", mv = 1 }', /microphone's encoding is current, rate or threshold/);
    bad('kind = "camera"\nencoding = { kind = "spikes", mv = 1 }', /camera's encoding is current, rate or threshold/);
    bad(`kind = "microphone"\nstart = "1 s"\n${enc}`, /no start or duration/);
    bad(`kind = "sensor"\nduration = "1 s"\n${enc}`, /no start or duration/);
    bad(`kind = "camera"\ngrid = "500x500"\n${enc}`, /each from 1 to 128/);
  });

  it("can't be trial sources", () => {
    const text = `[trials]\ncount = 2\nlength = "1 s"\n\n${one('kind = "microphone"\nscope = "trial"\nencoding = { kind = "rate", max_hz = 1, mv = 1 }')}`;
    expect(() => parseInputFile(text)).toThrow(/streams for the whole run; it can't be a trial source/);
  });

  it("parse grids", () => {
    expect(parseGrid("16x16", "g")).toEqual({ cols: 16, rows: 16 });
    expect(parseGrid(" 4 × 3 ", "g")).toEqual({ cols: 4, rows: 3 });
    expect(() => parseGrid("big", "g")).toThrow(InputFileError);
  });

  it("round-trip through a file with simulated sources and trials", () => {
    const file: InputFile = parseInputFile(`${one('kind = "pulse"\nat = ["1 s"]\nmv = 5')}\n\n[[source]]\nname = "voice"\nkind = "microphone"\nneurons = { prefix = "AW" }\nencoding = { kind = "rate", max_hz = 200, mv = 12 }`);
    expect(parseInputFile(serializeInputFile(file))).toEqual(file);
  });
});
