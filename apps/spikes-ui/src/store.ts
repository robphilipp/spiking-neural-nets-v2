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
  /** The description as last saved (to show unsaved changes). */
  savedDescription: string | null;
  savedProject: string | null;
  /** The input files as last saved (JSON). */
  savedInputs: string | null;
  /** Recently opened folders, newest first. */
  recent: string[];
}

const RECENT_KEY = "spikes.recentProjects";

const freshInputs = (p: Project): Record<string, InputFile> => Object.fromEntries(p.inputs.map((f) => [f, emptyInputFile()]));

export const useProject = create<ProjectState>()(() => ({
  folder: null,
  project: newProject("untitled"),
  inputFiles: freshInputs(newProject("untitled")),
  savedDescription: null,
  savedProject: null,
  savedInputs: null,
  recent: recall<string[]>(RECENT_KEY, []),
}));
rememberIn(useProject, RECENT_KEY, (s) => s.recent);

const withRecent = (recent: string[], folder: string) => [folder, ...recent.filter((f) => f !== folder)].slice(0, 10);

/** `saved`: whether the folder has project.toml and the input files as read (without, the project shows as
 *  unsaved until it's saved). */
export function projectOpened({
  folder,
  project,
  description,
  inputFiles,
  saved,
}: {
  folder: string;
  project: Project;
  description: string;
  inputFiles: Record<string, InputFile>;
  saved: boolean;
}) {
  useProject.setState((s) => ({
    folder,
    project,
    inputFiles,
    savedDescription: description,
    savedProject: saved ? JSON.stringify(project) : null,
    savedInputs: saved ? JSON.stringify(inputFiles) : null,
    recent: withRecent(s.recent, folder),
  }));
}

export function projectSaved({ folder, description }: { folder: string; description: string }) {
  useProject.setState((s) => ({
    folder,
    savedDescription: description,
    savedProject: JSON.stringify(s.project),
    savedInputs: JSON.stringify(s.inputFiles),
    recent: withRecent(s.recent, folder),
  }));
}

/** An input file changed (or was added: it joins the project's list). */
export function inputFileChanged(name: string, file: InputFile) {
  useProject.setState((s) => ({
    project: s.project.inputs.includes(name) ? s.project : { ...s.project, inputs: [...s.project.inputs, name] },
    inputFiles: { ...s.inputFiles, [name]: file },
  }));
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
function filesChanged(set: FileSet) {
  useProject.setState((s) => ({ project: { ...s.project, inputs: set.names, selectedInputs: set.selected }, inputFiles: set.files }));
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
  filesChanged(set);
  return next;
}

export function inputFileDeleted(name: string) {
  filesChanged(deleteFile(fileSetOf(useProject.getState()), name));
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
  useProject.setState({ folder: null, project, inputFiles: freshInputs(project), savedDescription: null, savedProject: null, savedInputs: null });
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
export function built(network: Network, inputs?: string | null) {
  useSession.setState((s) => ({ network, state: "ready", simMs: 0, build: ++builds, builtInputs: inputs === undefined ? s.builtInputs : inputs }));
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
