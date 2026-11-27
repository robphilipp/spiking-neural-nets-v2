// Application state, in zustand stores: settings (remembered in this browser), the project (saved with the network)
// and the session this window works with. Components read them with selectors (`useSession((s) => s.id)`); anything
// can change them through the actions exported here. (The live charts have stores of their own: see `charts/`.)

import type { Network, NetworkPreview, SourceError, State as SessionState } from "@spikes/client";
import { create } from "zustand";
import { addFile, deleteFile, duplicateFile, type FileSet, renameFile } from "./inputEdit";
import { emptyInputFile, type InputFile } from "./inputFile";
import { DEFAULT_RUN, newProject, type Project, type RunSettings } from "./project";
import type { ThemeChoice } from "./theme";

export function remember(key: string, value: unknown) {
  try {
    localStorage.setItem(key, JSON.stringify(value));
  } catch {
    // Private windows and blocked storage: settings just aren't remembered.
  }
}

export function recall<T>(key: string, fallback: T): T {
  try {
    const saved = localStorage.getItem(key);
    return saved ? (JSON.parse(saved) as T) : fallback;
  } catch {
    return fallback;
  }
}

/** Saves a store's state (or the part `pick` takes) under `key` whenever it changes. */
export function rememberIn<S, P>(store: { subscribe: (f: (s: S, before: S) => void) => unknown }, key: string, pick: (s: S) => P) {
  store.subscribe((s, before) => {
    if (pick(s) !== pick(before)) remember(key, pick(s));
  });
}

// ----- settings (remembered in this browser) -----

export interface Settings {
  /** The spikes-server address, e.g. http://127.0.0.1:7070. Empty: the server this page came from. */
  server: string;
  token: string;
  theme: ThemeChoice;
  /** Node agents (`host:port`, comma-separated) to spread networks over; empty: the engine alone. */
  nodes: string;
  /** Show a tooltip for the neuron under the pointer in the 3D view. */
  tooltips: boolean;
}

const SETTINGS_KEY = "spikes.settings";
const DEFAULT_SETTINGS: Settings = { server: "", token: "", theme: "system", nodes: "", tooltips: true };

export const useSettings = create<Settings>()(() => ({ ...DEFAULT_SETTINGS, ...recall<Partial<Settings>>(SETTINGS_KEY, {}) }));
useSettings.subscribe((s) => remember(SETTINGS_KEY, s));

export function updateSettings(change: Partial<Settings>) {
  useSettings.setState(change);
}

// ----- the project: settings saved with the network, in a folder (desktop) -----

export interface ProjectState {
  /** The project folder, or null for an unsaved project. */
  folder: string | null;
  project: Project;
  /** The project's input files, by file name (each of `project.inputs`). */
  inputFiles: Record<string, InputFile>;
  /** The data files input files refer to (`[[data]]`), by path in the project folder: read from the folder (desktop) or
   *  opened in the browser. Read-only references: saving doesn't write them, and they don't count as changes. */
  dataFiles: Record<string, Uint8Array>;
  /** The description as last saved (to show unsaved changes). */
  savedDescription: string | null;
  savedProject: string | null;
  /** The project's (listed) input files as last saved (see `inputsKey`). */
  savedInputs: string | null;
  /** Input files that are in this window but not in the project (not listed in project.toml): new, opened or found
   *  unlisted in the folder while a project is open. "Add to project" lists them. */
  loose: string[];
  /** Where input files outside the project folder live on disk (desktop), by file name; those in the project are
   *  at the project folder + name. */
  inputPaths: Record<string, string>;
  /** Each input file as last saved or opened (JSON), by name: to show which have unsaved changes. */
  savedFile: Record<string, string>;
  /** Recently opened folders, newest first. */
  recent: string[];
}

const RECENT_KEY = "spikes.recentProjects";

const freshInputs = (p: Project): Record<string, InputFile> => Object.fromEntries(p.inputs.map((f) => [f, emptyInputFile()]));

export const useProject = create<ProjectState>()(() => ({
  folder: null,
  project: newProject("untitled"),
  inputFiles: freshInputs(newProject("untitled")),
  dataFiles: {},
  savedDescription: null,
  savedProject: null,
  savedInputs: null,
  loose: [],
  inputPaths: {},
  savedFile: {},
  recent: recall<string[]>(RECENT_KEY, []),
}));
rememberIn(useProject, RECENT_KEY, (s) => s.recent);

const withRecent = (recent: string[], folder: string) => [folder, ...recent.filter((f) => f !== folder)].slice(0, 10);

/** The project as project.toml has it: without the files that aren't in it (and without a selection of one). */
export const writableProject = (s: Pick<ProjectState, "project" | "loose">): Project => ({
  ...s.project,
  inputs: s.project.inputs.filter((n) => !s.loose.includes(n)),
  selectedInputs: s.project.selectedInputs !== null && s.loose.includes(s.project.selectedInputs) ? null : s.project.selectedInputs,
});

/** The listed input files, as one comparable string (by name, in name order). */
export const inputsKey = (files: Record<string, InputFile | string>): string =>
  JSON.stringify(Object.keys(files).sort().map((n) => [n, files[n]]));

const listedKey = (s: Pick<ProjectState, "project" | "loose" | "inputFiles">): string =>
  inputsKey(Object.fromEntries(s.project.inputs.filter((n) => !s.loose.includes(n)).map((n) => [n, JSON.stringify(s.inputFiles[n] ?? emptyInputFile())])));

/** Whether the project (as project.toml and its listed input files) differs from what was saved. */
export const projectUnsaved = (s: ProjectState): boolean => JSON.stringify(writableProject(s)) !== s.savedProject || listedKey(s) !== s.savedInputs;

const filesJson = (files: Record<string, InputFile>): Record<string, string> => Object.fromEntries(Object.entries(files).map(([n, f]) => [n, JSON.stringify(f)]));

/** `saved`: whether the folder has project.toml and the input files as read (without, the project shows as
 *  unsaved until it's saved). */
export function projectOpened({
  folder,
  project,
  description,
  inputFiles,
  dataFiles = {},
  looseFiles = {},
  saved,
}: {
  folder: string;
  project: Project;
  description: string;
  inputFiles: Record<string, InputFile>;
  dataFiles?: Record<string, Uint8Array>;
  /** Input files found in the folder that the project doesn't list. */
  looseFiles?: Record<string, InputFile>;
  saved: boolean;
}) {
  const loose = Object.keys(looseFiles);
  const all = { ...inputFiles, ...looseFiles };
  useProject.setState((s) => ({
    folder,
    project: { ...project, inputs: [...project.inputs, ...loose] },
    inputFiles: all,
    loose,
    inputPaths: {},
    savedFile: filesJson(all),
    dataFiles,
    savedDescription: description,
    savedProject: saved ? JSON.stringify(project) : null,
    savedInputs: saved ? inputsKey(filesJson(inputFiles)) : null,
    recent: withRecent(s.recent, folder),
  }));
}

export function projectSaved({ folder, description }: { folder: string; description: string }) {
  useProject.setState((s) => {
    const listed = s.project.inputs.filter((n) => !s.loose.includes(n));
    const saved = filesJson(Object.fromEntries(listed.map((n) => [n, s.inputFiles[n] ?? emptyInputFile()])));
    return {
      folder,
      savedDescription: description,
      savedProject: JSON.stringify(writableProject(s)),
      savedInputs: inputsKey(saved),
      savedFile: { ...s.savedFile, ...saved },
      recent: withRecent(s.recent, folder),
    };
  });
}

/** An input file changed, or was added (opened): with a project folder open it is not in the project until it's added
 *  to it; without, it is part of the project being made. */
export function inputFileChanged(name: string, file: InputFile) {
  useProject.setState((s) => {
    const known = s.project.inputs.includes(name);
    return {
      project: known ? s.project : { ...s.project, inputs: [...s.project.inputs, name] },
      loose: !known && s.folder !== null ? [...s.loose, name] : s.loose,
      inputFiles: { ...s.inputFiles, [name]: file },
    };
  });
}

/** A data file's bytes (at `path` in the project folder). */
export function dataFileLoaded(path: string, bytes: Uint8Array) {
  useProject.setState((s) => ({ dataFiles: { ...s.dataFiles, [path]: bytes } }));
}

/** Runs use this input file (null: none). */
export function inputsSelected(name: string | null) {
  useProject.setState((s) => ({ project: { ...s.project, selectedInputs: name } }));
}

/** The selected input file, if any. */
export function selectedInputs(s: ProjectState): { name: string; file: InputFile } | null {
  const name = s.project.selectedInputs;
  return name === null ? null : { name, file: s.inputFiles[name] ?? emptyInputFile() };
}

const fileSetOf = (s: ProjectState): FileSet => ({ names: s.project.inputs, files: s.inputFiles, selected: s.project.selectedInputs });

/** Applies a change to the project's input files (and which is selected) as one update. */
function filesChanged(set: FileSet, renamed?: [string, string]) {
  useProject.setState((s) => {
    const known = new Set(s.project.inputs);
    const [from, to] = renamed ?? [null, null];
    const carry = <T,>(rec: Record<string, T>): Record<string, T> => {
      const out: Record<string, T> = {};
      for (const [n, v] of Object.entries(rec)) out[n === from ? to! : n] = v;
      return Object.fromEntries(Object.entries(out).filter(([n]) => set.names.includes(n)));
    };
    // A file that wasn't there is new: outside the project while a project folder is open.
    const fresh = set.names.filter((n) => !known.has(n) && n !== to);
    const loose = [...s.loose.map((n) => (n === from ? to! : n)).filter((n) => set.names.includes(n)), ...(s.folder !== null ? fresh : [])];
    return {
      project: { ...s.project, inputs: set.names, selectedInputs: set.selected },
      inputFiles: set.files,
      loose,
      inputPaths: carry(s.inputPaths),
      savedFile: carry(s.savedFile),
    };
  });
}

/** A new empty input file, selected; its name. */
export function inputFileAdded(label?: string): string {
  const { set, name } = addFile(fileSetOf(useProject.getState()), label);
  filesChanged(set);
  return name;
}

/** A copy of an input file, selected; its name. */
export function inputFileDuplicated(name: string): string {
  const { set, name: copy } = duplicateFile(fileSetOf(useProject.getState()), name);
  filesChanged(set);
  return copy;
}

/** An input file renamed (throws an InputFileError for an empty or taken name); its new file name. */
export function inputFileRenamed(name: string, label: string): string {
  const { set, name: next } = renameFile(fileSetOf(useProject.getState()), name, label);
  filesChanged(set, next === name ? undefined : [name, next]);
  return next;
}

export function inputFileDeleted(name: string) {
  filesChanged(deleteFile(fileSetOf(useProject.getState()), name));
}

/** An input file was written to `path` (null: downloaded, no path): its changes are saved. */
export function inputFileSaved(name: string, path: string | null) {
  useProject.setState((s) => ({
    savedFile: { ...s.savedFile, [name]: JSON.stringify(s.inputFiles[name] ?? emptyInputFile()) },
    inputPaths: path === null ? s.inputPaths : { ...s.inputPaths, [name]: path },
  }));
}

/** A file outside the project joins it (and is written, in the project folder): it goes last in the list. The caller has
 *  written it, and project.toml if there is one (`savedProject`). */
export function inputFileListed(name: string) {
  useProject.setState((s) => {
    const inputs = [...s.project.inputs.filter((n) => n !== name), name];
    const inputPaths = { ...s.inputPaths };
    delete inputPaths[name];
    const file = JSON.stringify(s.inputFiles[name] ?? emptyInputFile());
    const saved = s.savedProject === null ? null : (JSON.parse(s.savedProject) as Project);
    return {
      project: { ...s.project, inputs },
      loose: s.loose.filter((n) => n !== name),
      inputPaths,
      savedFile: { ...s.savedFile, [name]: file },
      savedProject: saved === null ? null : JSON.stringify({ ...saved, inputs: [...saved.inputs, name] }),
      savedInputs: s.savedInputs === null ? null : inputsKey({ ...Object.fromEntries((JSON.parse(s.savedInputs) as [string, string][]).map(([n, f]) => [n, f])), [name]: file }),
    };
  });
}

/** The project's saved settings (as project.toml has them) with a renamed input file: after the file was renamed on
 *  disk and in project.toml (`from` → `to`). */
export function savedProjectRenamed(from: string, to: string) {
  useProject.setState((s) => {
    if (s.savedProject === null) return {};
    const saved = JSON.parse(s.savedProject) as Project;
    const rename = (n: string) => (n === from ? to : n);
    const next = { ...saved, inputs: saved.inputs.map(rename), selectedInputs: saved.selectedInputs === null ? null : rename(saved.selectedInputs) };
    const files = Object.fromEntries((JSON.parse(s.savedInputs ?? "[]") as [string, string][]).map(([n, f]) => [rename(n), f]));
    return { savedProject: JSON.stringify(next), savedInputs: s.savedInputs === null ? null : inputsKey(files) };
  });
}

export function projectRenamed(name: string) {
  useProject.setState((s) => ({ project: { ...s.project, name } }));
}

export function runSettingsChanged(change: Partial<RunSettings>) {
  useProject.setState((s) => ({ project: { ...s.project, run: { ...s.project.run, ...change } } }));
}

export function forgetRecent(folder: string) {
  useProject.setState((s) => ({ recent: s.recent.filter((f) => f !== folder) }));
}

export function newProjectStarted() {
  const project = newProject("untitled");
  useProject.setState({ folder: null, project, inputFiles: freshInputs(project), dataFiles: {}, savedDescription: null, savedProject: null, savedInputs: null, loose: [], inputPaths: {}, savedFile: {} });
}

export { DEFAULT_RUN };

// ----- the session this window works with -----

export interface CurrentSession {
  id: number | null;
  name: string;
  description: string;
  /** The description as last sent to the engine. */
  applied: string | null;
  errors: SourceError[];
  state: SessionState | null;
  simMs: number;
  network: Network | null;
  /** A replay: the recording's last event time; null for a live session. */
  replayEndMs: number | null;
  /** The input file (name and contents, as JSON) the network was built with; null for a session opened as it was. */
  builtInputs: string | null;
  /** The node agents the network was built on (none: this engine alone). */
  builtNodes: string[];
  /** Counts builds (and sessions opened): live views keep their data until this or the session changes (adding or
   *  pruning connections changes the network, but not this). */
  build: number;
}

let builds = 0;

const initialSession: CurrentSession = {
  id: null,
  name: "untitled",
  description: "",
  applied: null,
  errors: [],
  state: null,
  simMs: 0,
  network: null,
  replayEndMs: null,
  builtInputs: null,
  builtNodes: [],
  build: 0,
};

export const useSession = create<CurrentSession>()(() => initialSession);

export function setDescription({ name, description }: { name?: string; description: string }) {
  useSession.setState((s) => ({ description, name: name || s.name }));
}

/** A different network in the editor (an example chosen): it hasn't been applied, whatever the engine has. (Applying
 *  it replaces the session's network.) Edits go through `setDescription`, and count as modifying what's applied. */
export function networkLoaded({ name, description }: { name: string; description: string }) {
  useSession.setState({ description, name, applied: null, errors: [] });
  usePreview.setState(noPreview); // the 3D preview showed the network before
}

export function applied({ id, description }: { id: number; description: string }) {
  useSession.setState({ id, applied: description, errors: [], state: "loaded", network: null });
}

export function parseFailed(errors: SourceError[]) {
  useSession.setState({ errors });
}

export function stateChanged(state: SessionState) {
  useSession.setState({ state });
}

export function simTime(simMs: number) {
  useSession.setState({ simMs });
}

/** A network was built (or reset to as built): its run starts at time 0. `inputs`: the input file it was given (a
 *  reset keeps what the build had). */
export function built(network: Network, inputs?: string | null, nodes?: string[]) {
  useSession.setState((s) => ({
    network,
    state: "ready",
    simMs: 0,
    build: ++builds,
    builtInputs: inputs === undefined ? s.builtInputs : inputs,
    builtNodes: nodes === undefined ? s.builtNodes : nodes,
  }));
}

/** Connections were added or pruned (the run state doesn't change). */
export function networkUpdated(network: Network) {
  useSession.setState({ network });
}

/** Switched to a session that already exists on the engine. */
export function opened(s: {
  id: number;
  name: string;
  description: string;
  state: SessionState;
  simMs: number;
  network: Network | null;
  replayEndMs?: number | null;
}) {
  useSession.setState({ ...initialSession, ...s, replayEndMs: s.replayEndMs ?? null, applied: s.description, build: ++builds }, true);
  usePreview.setState(noPreview);
}

export function forget() {
  useSession.setState(initialSession, true);
  usePreview.setState(noPreview);
}

// ----- the Network page's 3D preview -----

/** The Network page's last preview and the text it was made from (kept while the page is away; dropped when another
 *  network takes the window: a project opened, a new or other session). */
const noPreview = { preview: null, text: null };
export const usePreview = create<{ preview: NetworkPreview | null; text: string | null }>(() => noPreview);
