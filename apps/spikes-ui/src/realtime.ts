// The realtime manager as the app uses it (app plan 05, phase I4): one manager for the window, its statuses in a
// store for the pages, and a hook (mounted once, in the App) that feeds it the run's state. Streams live here, not in
// a page, so they keep going while you look at other pages.

import { useEffect, useState } from "react";
import { create } from "zustand";
import { openCapture } from "./capture";
import { useConnection } from "./connection";
import { emptyInputFile } from "./inputFile";
import { RealtimeManager, type RealtimeStatus } from "./realtimeManager";
import { useProject, useSession } from "./store";

/** The statuses of this run's realtime sources (empty before the first run), in the file's order. */
export const useRealtime = create<{ statuses: RealtimeStatus[] }>(() => ({ statuses: [] }));

export const realtime = new RealtimeManager(openCapture, (statuses) => useRealtime.setState({ statuses }), { pollMs: 1000 });

/** How long the speed must hold still before sources follow it (dragging the slider passes through speeds). */
const SPEED_SETTLE_MS = 300;

/** Keeps the manager in step with the connection, the session, the run and the selected input file. Mount once. */
export function useRealtimeHost() {
  const { client } = useConnection();
  const { id, network, state, replayEndMs } = useSession();
  const file = useProject((s) => (s.project.selectedInputs === null ? null : (s.inputFiles[s.project.selectedInputs] ?? emptyInputFile())));
  const runSpeed = useProject((s) => s.project.run.speed);
  const run = state === "running" ? "running" : state === "paused" ? "paused" : "idle";
  // Before a run starts, its speed is the setting as it is; during one, a change must settle first.
  const [speed, setSpeed] = useState(runSpeed);
  if (run === "idle" && speed !== runSpeed) setSpeed(runSpeed);
  useEffect(() => {
    if (run === "idle") return;
    const t = setTimeout(() => setSpeed(runSpeed), SPEED_SETTLE_MS);
    return () => clearTimeout(t);
  }, [runSpeed, run]);
  useEffect(() => {
    realtime.sync({ client, session: id, network, file, run, replay: replayEndMs !== null, speed });
  }, [client, id, network, file, run, replayEndMs, speed]);
}
