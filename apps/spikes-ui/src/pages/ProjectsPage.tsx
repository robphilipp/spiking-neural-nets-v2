import CreateNewFolderIcon from "@mui/icons-material/CreateNewFolder";
import DeleteIcon from "@mui/icons-material/Close";
import DownloadIcon from "@mui/icons-material/Download";
import FolderOpenIcon from "@mui/icons-material/FolderOpen";
import NoteAddIcon from "@mui/icons-material/NoteAdd";
import SaveIcon from "@mui/icons-material/Save";
import UploadFileIcon from "@mui/icons-material/UploadFile";
import {
  Alert,
  Button,
  Chip,
  IconButton,
  List,
  ListItem,
  ListItemButton,
  ListItemText,
  Paper,
  Stack,
  TextField,
  Typography,
} from "@mui/material";
import { useRef, useState } from "react";
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
    <Stack spacing={2} sx={{ maxWidth: 760 }}>
      <Paper variant="outlined" sx={{ p: 2 }}>
        <Stack direction="row" spacing={1.5} sx={{ alignItems: "center", mb: 1.5 }}>
          <TextField
            label="Project name"
            value={project.name}
            sx={{ flex: 1 }}
            onChange={(e) => {
              dispatch(projectRenamed(e.target.value));
              dispatch(setDescription({ name: e.target.value, description }));
            }}
          />
          {unsaved && <Chip label="unsaved changes" color="warning" size="small" data-testid="unsaved" />}
        </Stack>
        <Typography variant="body2" color="text.secondary" sx={{ mb: 1.5 }}>
          {folder ?? (desktop ? "Not saved to a folder yet." : "In this browser only; download the files to keep them.")}
        </Typography>
        {desktop ? (
          <Stack direction="row" spacing={1} sx={{ flexWrap: "wrap" }} useFlexGap>
            <Button startIcon={<NoteAddIcon />} onClick={() => {
              dispatch(forget());
              dispatch(newProjectStarted());
              dispatch(setDescription({ name: "untitled", description: "" }));
            }}>
              New
            </Button>
            <Button startIcon={<FolderOpenIcon />} onClick={attempt(async () => {
              const dir = await pickFolder("Open a project folder");
              if (dir) await open(dir);
            })}>
              Open folder…
            </Button>
            <Button variant="contained" startIcon={<SaveIcon />} disabled={!unsaved && folder !== null} onClick={attempt(async () => {
              const dir = folder ?? (await pickFolder("Choose a folder for the project"));
              if (dir) await save(dir);
            })}>
              Save
            </Button>
            <Button startIcon={<CreateNewFolderIcon />} onClick={attempt(async () => {
              const dir = await pickFolder("Save the project into a folder");
              if (dir) await save(dir);
            })}>
              Save as…
            </Button>
          </Stack>
        ) : (
          <Stack direction="row" spacing={1} sx={{ flexWrap: "wrap" }} useFlexGap>
            <Button startIcon={<UploadFileIcon />} onClick={() => fileInput.current?.click()} data-testid="open-file">
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
            <Button startIcon={<DownloadIcon />} onClick={() => download(project.network, description)} data-testid="download-network">
              Download {project.network}
            </Button>
            <Button startIcon={<DownloadIcon />} onClick={() => download("project.toml", serializeProject(project))}>
              Download project.toml
            </Button>
          </Stack>
        )}
        {message && (
          <Alert severity={message.ok ? "success" : "error"} sx={{ mt: 1.5 }} onClose={() => setMessage(null)}>
            {message.text}
          </Alert>
        )}
      </Paper>
      {desktop && recent.length > 0 && (
        <Paper variant="outlined">
          <Typography variant="subtitle2" sx={{ px: 2, pt: 1.5 }}>
            Recent projects
          </Typography>
          <List dense>
            {recent.map((dir) => (
              <ListItem
                key={dir}
                disablePadding
                secondaryAction={
                  <IconButton edge="end" size="small" title="Remove from the list" onClick={() => dispatch(forgetRecent(dir))}>
                    <DeleteIcon fontSize="small" />
                  </IconButton>
                }
              >
                <ListItemButton onClick={attempt(() => open(dir))}>
                  <ListItemText primary={dir.split(/[\\/]/).pop()} secondary={dir} />
                </ListItemButton>
              </ListItem>
            ))}
          </List>
        </Paper>
      )}
    </Stack>
  );
}
