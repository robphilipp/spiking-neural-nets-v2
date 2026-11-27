// Project folders on disk (desktop app only: a web page can't open folders).

import { join } from "@tauri-apps/api/path";
import { open } from "@tauri-apps/plugin-dialog";
import { exists, mkdir, readDir, readTextFile, writeTextFile } from "@tauri-apps/plugin-fs";
import { emptyInputFile, type InputFile, parseInputFile, serializeInputFile } from "./inputFile";
import { MIGRATED_INPUTS, newProject, parseProject, type Project, ProjectError, serializeProject } from "./project";

export const PROJECT_FILE = "project.toml";

/** Asks for a folder. */
export async function pickFolder(title: string): Promise<string | null> {
  const folder = await open({ directory: true, multiple: false, title });
  return typeof folder === "string" ? folder : null;
}

/** A project folder, read. */
export type ReadProject =
  | {
      kind: "project";
      project: Project;
      description: string;
      /** Its input files, by name. */
      inputFiles: Record<string, InputFile>;
      /** Whether the folder has project.toml (without it, the project is made from the folder and its network). */
      saved: boolean;
    }
  /** No project.toml, and several network descriptions: which is the project's? */
  | { kind: "choose"; networks: string[] };

/** The network descriptions (`.boo`) in a folder, by name. */
async function networksIn(folder: string): Promise<string[]> {
  const entries = await readDir(folder);
  return entries
    .filter((e) => e.isFile && e.name.toLowerCase().endsWith(".boo"))
    .map((e) => e.name)
    .sort((a, b) => a.localeCompare(b));
}

/**
 * Reads a project folder: its settings (project.toml) and network description. A folder without project.toml (a
 * folder of .boo files, say) becomes a project named after the folder, with its network: the folder's one .boo file,
 * or `network` (asked for when there are several); with none, an empty network (Save writes both files).
 */
export async function readProject(folder: string, network?: string): Promise<ReadProject> {
  const file = await join(folder, PROJECT_FILE);
  if (await exists(file)) {
    const { project, migrated } = parseProject(await readTextFile(file));
    const path = await join(folder, project.network);
    if (!(await exists(path))) throw new ProjectError(`${PROJECT_FILE} names the network ${project.network}, but it isn't in ${folder}`);
    const inputFiles: Record<string, InputFile> = {};
    for (const name of project.inputs) {
      if (name === MIGRATED_INPUTS && migrated) {
        inputFiles[name] = migrated;
        continue;
      }
      const inputPath = await join(folder, name);
      if (!(await exists(inputPath))) throw new ProjectError(`${PROJECT_FILE} lists the input file ${name}, but it isn't in ${folder}`);
      inputFiles[name] = parseInputFile(await readTextFile(inputPath), name);
    }
    // A project from before input files is unsaved until its background input is written out as one.
    return { kind: "project", project, description: await readTextFile(path), inputFiles, saved: migrated === null };
  }
  const project = newProject(folder.split(/[\\/]/).pop() || "untitled");
  const networks = await networksIn(folder);
  if (network === undefined) {
    if (networks.length > 1) return { kind: "choose", networks };
    network = networks[0];
  }
  const inputFiles = Object.fromEntries(project.inputs.map((f) => [f, emptyInputFile()]));
  if (network === undefined) return { kind: "project", project, description: "", inputFiles, saved: false };
  if (!networks.includes(network)) throw new ProjectError(`${network} isn't in ${folder}`);
  project.network = network;
  return { kind: "project", project, description: await readTextFile(await join(folder, network)), inputFiles, saved: false };
}

/** Writes a project's settings, network description and input files into its folder (creating the folder if
 *  needed). */
export async function writeProject(
  folder: string,
  project: Project,
  description: string,
  inputFiles: Record<string, InputFile> = {},
): Promise<void> {
  if (!(await exists(folder))) await mkdir(folder, { recursive: true });
  await writeTextFile(await join(folder, PROJECT_FILE), serializeProject(project));
  await writeTextFile(await join(folder, project.network), description);
  for (const name of project.inputs) {
    await writeTextFile(await join(folder, name), serializeInputFile(inputFiles[name] ?? emptyInputFile()));
  }
}
