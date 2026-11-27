import { beforeEach, describe, expect, it } from "vitest";
import { emptyInputFile } from "../src/inputFile";
import { newProject } from "../src/project";
import {
  inputFileAdded,
  inputFileChanged,
  inputFileListed,
  inputFileRenamed,
  inputFileSaved,
  newProjectStarted,
  projectOpened,
  projectUnsaved,
  useProject,
  writableProject,
} from "../src/store";

const open = () => {
  const project = { ...newProject("p"), inputs: ["a.inputs.toml"], selectedInputs: "a.inputs.toml" };
  projectOpened({ folder: "/proj", project, description: "net", inputFiles: { "a.inputs.toml": emptyInputFile() }, saved: true });
};

describe("input files outside the project", () => {
  beforeEach(() => newProjectStarted());

  it("a new file is part of a project that has no folder yet", () => {
    const name = inputFileAdded("x");
    expect(useProject.getState().loose).toEqual([]);
    expect(writableProject(useProject.getState()).inputs).toContain(name);
  });

  it("with a project open, a new file is outside it until added; the project doesn't count it", () => {
    open();
    expect(projectUnsaved(useProject.getState())).toBe(false);
    const name = inputFileAdded("x");
    const s = useProject.getState();
    expect(s.loose).toEqual([name]);
    expect(writableProject(s).inputs).toEqual(["a.inputs.toml"]);
    expect(writableProject(s).selectedInputs).toBeNull(); // the new file is selected, but project.toml can't say so
    expect(projectUnsaved(s)).toBe(true); // (the selection changed) ...
    inputFileChanged(name, { ...emptyInputFile(), description: "edited" });
    expect(useProject.getState().savedFile[name]).toBeUndefined();
  });

  it("adding to the project lists it last, and the project reads as saved once the caller has written it", () => {
    open();
    const name = inputFileAdded("x");
    useProject.setState((s) => ({ project: { ...s.project, selectedInputs: "a.inputs.toml" } }));
    expect(projectUnsaved(useProject.getState())).toBe(false);
    inputFileListed(name);
    const s = useProject.getState();
    expect(s.loose).toEqual([]);
    expect(s.project.inputs.at(-1)).toBe(name);
    expect(JSON.parse(s.savedProject!).inputs).toEqual(["a.inputs.toml", name]);
    expect(projectUnsaved(s)).toBe(false);
  });

  it("renaming carries a file's loose state, path and saved state", () => {
    open();
    const name = inputFileAdded("x");
    inputFileSaved(name, "/elsewhere/x.inputs.toml");
    const next = inputFileRenamed(name, "y");
    const s = useProject.getState();
    expect(s.loose).toEqual([next]);
    expect(s.inputPaths[next]).toBe("/elsewhere/x.inputs.toml");
    expect(s.savedFile[next]).toBeDefined();
    expect(s.savedFile[name]).toBeUndefined();
  });

  it("renaming a listed file keeps it listed", () => {
    open();
    const next = inputFileRenamed("a.inputs.toml", "b");
    const s = useProject.getState();
    expect(s.loose).toEqual([]);
    expect(s.project.inputs).toEqual([next]);
  });
});
