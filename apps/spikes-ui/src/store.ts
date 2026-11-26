// Application state, in zustand stores: settings (remembered in this browser), the project (saved with the network)
// and the session this window works with. Components read them with selectors (`useSession((s) => s.id)`); anything
// can change them through the actions exported here. (The live charts have stores of their own: see `charts/`.)

import type { Network, NetworkPreview, SourceError, State as SessionState } from "@spikes/client";
import { create } from "zustand";
import { DEFAULT_RUN, type Background, newProject, type Project, type RunSettings } from "./project";
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
  /** The description as last saved (to show unsaved changes). */
  savedDescription: string | null;
  savedProject: string | null;
  /** Recently opened folders, newest first. */
  recent: string[];
}

const RECENT_KEY = "spikes.recentProjects";

export const useProject = create<ProjectState>()(() => ({
  folder: null,
  project: newProject("untitled"),
  savedDescription: null,
  savedProject: null,
  recent: recall<string[]>(RECENT_KEY, []),
}));
rememberIn(useProject, RECENT_KEY, (s) => s.recent);

const withRecent = (recent: string[], folder: string) => [folder, ...recent.filter((f) => f !== folder)].slice(0, 10);

/** `saved`: whether the folder has project.toml (without it, the project shows as unsaved until it's saved). */
export function projectOpened({ folder, project, description, saved }: { folder: string; project: Project; description: string; saved: boolean }) {
  useProject.setState((s) => ({
    folder,
    project,
    savedDescription: description,
    savedProject: saved ? JSON.stringify(project) : null,
    recent: withRecent(s.recent, folder),
  }));
}

export function projectSaved({ folder, description }: { folder: string; description: string }) {
  useProject.setState((s) => ({
    folder,
    savedDescription: description,
    savedProject: JSON.stringify(s.project),
    recent: withRecent(s.recent, folder),
  }));
}

export function projectRenamed(name: string) {
  useProject.setState((s) => ({ project: { ...s.project, name } }));
}

export function runSettingsChanged(change: Partial<RunSettings>) {
  useProject.setState((s) => ({ project: { ...s.project, run: { ...s.project.run, ...change } } }));
}

export function backgroundChanged(background: Background | null) {
  useProject.setState((s) => ({ project: { ...s.project, background } }));
}

export function forgetRecent(folder: string) {
  useProject.setState((s) => ({ recent: s.recent.filter((f) => f !== folder) }));
}

export function newProjectStarted() {
  useProject.setState({ folder: null, project: newProject("untitled"), savedDescription: null, savedProject: null });
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
  build: 0,
};

export const useSession = create<CurrentSession>()(() => initialSession);

export function setDescription({ name, description }: { name?: string; description: string }) {
  useSession.setState((s) => ({ description, name: name || s.name }));
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

export function built(network: Network) {
  useSession.setState({ network, state: "ready", build: ++builds });
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
