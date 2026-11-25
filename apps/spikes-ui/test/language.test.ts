import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { applicableKeys, hover, scan, suggestions } from "../src/language/spikes";

const at = (text: string) => {
  const offset = text.indexOf("|");
  return { text: text.replace("|", ""), offset };
};
const labels = (text: string) => {
  const { text: t, offset } = at(text);
  return suggestions(t, offset).map((s) => s.label);
};

describe("context", () => {
  it("finds the record and what is being written", () => {
    const { text, offset } = at("(synapse_types=[(name=s, weight=1, limits=(min=0, |");
    const ctx = scan(text, offset);
    expect(ctx.mode).toBe("key");
    expect(ctx.node?.record).toBe("limits");
    expect(ctx.node?.parent?.values.get("name")).toBe("s");
  });

  it("knows a value is being written", () => {
    const { text, offset } = at("(neurons=[(name=a, dynamics=bi|");
    const ctx = scan(text, offset);
    expect(ctx).toMatchObject({ mode: "value", key: "dynamics", prefix: "bi" });
  });

  it("reads points, calls and lists of names as single values", () => {
    const neuron = "(neurons=[(name=a, position=[0, 10 µm, 2], |";
    const first = scan(at(neuron).text, at(neuron).offset);
    expect(first.mode).toBe("key");
    expect(first.node?.record).toBe("neuron");
    expect(first.node?.values.get("position")).toBe("[0, 10 µm, 2]");
    const text =
      "(neurons=[(name=a, position=[0, 0, 0]), (name=b, position=(r=40, phi=2.1, z=0))], connections=[(from=[a, b{1:2:1}], to=a, weight=uniform(0, 2), ";
    const ctx = scan(text);
    expect(ctx.mode).toBe("key");
    expect(ctx.node?.record).toBe("connection");
    expect(ctx.node?.values.get("weight")).toBe("uniform(0, 2)");
    expect(ctx.node?.values.get("from")).toBe("[a, b{1:2:1}]");
  });

  it("skips comments and id sets", () => {
    const { text, offset } = at("(\n// neurons=[( not real\n/* sets=[(\n */ connections=[(from={a, b}, to=c, |");
    expect(scan(text, offset).node?.record).toBe("connection");
  });

  it("scans every example network to the end without getting lost", () => {
    for (const sub of ["networks", "examples"]) {
      const dir = join(__dirname, "..", "..", "..", "test-data", sub);
      for (const file of readdirSync(dir).filter((f) => f.endsWith(".boo"))) {
        const text = readFileSync(join(dir, file), "utf8");
        expect(scan(text).mode, file).toBe("outside");
        expect(scan(text).root?.children.length, file).toBeGreaterThan(0);
      }
    }
  });
});

describe("suggestions", () => {
  it("offer the keys that apply and aren't written yet", () => {
    const keys = labels("(neurons=[(name=a, threshold=1 mV, |");
    expect(keys).toContain("refractory_period");
    expect(keys).toContain("position");
    expect(keys).toContain("type");
    expect(keys).not.toContain("threshold");
  });

  it("follow the record's kind", () => {
    expect(labels("(neurons=[(name=a, dynamics=bistable_integrator, |")).toContain("tonic_on_threshold");
    expect(labels("(neurons=[(name=a, dynamics=bistable_integrator, |")).not.toContain("threshold");
    expect(labels("(learning_rules=[(name=x, rule=stdp_alpha, |")).toEqual(expect.arrayContaining(["baseline", "time", "rate"]));
    expect(labels("(learning_rules=[(name=x, rule=stdp_alpha, |")).not.toContain("depression");
  });

  it("offer choices, booleans and names defined in the document", () => {
    expect(labels("(neurons=[(name=a, dynamics=|")).toEqual([
      "monostable_integrator",
      "bistable_integrator",
      "monostable_resonator",
      "bistable_resonator",
    ]);
    expect(labels("(neurons=[(name=a, inhibitory=|")).toEqual(["true", "false"]);
    const doc = "(neurons=[(name=in-1), (name=out-1)], populations=[(name=exc)], sets=[(name=inputs)], connections=[(from=|";
    expect(labels(doc)).toEqual(["in-1", "out-1", "exc", "inputs"]);
    const rules = "(neurons=[(name=a)], learning_rules=[(name=fast, rule=stdp_hard)], connections=[(from=a, to=a, learning=|";
    expect(labels(rules)).toEqual(["fast", "flat"]);
    const types = "(synapse_types=[(name=plastic), (name=fixed)], connections=[(from=a, to=a, type=|";
    expect(labels(types)).toEqual(["plastic", "fixed"]);
    expect(labels("(nodes=[(name=camera)], neurons=[(name=a, node=|")).toEqual(["camera", "main"]);
  });

  it("offer a new record inside a list, with its required keys", () => {
    const { text, offset } = at("(neurons=[|");
    const [s] = suggestions(text, offset);
    expect(s?.insert).toMatch(/^\(name=.*position=/);
  });
});

describe("hover", () => {
  it("explains a key", () => {
    const text = "(neurons=[(name=a, decay_time=250 ms)])";
    const h = hover(text, text.indexOf("decay_time") + 1);
    expect(h?.markdown).toContain("decay time constant");
    expect(h?.markdown).toContain("default 250 ms");
  });

  it("knows which record a key belongs to", () => {
    // `time` means different things in a weight decay and in an alpha learning rule.
    const text = "(synapse_types=[(name=s, decay=(time=1 s))], learning_rules=[(name=x, rule=stdp_alpha, time=5 ms)])";
    expect(hover(text, text.indexOf("time") + 1)?.markdown).toContain("Decay time constant");
    expect(hover(text, text.lastIndexOf("time") + 1)?.markdown).toContain("Alpha time constant");
  });

  it("ignores values and unknown words", () => {
    const text = "(neurons=[(name=alpha)])";
    expect(hover(text, text.indexOf("alpha") + 1)).toBeNull();
  });
});

describe("applicable keys", () => {
  it("all of a record's keys before its kind is written", () => {
    const text = "(neurons=[(name=a, ";
    const ctx = scan(text, text.length);
    expect(applicableKeys(ctx.node!).map((k) => k.key)).toEqual(expect.arrayContaining(["threshold", "tonic_on_threshold"]));
  });
});
