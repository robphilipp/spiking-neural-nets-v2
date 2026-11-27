// Project folders on disk (desktop app only: a web page can't open folders).

import { join } from "@tauri-apps/api/path";
import { open, save } from "@tauri-apps/plugin-dialog";
import { exists, mkdir, readDir, readFile, readTextFile, rename, writeTextFile } from "@tauri-apps/plugin-fs";
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
      /** The data files its input files refer to, by path in the folder (those that are there). */
      dataFiles: Record<string, Uint8Array>;
      /** Input files in the folder that the project doesn't list (read, so they can be added to it). */
      looseFiles: Record<string, InputFile>;
      /** Unlisted input files that couldn't be read, with why. */
      skipped: string[];
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
    // Data files are read-only references; one that isn't there is left out (a source using it says so).
    const dataFiles: Record<string, Uint8Array> = {};
    for (const ref of Object.values(inputFiles).flatMap((f) => f.data ?? [])) {
      const dataPath = await join(folder, ref.file);
      if (!(ref.file in dataFiles) && (await exists(dataPath))) dataFiles[ref.file] = await readFile(dataPath);
    }
    const { looseFiles, skipped } = await unlistedInputs(folder, project.inputs);
    for (const ref of Object.values(looseFiles).flatMap((f) => f.data ?? [])) {
      const dataPath = await join(folder, ref.file);
      if (!(ref.file in dataFiles) && (await exists(dataPath))) dataFiles[ref.file] = await readFile(dataPath);
    }
    // A project from before input files is unsaved until its background input is written out as one.
    return { kind: "project", project, description: await readTextFile(path), inputFiles, dataFiles, looseFiles, skipped, saved: migrated === null };
  }
  const project = newProject(folder.split(/[\\/]/).pop() || "untitled");
  const networks = await networksIn(folder);
  if (network === undefined) {
    if (networks.length > 1) return { kind: "choose", networks };
    network = networks[0];
  }
  const inputFiles = Object.fromEntries(project.inputs.map((f) => [f, emptyInputFile()]));
  const { looseFiles, skipped } = await unlistedInputs(folder, project.inputs);
  if (network === undefined) return { kind: "project", project, description: "", inputFiles, dataFiles: {}, looseFiles, skipped, saved: false };
  if (!networks.includes(network)) throw new ProjectError(`${network} isn't in ${folder}`);
  project.network = network;
  return { kind: "project", project, description: await readTextFile(await join(folder, network)), inputFiles, dataFiles: {}, looseFiles, skipped, saved: false };
}

/** The input files (`*.inputs.toml`) in a folder that `listed` doesn't have: read, or (those that can't be) skipped. */
async function unlistedInputs(folder: string, listed: string[]): Promise<{ looseFiles: Record<string, InputFile>; skipped: string[] }> {
  const looseFiles: Record<string, InputFile> = {};
  const skipped: string[] = [];
  for (const e of await readDir(folder)) {
    if (!e.isFile || !e.name.endsWith(".inputs.toml") || listed.includes(e.name)) continue;
    try {
      looseFiles[e.name] = parseInputFile(await readTextFile(await join(folder, e.name)), e.name);
    } catch (err) {
      skipped.push(`${e.name}: ${err instanceof Error ? err.message : String(err)}`);
    }
  }
  return { looseFiles, skipped };
}

/** Asks where to save an input file (starting at `defaultPath`); null if cancelled. */
export async function pickSavePath(defaultPath: string): Promise<string | null> {
  return (await save({ title: "Save the input file", defaultPath, filters: [{ name: "Input file", extensions: ["toml"] }] })) ?? null;
}

/** Asks for an input file to open: its path and text. */
export async function pickInputFile(): Promise<{ path: string; text: string } | null> {
  const picked = await open({ directory: false, multiple: false, title: "Open an input file", filters: [{ name: "Input file", extensions: ["toml"] }] });
  return typeof picked === "string" ? { path: picked, text: await readTextFile(picked) } : null;
}

export async function writeTextAt(path: string, text: string): Promise<void> {
  await writeTextFile(path, text);
}

export const pathExists = (path: string): Promise<boolean> => exists(path);

export async function renameFileOnDisk(from: string, to: string): Promise<void> {
  await rename(from, to);
}

/** `name` inside `folder`. */
export const inFolderPath = (folder: string, name: string): Promise<string> => join(folder, name);

/** Writes project.toml alone (the network and input files are left as they are). */
export async function writeProjectFile(folder: string, project: Project): Promise<void> {
  await writeTextFile(await join(folder, PROJECT_FILE), serializeProject(project));
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

/** Asks for a data file inside the project folder; its path relative to the folder, and its bytes. Throws a
 *  ProjectError for a file outside it. */
export async function pickDataFile(folder: string): Promise<{ path: string; bytes: Uint8Array } | null> {
  const picked = await open({ directory: false, multiple: false, defaultPath: folder, title: "Open a data file (CSV or WAV) from the project folder", filters: [{ name: "Data", extensions: ["csv", "tsv", "txt", "wav"] }] });
  if (typeof picked !== "string") return null;
  const root = folder.replace(/[\\/]+$/, "");
  if (!picked.startsWith(root + "/") && !picked.startsWith(root + "\\")) {
    throw new ProjectError(`${picked} isn't inside the project folder ${root}: copy it there first (data files are references into the folder)`);
  }
  return { path: picked.slice(root.length + 1).replace(/\\/g, "/"), bytes: await readFile(picked) };
}
