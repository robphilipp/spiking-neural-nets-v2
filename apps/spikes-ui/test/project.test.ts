import { describe, expect, it } from "vitest";
import { newProject, parseProject, ProjectError, serializeProject } from "../src/project";

describe("project.toml", () => {
  it("round-trips", () => {
    const p = { ...newProject("ring"), run: { seed: 7, shards: 4, speed: null, durationS: 30 }, background: { rateHz: 5, mv: 12, neurons: { mode: "prefix" as const, pattern: "input-" } } };
    expect(parseProject(serializeProject(p))).toEqual(p);
    const regex = { ...p, background: { ...p.background, neurons: { mode: "regex" as const, pattern: "^AS[HK]" } } };
    expect(serializeProject(regex)).toContain('match = "regex"');
    expect(parseProject(serializeProject(regex))).toEqual(regex);
  });

  it("reads a background prefix without a match mode as a prefix", () => {
    const p = parseProject('[background]\nrate_hz = 5\nmv = 12\nneurons = "hidden-1"');
    expect(p.background?.neurons).toEqual({ mode: "prefix", pattern: "hidden-1" });
    expect(() => parseProject('[background]\nrate_hz = 5\nmv = 12\nmatch = "glob"')).toThrow(ProjectError);
  });

  it("fills in defaults", () => {
    expect(parseProject('name = "x"')).toEqual(newProject("x"));
  });

  it("rejects bad values and files outside the project", () => {
    expect(() => parseProject("[run]\nshards = 0")).toThrow(ProjectError);
    expect(() => parseProject("[run]\nspeed = -1")).toThrow(ProjectError);
    expect(() => parseProject('network = "../../etc/passwd"')).toThrow(ProjectError);
    expect(() => parseProject("not toml = = =")).toThrow(ProjectError);
  });
});
