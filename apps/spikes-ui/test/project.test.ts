import { existsSync, readdirSync, readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { parseInputFile } from "../src/inputFile";
import { MIGRATED_INPUTS, newProject, parseProject, ProjectError, serializeProject } from "../src/project";

describe("project.toml", () => {
  it("round-trips, with its input files and the selected one", () => {
    const p = {
      ...newProject("ring"),
      run: { seed: 7, shards: 4, speed: null, durationS: 30 },
      inputs: ["baseline.inputs.toml", "touch.inputs.toml"],
      selectedInputs: "touch.inputs.toml",
    };
    const text = serializeProject(p);
    expect(text).toContain('selected_inputs = "touch.inputs.toml"');
    expect(parseProject(text)).toEqual({ project: p, migrated: null });
  });

  it("fills in defaults", () => {
    expect(parseProject('name = "x"').project).toEqual({ ...newProject("x"), inputs: [], selectedInputs: null });
  });

  it("turns a [background] table into a selected input file with a Poisson source", () => {
    const { project, migrated } = parseProject('name = "old"\n[background]\nrate_hz = 5\nmv = 12\nneurons = "^AS"\nmatch = "regex"');
    expect(project.inputs).toEqual([MIGRATED_INPUTS]);
    expect(project.selectedInputs).toBe(MIGRATED_INPUTS);
    expect(migrated?.sources).toEqual([
      { name: "background", kind: "poisson", targets: { regex: "^AS" }, start: 0, duration: null, enabled: true, hz: 5, mv: 12 },
    ]);
    expect(parseProject('[background]\nrate_hz = 5\nmv = 12\nneurons = "in-"').migrated?.sources[0]?.targets).toEqual({ prefix: "in-" });
  });

  it("rejects bad values and files outside the project", () => {
    expect(() => parseProject("[run]\nshards = 0")).toThrow(ProjectError);
    expect(() => parseProject("[run]\nspeed = -1")).toThrow(ProjectError);
    expect(() => parseProject('network = "../../etc/passwd"')).toThrow(ProjectError);
    expect(() => parseProject('inputs = ["../x.inputs.toml"]')).toThrow(ProjectError);
    expect(() => parseProject('inputs = ["a.inputs.toml"]\nselected_inputs = "b.inputs.toml"')).toThrow(ProjectError);
    expect(() => parseProject('[background]\nrate_hz = 5\nmv = 12\nmatch = "glob"')).toThrow(ProjectError);
    expect(() => parseProject("not toml = = =")).toThrow(ProjectError);
  });
});

describe("the sample projects (test-data/projects)", () => {
  const root = new URL("../../../test-data/projects/", import.meta.url);
  for (const dir of readdirSync(root)) {
    it(`${dir} reads, with its input files`, () => {
      const folder = new URL(`${dir}/`, root);
      if (!existsSync(new URL("project.toml", folder))) return;
      const { project, migrated } = parseProject(readFileSync(new URL("project.toml", folder), "utf8"));
      expect(migrated).toBeNull();
      expect(existsSync(new URL(project.network, folder))).toBe(true);
      for (const name of project.inputs) parseInputFile(readFileSync(new URL(name, folder), "utf8"), name);
    });
  }
});
