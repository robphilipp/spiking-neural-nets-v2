import { configureStore, createSlice, type PayloadAction } from "@reduxjs/toolkit";
import type { Network, SourceError, State as SessionState } from "@spikes/client";
import { DEFAULT_RUN, type Background, newProject, type Project, type RunSettings } from "./project";
import type { ThemeChoice } from "./theme";

// ----- settings (remembered in this browser) -----

export interface Settings {
  /** The spikes-server address, e.g. http://127.0.0.1:7070. Empty: the server this page came from. */
  server: string;
  token: string;
  theme: ThemeChoice;
}

const SETTINGS_KEY = "spikes.settings";

function loadSettings(): Settings {
  const defaults: Settings = { server: "", token: "", theme: "system" };
  try {
    const saved = localStorage.getItem(SETTINGS_KEY);
    return saved ? { ...defaults, ...(JSON.parse(saved) as Partial<Settings>) } : defaults;
  } catch {
    return defaults;
  }
}

const settings = createSlice({
  name: "settings",
  initialState: loadSettings,
  reducers: {
    updateSettings(state, action: PayloadAction<Partial<Settings>>) {
      Object.assign(state, action.payload);
    },
  },
});

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

function loadRecent(): string[] {
  try {
    return JSON.parse(localStorage.getItem(RECENT_KEY) ?? "[]") as string[];
  } catch {
    return [];
  }
}

const project = createSlice({
  name: "project",
  initialState: (): ProjectState => ({ folder: null, project: newProject("untitled"), savedDescription: null, savedProject: null, recent: loadRecent() }),
  reducers: {
    projectOpened(state, action: PayloadAction<{ folder: string; project: Project; description: string }>) {
      const { folder, project, description } = action.payload;
      state.folder = folder;
      state.project = project;
      state.savedDescription = description;
      state.savedProject = JSON.stringify(project);
      state.recent = [folder, ...state.recent.filter((f) => f !== folder)].slice(0, 10);
    },
    projectSaved(state, action: PayloadAction<{ folder: string; description: string }>) {
      state.folder = action.payload.folder;
      state.savedDescription = action.payload.description;
      state.savedProject = JSON.stringify(state.project);
      state.recent = [action.payload.folder, ...state.recent.filter((f) => f !== action.payload.folder)].slice(0, 10);
    },
    projectRenamed(state, action: PayloadAction<string>) {
      state.project.name = action.payload;
    },
    runSettingsChanged(state, action: PayloadAction<Partial<RunSettings>>) {
      Object.assign(state.project.run, action.payload);
    },
    backgroundChanged(state, action: PayloadAction<Background | null>) {
      state.project.background = action.payload;
    },
    forgetRecent(state, action: PayloadAction<string>) {
      state.recent = state.recent.filter((f) => f !== action.payload);
    },
    newProjectStarted(state) {
      state.folder = null;
      state.project = newProject("untitled");
      state.savedDescription = null;
      state.savedProject = null;
    },
  },
});

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
}

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
};

const session = createSlice({
  name: "session",
  initialState: initialSession,
  reducers: {
    setDescription(state, action: PayloadAction<{ name?: string; description: string }>) {
      state.description = action.payload.description;
      if (action.payload.name) state.name = action.payload.name;
    },
    applied(state, action: PayloadAction<{ id: number; description: string }>) {
      state.id = action.payload.id;
      state.applied = action.payload.description;
      state.errors = [];
      state.state = "loaded";
      state.network = null;
    },
    parseFailed(state, action: PayloadAction<SourceError[]>) {
      state.errors = action.payload;
    },
    stateChanged(state, action: PayloadAction<SessionState>) {
      state.state = action.payload;
    },
    simTime(state, action: PayloadAction<number>) {
      state.simMs = action.payload;
    },
    built(state, action: PayloadAction<Network>) {
      state.network = action.payload;
      state.state = "ready";
    },
    /** Connections were added or pruned (the run state doesn't change). */
    networkUpdated(state, action: PayloadAction<Network>) {
      state.network = action.payload;
    },
    /** Switched to a session that already exists on the engine. */
    opened(
      _,
      action: PayloadAction<{
        id: number;
        name: string;
        description: string;
        state: SessionState;
        simMs: number;
        network: Network | null;
        replayEndMs?: number | null;
      }>,
    ) {
      const s = action.payload;
      return { ...initialSession, ...s, replayEndMs: s.replayEndMs ?? null, applied: s.description };
    },
    forget() {
      return initialSession;
    },
  },
});

export const { updateSettings } = settings.actions;
export const { projectOpened, projectSaved, projectRenamed, runSettingsChanged, backgroundChanged, forgetRecent, newProjectStarted } =
  project.actions;
export { DEFAULT_RUN };
export const { setDescription, applied, parseFailed, stateChanged, simTime, built, networkUpdated, opened, forget } = session.actions;

export const store = configureStore({
  reducer: { settings: settings.reducer, session: session.reducer, project: project.reducer },
  middleware: (getDefault) => getDefault({ serializableCheck: false }),
});

store.subscribe(() => {
  try {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(store.getState().settings));
    localStorage.setItem(RECENT_KEY, JSON.stringify(store.getState().project.recent));
  } catch {
    // Private windows and blocked storage: settings just aren't remembered.
  }
});

export type RootState = ReturnType<typeof store.getState>;
export type AppDispatch = typeof store.dispatch;
