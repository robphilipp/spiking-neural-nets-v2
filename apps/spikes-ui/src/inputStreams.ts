// Inputs streaming into a session (a designed signal, or the microphone). They live outside any page, so they keep
// going while you watch the Run page, until stopped (or the connection goes).

import { useSyncExternalStore } from "react";

export interface ActiveInput {
  kind: "live" | "microphone";
  session: number;
  stop: () => void;
}

let active: ActiveInput | null = null;
const listeners = new Set<() => void>();
const changed = () => listeners.forEach((l) => l());

export function startInput(input: ActiveInput) {
  active?.stop();
  active = input;
  changed();
}

export function stopInput() {
  active?.stop();
  active = null;
  changed();
}

/** The input streaming now, if any. */
export function useActiveInput(): ActiveInput | null {
  return useSyncExternalStore(
    (l) => {
      listeners.add(l);
      return () => listeners.delete(l);
    },
    () => active,
  );
}
