// Saving, adding to the project and renaming one input file (the Inputs page's buttons). On the desktop these work on
// files on disk; a browser can't, so Save downloads the file and Rename only changes the name.

import { inFolderPath, pathExists, pickSavePath, renameFileOnDisk, writeProjectFile, writeTextAt } from "./files";
import { renameFile } from "./inputEdit";
import { emptyInputFile, serializeInputFile } from "./inputFile";
import type { Project } from "./project";
import { inputFileListed, inputFileRenamed, inputFileSaved, savedProjectRenamed, useProject } from "./store";
import { inTauri } from "./tauriClient";

function download(name: string, text: string) {
  const url = URL.createObjectURL(new Blob([text], { type: "text/plain" }));
  Object.assign(document.createElement("a"), { href: url, download: name }).click();
  URL.revokeObjectURL(url);
}

/** Where a file is on disk: in the project folder if it's in the project, else where it was opened or saved. */
export async function pathOf(name: string): Promise<string | null> {
  const s = useProject.getState();
  if (s.folder !== null && !s.loose.includes(name)) return inFolderPath(s.folder, name);
  return s.inputPaths[name] ?? null;
}

/** Whether "Add to project" applies: a project folder is open and the file isn't in the project. */
export const canAddToProject = (name: string | null): boolean => {
  const s = useProject.getState();
  return inTauri() && name !== null && s.folder !== null && s.loose.includes(name);
};

/** Writes the file where it is, or asks where (and remembers). Returns where it went; null if cancelled. */
export async function saveInputFile(name: string): Promise<string | null> {
  const s = useProject.getState();
  const text = serializeInputFile(s.inputFiles[name] ?? emptyInputFile());
  if (!inTauri()) {
    download(name, text);
    inputFileSaved(name, null);
    return name;
  }
  let path = await pathOf(name);
  if (path === null) {
    path = await pickSavePath(s.folder !== null ? await inFolderPath(s.folder, name) : name);
    if (path === null) return null;
  }
  await writeTextAt(path, text);
  inputFileSaved(name, path);
  return path;
}

/** Saves the file into the project folder and lists it in the project (and in project.toml, if the project has one). */
export async function addInputFileToProject(name: string): Promise<string> {
  const s = useProject.getState();
  if (s.folder === null || !s.loose.includes(name)) throw new Error("open a project first, and pick a file that isn't in it");
  const target = await inFolderPath(s.folder, name);
  const here = s.inputPaths[name] === target;
  if (!here && (await pathExists(target))) throw new Error(`the project folder already has a file called ${name}: rename this one first`);
  await writeTextAt(target, serializeInputFile(s.inputFiles[name] ?? emptyInputFile()));
  if (s.savedProject !== null) {
    const saved = JSON.parse(s.savedProject) as Project;
    await writeProjectFile(s.folder, { ...saved, inputs: [...saved.inputs, name] });
  }
  inputFileListed(name);
  return target;
}

/** Renames the file: its name, on disk (where it is), and in the project (project.toml too, if it lists it). Throws an
 *  InputFileError for a name that can't be used, or an Error if the disk refuses. Returns the new file name. */
export async function renameInputFile(name: string, label: string): Promise<string> {
  const s = useProject.getState();
  const { name: next } = renameFile({ names: s.project.inputs, files: s.inputFiles, selected: s.project.selectedInputs }, name, label); // validates
  if (next === name) return name;
  let newPath: string | null = null;
  const old = inTauri() ? await pathOf(name) : null;
  const listedOnDisk = s.savedProject !== null && (JSON.parse(s.savedProject) as Project).inputs.includes(name);
  if (old !== null && (await pathExists(old))) {
    const base = old.split(/[\\/]/).pop()!;
    newPath = old.slice(0, old.length - base.length) + next;
    if (await pathExists(newPath)) throw new Error(`there's already a file at ${newPath}`);
    await renameFileOnDisk(old, newPath);
    if (listedOnDisk && s.folder !== null) {
      const saved = JSON.parse(s.savedProject!) as Project;
      const rename = (n: string) => (n === name ? next : n);
      await writeProjectFile(s.folder, { ...saved, inputs: saved.inputs.map(rename), selectedInputs: saved.selectedInputs === null ? null : rename(saved.selectedInputs) });
      savedProjectRenamed(name, next);
    }
  }
  inputFileRenamed(name, label);
  if (newPath !== null && (s.loose.includes(name) || s.folder === null)) inputFileSaved(next, newPath);
  return next;
}
