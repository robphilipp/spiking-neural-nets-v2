import { Copy, FilePlus, FolderInput, Pencil, Save, Trash2 } from "lucide-react";
import { useRef, useState } from "react";
import { Choice, Field, Heading, Panel, Row } from "@/components/kit";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { inputLabel } from "../../inputEdit";
import type { InputFile } from "../../inputFile";

const NO_FILE = "(none)";

/** The file's name, renamed when you leave the field or press Enter (so a half-typed name never becomes a file); a name
 *  that can't be used says why and stays as typed. */
function NameField({
  label,
  onRename,
  disabled,
  pending,
}: {
  label: string;
  onRename: (label: string) => void | Promise<void>;
  disabled: boolean;
  /** Set to a function that applies a name that was typed but not yet renamed (false: it can't be used). */
  pending: { current: (() => Promise<boolean>) | null };
}) {
  const [text, setText] = useState(label);
  const [last, setLast] = useState(label);
  const [error, setError] = useState<string | null>(null);
  if (label !== last) {
    setLast(label);
    setText(label);
    setError(null);
  }
  const commit = async (): Promise<boolean> => {
    if (text === label) {
      setError(null);
      return true;
    }
    try {
      await onRename(text);
      return true;
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      return false;
    }
  };
  pending.current = commit;
  return (
    <>
    <Field
      label="File name"
      width={200}
      value={text}
      error={error !== null}
      help={error ?? undefined}
      disabled={disabled}
      spellCheck={false}
      onChange={(e) => setText(e.target.value)}
      onKeyDown={(e) => e.key === "Enter" && void commit()}
      data-testid="file-name"
    />
    <Button variant="outline" onClick={() => void commit()} disabled={disabled || text === label} data-testid="file-rename" title="Rename the file (on disk too, and in the project)">
      <Pencil data-icon="inline-start" />
      Rename
    </Button>
    </>
  );
}

/**
 * The project's input files: which one runs use (and this page edits), new, duplicate, rename and delete, and the
 * file's description. Deleting asks first. `locked`: a run is open, so nothing changes.
 */
export function FilesPanel({
  names,
  selected,
  file,
  onSelect,
  status,
  onSave,
  onAddToProject,
  onNew,
  onDuplicate,
  onRename,
  onDelete,
  onDescription,
  locked,
}: {
  names: string[];
  selected: string | null;
  file: InputFile | null;
  onSelect: (name: string | null) => void;
  onNew: () => void;
  onDuplicate: () => void;
  /** Throws an InputFileError for a name that can't be used. */
  onRename: (label: string) => void | Promise<void>;
  onDelete: () => void;
  /** Where the file stands: in the project or not, and whether its changes are saved. */
  status: { inProject: boolean; unsaved: boolean; path: string | null } | null;
  onSave: () => void;
  /** Null: not available (no project is open, or the file is in it). */
  onAddToProject: (() => void) | null;
  /** What Save does, for the button's title. */
  onDescription: (description: string) => void;
  locked: boolean;
}) {
  const [confirming, setConfirming] = useState(false);
  const pending = useRef<(() => Promise<boolean>) | null>(null);
  /** Applies a typed name first (a Save or Add right after typing a name acts on the new name), then `then`. */
  const afterRename = (then: () => void) => () => {
    void (async () => {
      if ((await pending.current?.()) === false) return;
      then();
    })();
  };
  return (
    <Panel data-testid="files-panel">
      <Heading>Input files</Heading>
      <Row>
        {/* Which file runs use: also the one edited below, and the Run page's. Switching files is allowed during a run
            (it takes effect at the next Build), editing them isn't. */}
        <Choice
          label="Input file (used by runs)"
          width={230}
          value={selected ?? NO_FILE}
          onChange={(v) => {
            setConfirming(false);
            onSelect(v === NO_FILE ? null : v);
          }}
          options={[{ value: NO_FILE, label: "none" }, ...names.map((n) => ({ value: n, label: inputLabel(n) }))]}
          disabled={locked}
          testId="input-file"
        />
        {selected !== null && (
          <>
            <NameField label={inputLabel(selected)} onRename={onRename} disabled={locked} pending={pending} />
          </>
        )}
        <Button variant="outline" onClick={onNew} disabled={locked} data-testid="file-new">
          <FilePlus data-icon="inline-start" />
          New
        </Button>
        {selected !== null && (
          <>
            <Button variant="outline" onClick={afterRename(onSave)} data-testid="file-save" title={status?.path ? `Writes the file to ${status.path}` : "Asks where to save the file"}>
              <Save data-icon="inline-start" />
              Save
            </Button>
            <Button
              variant="outline"
              onClick={onAddToProject ? afterRename(onAddToProject) : undefined}
              disabled={onAddToProject === null || locked}
              data-testid="file-add-to-project"
              title={onAddToProject ? "Saves the file in the project folder and lists it in the project" : "Needs an open project, and a file that isn't in it yet"}
            >
              <FolderInput data-icon="inline-start" />
              Add to project
            </Button>
            <Button variant="outline" onClick={onDuplicate} disabled={locked} data-testid="file-duplicate">
              <Copy data-icon="inline-start" />
              Duplicate
            </Button>
            {confirming ? (
              <>
                <Button variant="destructive" onClick={() => { setConfirming(false); onDelete(); }} data-testid="file-delete-confirm">
                  Delete {inputLabel(selected)}?
                </Button>
                <Button variant="ghost" onClick={() => setConfirming(false)}>
                  Cancel
                </Button>
              </>
            ) : (
              <Button variant="outline" onClick={() => setConfirming(true)} disabled={locked} data-testid="file-delete">
                <Trash2 data-icon="inline-start" />
                Delete
              </Button>
            )}
          </>
        )}
      </Row>
      {status && (
        <div className="flex flex-wrap items-center gap-2 text-xs text-muted-foreground" data-testid="file-status">
          <Badge variant="outline" className="font-normal" data-testid="file-membership">
            {status.inProject ? "in the project" : "not in the project"}
          </Badge>
          {status.unsaved ? (
            <Badge className="border-0 bg-warning font-normal text-black" data-testid="file-unsaved">
              unsaved changes
            </Badge>
          ) : (
            <span data-testid="file-saved">saved</span>
          )}
          {status.path && <span className="break-all">{status.path}</span>}
        </div>
      )}
      {file && (
        <Field
          label="Description"
          value={file.description}
          onChange={(e) => onDescription(e.target.value)}
          disabled={locked}
          placeholder="What this protocol does"
          data-testid="file-description"
        />
      )}
    </Panel>
  );
}
