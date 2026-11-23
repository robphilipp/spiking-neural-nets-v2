import { readFileSync, readdirSync } from "node:fs";
import { join } from "node:path";
import { describe, expect, it } from "vitest";
import { applicableKeys, hover, kindOf, scan, suggestions } from "../src/language/spikes";

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
    const { text, offset } = at("(NRN=[(nid=a, mst=1 mV, WDF=(fnc=exp, |");
    const ctx = scan(text, offset);
    expect(ctx.mode).toBe("key");
    expect(ctx.node?.record).toBe("weightDecay");
    expect(kindOf(ctx.node!)).toBe("exp");
    expect(ctx.node?.parent?.values.get("nid")).toBe("a");
  });

  it("knows a value is being written", () => {
    const { text, offset } = at("(NRN=[(nid=a, nty=b|");
    const ctx = scan(text, offset);
    expect(ctx).toMatchObject({ mode: "value", key: "nty", prefix: "b" });
  });

  it("skips comments and id sets", () => {
    const { text, offset } = at("(\n// NRN=[( not real\n/* LRN=[(\n */ CON=[(prn={a, b}, psn=c, |");
    expect(scan(text, offset).node?.record).toBe("connection");
  });

  it("scans every example network to the end without getting lost", () => {
    const dir = join(__dirname, "..", "..", "..", "test-data", "v1");
    for (const file of readdirSync(dir).filter((f) => f.endsWith(".boo"))) {
      const text = readFileSync(join(dir, file), "utf8");
      expect(scan(text).mode, file).toBe("outside");
      expect(scan(text).root?.children.length, file).toBeGreaterThan(0);
    }
  });
});

describe("suggestions", () => {
  it("offer the keys that apply and aren't written yet", () => {
    const keys = labels("(NRN=[(nid=a, mst=1 mV, |");
    expect(keys).toContain("rfp");
    expect(keys).toContain("LOC");
    expect(keys).not.toContain("mst");
    expect(keys).not.toContain("SNT"); // hidden old name
  });

  it("follow the record's kind", () => {
    expect(labels("(NRN=[(nid=a, WDF=(fnc=exp, |")).toEqual(["dhl"]);
    expect(labels("(NRN=[(nid=a, WDF=(fnc=zer, |")).toEqual([]);
    expect(labels("(NRN=[(nid=a, nty=bi, |")).toContain("lct");
    expect(labels("(NRN=[(nid=a, nty=bi, |")).not.toContain("mst");
    expect(labels("(LRN=[(fnc=stdp_alpha, |")).toEqual(expect.arrayContaining(["bln", "atc", "alr"]));
  });

  it("offer choices, booleans and names defined in the document", () => {
    expect(labels("(NRN=[(nid=a, nty=|")).toEqual(["mi", "bi", "mr", "br"]);
    expect(labels("(NRN=[(nid=a, inh=|")).toEqual(["t", "f"]);
    const doc = "(NRN=[(nid=in-1), (nid=out-1)], LRN=[(fnc=stdp_hard, nam=fast)], CON=[(prn=|";
    expect(labels(doc)).toEqual(["in-1", "out-1"]);
    const lrn = "(NRN=[(nid=a)], LRN=[(fnc=stdp_hard, nam=fast)], CON=[(prn=a, psn=a, lrn=|";
    expect(labels(lrn)).toEqual(["fast", "flat"]);
  });

  it("offer a new record inside a list, with its required keys", () => {
    const { text, offset } = at("(NRN=[|");
    const [s] = suggestions(text, offset);
    expect(s?.insert).toMatch(/^\(nid=.*LOC=\(px1=.*\)/);
  });
});

describe("hover", () => {
  it("explains a key", () => {
    const text = "(NRN=[(nid=a, mpd=250 ms)])";
    const h = hover(text, text.indexOf("mpd") + 1);
    expect(h?.markdown).toContain("decay time constant");
    expect(h?.markdown).toContain("default 250 ms");
  });

  it("knows which record a key belongs to", () => {
    // `upb` means different things in a weight limit and in homeostasis.
    const text = "(NRN=[(nid=a, WLF=(fnc=bnd, lwb=0, upb=1), HIP=(tgt=5 Hz, str=1, upb=2 mV))])";
    expect(hover(text, text.indexOf("upb") + 1)?.markdown).toContain("Highest weight");
    expect(hover(text, text.lastIndexOf("upb") + 1)?.markdown).toContain("Highest threshold shift");
  });

  it("ignores values and unknown words", () => {
    const text = "(NRN=[(nid=alpha)])";
    expect(hover(text, text.indexOf("alpha") + 1)).toBeNull();
  });
});

describe("applicable keys", () => {
  it("all of a record's keys before its kind is written", () => {
    const ctx = scan("(NRN=[(nid=a, ", 15);
    expect(applicableKeys(ctx.node!).map((k) => k.key)).toEqual(expect.arrayContaining(["mst", "lct"]));
  });
});
