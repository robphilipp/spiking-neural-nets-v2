import { Download, FilePlus, FolderOpen, FolderPlus, Save, Upload, X } from "lucide-react";
import { useRef, useState } from "react";
import { Field, Heading, Notice, Panel } from "@/components/kit";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { pickFolder, readProject, writeProject } from "../files";
import { useAppDispatch, useAppSelector } from "../hooks";
import { serializeProject } from "../project";
import { forget, forgetRecent, newProjectStarted, projectOpened, projectRenamed, projectSaved, setDescription } from "../store";
import { inTauri } from "../tauriClient";

function download(name: string, text: string) {
  const url = URL.createObjectURL(new Blob([text], { type: "text/plain" }));
  const a = Object.assign(document.createElement("a"), { href: url, download: name });
  a.click();
  URL.revokeObjectURL(url);
}

/** Projects: a folder with project.toml and the network (desktop), or files in and out (browser). */
export function ProjectsPage({ onOpened }: { onOpened: () => void }) {
  const dispatch = useAppDispatch();
  const { folder, project, savedDescription, savedProject, recent } = useAppSelector((s) => s.project);
  const description = useAppSelector((s) => s.session.description);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);
  const fileInput = useRef<HTMLInputElement>(null);
  const desktop = inTauri();
  const unsaved = description !== (savedDescription ?? "") || JSON.stringify(project) !== savedProject;

  const attempt = (f: () => Promise<void>) => async () => {
    setMessage(null);
    try {
      await f();
    } catch (e) {
      setMessage({ ok: false, text: e instanceof Error ? e.message : String(e) });
    }
  };

  const open = async (dir: string) => {
    const { project: p, description: text } = await readProject(dir);
    dispatch(forget()); // a new engine session is created when the network is applied
    dispatch(projectOpened({ folder: dir, project: p, description: text }));
    dispatch(setDescription({ name: p.name, description: text }));
    onOpened();
  };

  const save = async (dir: string) => {
    await writeProject(dir, project, description);
    dispatch(projectSaved({ folder: dir, description }));
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
              dispatch(projectRenamed(e.target.value));
              dispatch(setDescription({ name: e.target.value, description }));
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
                dispatch(forget());
                dispatch(newProjectStarted());
                dispatch(setDescription({ name: "untitled", description: "" }));
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
              Open folder…
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
                  dispatch(forget());
                  dispatch(projectRenamed(name));
                  dispatch(setDescription({ name, description: text }));
                  onOpened();
                });
                e.target.value = "";
              }}
            />
            <Button variant="outline" onClick={() => download(project.network, description)} data-testid="download-network">
              <Download data-icon="inline-start" />
              Download {project.network}
            </Button>
            <Button variant="outline" onClick={() => download("project.toml", serializeProject(project))}>
              <Download data-icon="inline-start" />
              Download project.toml
            </Button>
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
                <Button variant="ghost" size="icon-xs" title="Remove from the list" aria-label="Remove from the list" onClick={() => dispatch(forgetRecent(dir))}>
                  <X />
                </Button>
              </li>
            ))}
          </ul>
        </Panel>
      )}
    </div>
  );
}
