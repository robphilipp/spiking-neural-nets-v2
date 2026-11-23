import { configureStore, createSlice, type PayloadAction } from "@reduxjs/toolkit";
import type { Network, SourceError, State as SessionState } from "@spikes/client";
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
    /** Switched to a session that already exists on the engine. */
    opened(_, action: PayloadAction<{ id: number; name: string; description: string; state: SessionState; simMs: number; network: Network | null }>) {
      const s = action.payload;
      return { ...initialSession, ...s, applied: s.description };
    },
    forget() {
      return initialSession;
    },
  },
});

export const { updateSettings } = settings.actions;
export const { setDescription, applied, parseFailed, stateChanged, simTime, built, opened, forget } = session.actions;

export const store = configureStore({
  reducer: { settings: settings.reducer, session: session.reducer },
  middleware: (getDefault) => getDefault({ serializableCheck: false }),
});

store.subscribe(() => {
  try {
    localStorage.setItem(SETTINGS_KEY, JSON.stringify(store.getState().settings));
  } catch {
    // Private windows and blocked storage: settings just aren't remembered.
  }
});

export type RootState = ReturnType<typeof store.getState>;
export type AppDispatch = typeof store.dispatch;
