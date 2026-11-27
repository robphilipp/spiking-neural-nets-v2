import { Download, FilePlus, FolderOpen, FolderPlus, Save, Upload, X } from "lucide-react";
import { useRef, useState } from "react";
import { Field, Heading, Notice, Panel } from "@/components/kit";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { toast } from "sonner";
import { OpenProjectDialog } from "../components/OpenProjectDialog";
import { useConnection } from "../connection";
import { inFolderPath, pickFolder, pickInputFile, readProject } from "../files";
import { applyAndBuild, NetworkErrors } from "../launch";
import { saveProjectTo, useUnsaved } from "../saveProject";
import { emptyInputFile, parseInputFile, serializeInputFile } from "../inputFile";
import { serializeProject } from "../project";
import {
  forget,
  forgetRecent,
  inputFileChanged,
  inputFileSaved,
  inputsSelected,
  newProjectStarted,
  projectOpened,
  projectRenamed,
  setDescription,
  useProject,
  useSession,
  writableProject,
} from "../store";
import { inTauri } from "../tauriClient";

function download(name: string, content: string | Uint8Array) {
  const url = URL.createObjectURL(new Blob([content as BlobPart], { type: typeof content === "string" ? "text/plain" : "application/octet-stream" }));
  const a = Object.assign(document.createElement("a"), { href: url, download: name });
  a.click();
  URL.revokeObjectURL(url);
}

/** Projects: a folder with project.toml and the network (desktop), or files in and out (browser). Once one is opened,
 *  it asks whether to run it (`onRun`, once it's built) or edit its network (`onEdit`). */
export function ProjectsPage({ onRun, onEdit }: { onRun: () => void; onEdit: () => void }) {
  const { folder, project, inputFiles, dataFiles, savedDescription, recent } = useProject();
  const description = useSession((s) => s.description);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);
  const fileInput = useRef<HTMLInputElement>(null);
  const inputUpload = useRef<HTMLInputElement>(null);
  const desktop = inTauri();
  const unsaved = useUnsaved();
  const { client } = useConnection();
  const loose = useProject((s) => s.loose);
  // The project was just opened: run it or edit it?
  const [asking, setAsking] = useState(false);
  const [launching, setLaunching] = useState(false);
  const [launchError, setLaunchError] = useState<string | null>(null);
  const ask = () => {
    setLaunchError(null);
    setAsking(true);
  };
  const run = async (inputs: string | null) => {
    if (!client) return setLaunchError("Not connected to the engine: check the connection (Settings), then try again.");
    if (inputs !== null) inputsSelected(inputs);
    setLaunchError(null);
    setLaunching(true);
    try {
      await applyAndBuild(client);
      setAsking(false);
      onRun();
    } catch (e) {
      const text = e instanceof Error ? e.message : String(e);
      if (e instanceof NetworkErrors) {
        // The errors are marked in the editor.
        setAsking(false);
        toast.error("Not run", { description: text, duration: 10_000 });
        onEdit();
      } else if (useSession.getState().network !== null) {
        // Built, but the input file couldn't be given to it: the Run page says what's wrong with it.
        setAsking(false);
        toast.error("Built, but the inputs weren't given to the run", { description: text, duration: 10_000 });
        onRun();
      } else {
        setLaunchError(text);
      }
    } finally {
      setLaunching(false);
    }
  };

  const attempt = (f: () => Promise<void>) => async () => {
    setMessage(null);
    try {
      await f();
    } catch (e) {
      setMessage({ ok: false, text: e instanceof Error ? e.message : String(e) });
    }
  };

  /** A folder without project.toml and with several networks: which to open. */
  const [choose, setChoose] = useState<{ folder: string; networks: string[] } | null>(null);

  const open = async (dir: string, network?: string) => {
    setChoose(null);
    const read = await readProject(dir, network);
    if (read.kind === "choose") return setChoose({ folder: dir, networks: read.networks });
    const { project: p, description: text, inputFiles: files, dataFiles: data, looseFiles, skipped, saved } = read;
    forget(); // a new engine session is created when the network is applied
    projectOpened({ folder: dir, project: p, description: text, inputFiles: files, dataFiles: data, looseFiles, saved });
    for (const name of Object.keys(looseFiles)) inputFileSaved(name, await inFolderPath(dir, name));
    setDescription({ name: p.name, description: text });
    const unlisted = Object.keys(looseFiles);
    if (unlisted.length > 0 || skipped.length > 0) {
      setMessage({
        ok: skipped.length === 0,
        text:
          (unlisted.length > 0 ? `${unlisted.join(", ")} ${unlisted.length === 1 ? "is" : "are"} in the folder but not in the project: ${unlisted.length === 1 ? "it's" : "they're"} on the Inputs page, where Add to project lists ${unlisted.length === 1 ? "it" : "them"}. ` : "") +
          (skipped.length > 0 ? `Couldn't read: ${skipped.join("; ")}` : ""),
      });
    }
    if (!text) {
      // No network to run: straight to the editor (Save writes the files).
      toast.info(`${dir} has no project.toml or .boo file`, { description: "A new project there, with an empty network: Save writes them." });
      onEdit();
      return;
    }
    ask();
  };

  const save = async (dir: string) => {
    await saveProjectTo(dir);
    setMessage({ ok: true, text: `Saved to ${dir}` });
  };

  return (
    <div className="flex max-w-3xl flex-col gap-3">
      <Panel>
        <div className="flex items-end gap-3">
          <Field
            className="flex-1"
            label="Project name"
            value={project.name}
            onChange={(e) => {
              projectRenamed(e.target.value);
              setDescription({ name: e.target.value, description });
            }}
          />
          {unsaved && (
            <Badge className="mb-1.5 border-0 bg-warning text-black" data-testid="unsaved">
              unsaved changes
            </Badge>
          )}
        </div>
        <p className="text-sm text-muted-foreground">
          {folder ?? (desktop ? "Not saved to a folder yet." : "In this browser only; download the files to keep them.")}
        </p>
        {desktop ? (
          <div className="flex flex-wrap gap-2">
            <Button
              variant="outline"
              onClick={() => {
                forget();
                newProjectStarted();
                setDescription({ name: "untitled", description: "" });
                onEdit(); // a new project starts with writing its network
              }}
            >
              <FilePlus data-icon="inline-start" />
              New
            </Button>
            <Button
              variant="outline"
              onClick={attempt(async () => {
                const dir = await pickFolder("Open a project folder");
                if (dir) await open(dir);
              })}
            >
              <FolderOpen data-icon="inline-start" />
              Open project…
            </Button>
            <Button
              disabled={!unsaved && folder !== null}
              onClick={attempt(async () => {
                const dir = folder ?? (await pickFolder("Choose a folder for the project"));
                if (dir) await save(dir);
              })}
            >
              <Save data-icon="inline-start" />
              Save
            </Button>
            <Button
              variant="outline"
              onClick={attempt(async () => {
                const dir = await pickFolder("Save the project into a folder");
                if (dir) await save(dir);
              })}
            >
              <FolderPlus data-icon="inline-start" />
              Save as…
            </Button>
          </div>
        ) : (
          <div className="flex flex-wrap gap-2">
            <Button variant="outline" onClick={() => fileInput.current?.click()} data-testid="open-file">
              <Upload data-icon="inline-start" />
              Open a .boo file…
            </Button>
            <input
              ref={fileInput}
              type="file"
              accept=".boo,text/plain"
              hidden
              data-testid="file-input"
              onChange={(e) => {
                const file = e.target.files?.[0];
                if (!file) return;
                void file.text().then((text) => {
                  const name = file.name.replace(/\.boo$/, "");
                  forget();
                  projectRenamed(name);
                  setDescription({ name, description: text });
                  ask();
                });
                e.target.value = "";
              }}
            />
            <Button
              variant="outline"
              onClick={
                desktop
                  ? attempt(async () => {
                      const picked = await pickInputFile();
                      if (!picked) return;
                      const name = picked.path.split(/[\\/]/).pop()!;
                      inputFileChanged(name, parseInputFile(picked.text, name));
                      inputFileSaved(name, picked.path);
                      inputsSelected(name);
                      setMessage({ ok: true, text: `Opened ${picked.path}: runs use it (Run page › Input file).` });
                    })
                  : () => inputUpload.current?.click()
              }
              data-testid="open-inputs"
            >
              <Upload data-icon="inline-start" />
              Open an input file…
            </Button>
            <input
              ref={inputUpload}
              type="file"
              accept=".toml,text/plain"
              hidden
              data-testid="inputs-file-input"
              onChange={(e) => {
                const file = e.target.files?.[0];
                if (!file) return;
                void file.text().then((text) => {
                  try {
                    inputFileChanged(file.name, parseInputFile(text, file.name));
                    inputsSelected(file.name);
                    setMessage({ ok: true, text: `Opened ${file.name}: runs use it (Run page › Input file).` });
                  } catch (err) {
                    setMessage({ ok: false, text: err instanceof Error ? err.message : String(err) });
                  }
                });
                e.target.value = "";
              }}
            />
            <Button variant="outline" onClick={() => download(project.network, description)} data-testid="download-network">
              <Download data-icon="inline-start" />
              Download {project.network}
            </Button>
            <Button variant="outline" onClick={() => download("project.toml", serializeProject(writableProject(useProject.getState())))}>
              <Download data-icon="inline-start" />
              Download project.toml
            </Button>
            {project.inputs.map((name) => (
              <Button key={name} variant="outline" onClick={() => download(name, serializeInputFile(inputFiles[name] ?? emptyInputFile()))}>
                <Download data-icon="inline-start" />
                Download {name}
              </Button>
            ))}
            {Object.entries(dataFiles).map(([name, bytes]) => (
              <Button key={name} variant="outline" onClick={() => download(name.split("/").pop() ?? name, bytes)} data-testid={`download-data-${name}`}>
                <Download data-icon="inline-start" />
                Download {name}
              </Button>
            ))}
          </div>
        )}
        {choose && (
          <div className="flex flex-col gap-2 rounded-md border p-3" data-testid="choose-network">
            <p className="text-sm">
              {choose.folder} has no project.toml, and {choose.networks.length} networks. Which is the project's? (Save then writes its
              project.toml.)
            </p>
            <div className="flex flex-wrap gap-2">
              {choose.networks.map((n) => (
                <Button key={n} size="sm" variant="outline" onClick={attempt(() => open(choose.folder, n))}>
                  {n}
                </Button>
              ))}
              <Button size="sm" variant="ghost" onClick={() => setChoose(null)}>
                Cancel
              </Button>
            </div>
          </div>
        )}
        {message && (
          <Notice tone={message.ok ? "success" : "error"} onClose={() => setMessage(null)}>
            {message.text}
          </Notice>
        )}
      </Panel>
      {desktop && recent.length > 0 && (
        <Panel className="gap-1">
          <Heading>Recent projects</Heading>
          <ul className="flex flex-col">
            {recent.map((dir) => (
              <li key={dir} className="flex items-center gap-2 rounded-md hover:bg-muted">
                <button type="button" className="flex-1 px-2 py-1.5 text-left" onClick={attempt(() => open(dir))}>
                  <div className="text-sm">{dir.split(/[\\/]/).pop()}</div>
                  <div className="text-xs text-muted-foreground">{dir}</div>
                </button>
                <Button variant="ghost" size="icon-xs" title="Remove from the list" aria-label="Remove from the list" onClick={() => forgetRecent(dir)}>
                  <X />
                </Button>
              </li>
            ))}
          </ul>
        </Panel>
      )}
      <OpenProjectDialog
        open={asking}
        project={project.name}
        names={project.inputs}
        loose={loose}
        selected={project.selectedInputs}
        busy={launching}
        error={launchError}
        onRun={(inputs) => void run(inputs)}
        onEdit={() => {
          setAsking(false);
          onEdit();
        }}
        onClose={() => setAsking(false)}
      />
    </div>
  );
}
