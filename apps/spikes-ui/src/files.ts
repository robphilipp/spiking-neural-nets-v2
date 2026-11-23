// Project folders on disk (desktop app only: a web page can't open folders).

import { join } from "@tauri-apps/api/path";
import { open } from "@tauri-apps/plugin-dialog";
import { exists, mkdir, readTextFile, writeTextFile } from "@tauri-apps/plugin-fs";
import { newProject, parseProject, type Project, serializeProject } from "./project";

export const PROJECT_FILE = "project.toml";

/** Asks for a folder. */
export async function pickFolder(title: string): Promise<string | null> {
  const folder = await open({ directory: true, multiple: false, title });
  return typeof folder === "string" ? folder : null;
}

/** Reads a project folder: its settings and network description. A folder without project.toml becomes a new project. */
export async function readProject(folder: string): Promise<{ project: Project; description: string }> {
  const file = await join(folder, PROJECT_FILE);
  const project = (await exists(file)) ? parseProject(await readTextFile(file)) : newProject(folder.split(/[\\/]/).pop() || "untitled");
  const network = await join(folder, project.network);
  const description = (await exists(network)) ? await readTextFile(network) : "";
  return { project, description };
}

/** Writes a project's settings and network description into its folder (creating the folder if needed). */
export async function writeProject(folder: string, project: Project, description: string): Promise<void> {
  if (!(await exists(folder))) await mkdir(folder, { recursive: true });
  await writeTextFile(await join(folder, PROJECT_FILE), serializeProject(project));
  await writeTextFile(await join(folder, project.network), description);
}
