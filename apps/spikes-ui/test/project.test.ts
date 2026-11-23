import { describe, expect, it } from "vitest";
import { newProject, parseProject, ProjectError, serializeProject } from "../src/project";

describe("project.toml", () => {
  it("round-trips", () => {
    const p = { ...newProject("ring"), run: { seed: 7, shards: 4, speed: null, durationS: 30 }, background: { rateHz: 5, mv: 12, prefix: "input-" } };
    expect(parseProject(serializeProject(p))).toEqual(p);
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
