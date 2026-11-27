import { describe, expect, it } from "vitest";
import { compileMatch, describeMatch, matchNeurons } from "../src/neuronMatch";

const neurons = ["ASHL", "ASHR", "ASKL", "ASKR", "AVAL", "in-1", "in-2", "input-1"].map((name, index) => ({ name, index }));
const names = (mode: "prefix" | "regex", pattern: string) => matchNeurons(neurons, { mode, pattern }).map((n) => n.name);

describe("neuron matches", () => {
  it("picks by prefix", () => {
    expect(names("prefix", "AS")).toEqual(["ASHL", "ASHR", "ASKL", "ASKR"]);
    expect(names("prefix", "in-")).toEqual(["in-1", "in-2"]);
    expect(names("prefix", "")).toHaveLength(neurons.length);
  });

  it("picks by regular expression, anywhere in the name unless anchored", () => {
    expect(names("regex", "^AS[HK]L$")).toEqual(["ASHL", "ASKL"]);
    expect(names("regex", "L$")).toEqual(["ASHL", "ASKL", "AVAL"]);
    expect(names("regex", "^in(put)?-1$")).toEqual(["in-1", "input-1"]);
    expect(names("regex", "")).toHaveLength(neurons.length);
    // Regex characters in a prefix are plain text.
    expect(names("prefix", "in.")).toEqual([]);
  });

  it("explains an invalid regular expression", () => {
    const c = compileMatch({ mode: "regex", pattern: "AS[" });
    expect("error" in c && c.error).toMatch(/not a regular expression/);
    expect(() => names("regex", "AS[")).toThrow(/not a regular expression/);
  });

  it("describes a match for messages", () => {
    expect(describeMatch({ mode: "prefix", pattern: "in-" })).toBe("names starting with 'in-'");
    expect(describeMatch({ mode: "regex", pattern: "^AS" })).toBe("names matching /^AS/");
  });
});
