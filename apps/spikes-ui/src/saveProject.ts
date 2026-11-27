// Saving the open project, shared by the Projects and Inputs pages.

import { pickFolder, writeProject } from "./files";
import { projectSaved, useProject, useSession } from "./store";

/** Whether anything differs from what was last saved (or opened): the network, the project's settings or an input file. */
export function useUnsaved(): boolean {
  const { project, inputFiles, savedDescription, savedProject, savedInputs } = useProject();
  const description = useSession((s) => s.description);
  return description !== (savedDescription ?? "") || JSON.stringify(project) !== savedProject || JSON.stringify(inputFiles) !== savedInputs;
}

/** Writes the project into `dir`; returns where. */
export async function saveProjectTo(dir: string): Promise<string> {
  const { project, inputFiles } = useProject.getState();
  const description = useSession.getState().description;
  await writeProject(dir, project, description, inputFiles);
  projectSaved({ folder: dir, description });
  return dir;
}

/** Saves into the project's folder, asking for one if it has none yet; null if the choice was cancelled. */
export async function saveProject(): Promise<string | null> {
  const dir = useProject.getState().folder ?? (await pickFolder("Choose a folder for the project"));
  return dir ? saveProjectTo(dir) : null;
}
