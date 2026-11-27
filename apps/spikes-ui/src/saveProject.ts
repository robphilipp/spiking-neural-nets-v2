// Saving the open project, shared by the Projects and Inputs pages.

import { pickFolder, writeProject } from "./files";
import { projectSaved, projectUnsaved, useProject, useSession, writableProject } from "./store";

/** Whether the project (its settings, network, or a listed input file) differs from what was saved or opened. */
export function useUnsaved(): boolean {
  const state = useProject();
  const description = useSession((s) => s.description);
  return description !== (state.savedDescription ?? "") || projectUnsaved(state);
}

/** Writes the project into `dir`; returns where. Input files outside the project aren't written. */
export async function saveProjectTo(dir: string): Promise<string> {
  const state = useProject.getState();
  const description = useSession.getState().description;
  await writeProject(dir, writableProject(state), description, state.inputFiles);
  projectSaved({ folder: dir, description });
  return dir;
}

/** Saves into the project's folder, asking for one if it has none yet; null if the choice was cancelled. */
export async function saveProject(): Promise<string | null> {
  const dir = useProject.getState().folder ?? (await pickFolder("Choose a folder for the project"));
  return dir ? saveProjectTo(dir) : null;
}
