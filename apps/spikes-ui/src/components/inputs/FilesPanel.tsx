import { Copy, FilePlus, Trash2 } from "lucide-react";
import { useState } from "react";
import { Choice, Field, Heading, Panel, Row } from "@/components/kit";
import { Button } from "@/components/ui/button";
import { inputLabel } from "../../inputEdit";
import { type InputFile, InputFileError } from "../../inputFile";

const NO_FILE = "(none)";

/** The file's name, renamed when you leave the field or press Enter (so a half-typed name never becomes a file); a name
 *  that can't be used says why and stays as typed. */
function NameField({ label, onRename, disabled }: { label: string; onRename: (label: string) => void; disabled: boolean }) {
  const [text, setText] = useState(label);
  const [last, setLast] = useState(label);
  const [error, setError] = useState<string | null>(null);
  if (label !== last) {
    setLast(label);
    setText(label);
    setError(null);
  }
  const commit = () => {
    if (text === label) return setError(null);
    try {
      onRename(text);
    } catch (e) {
      if (!(e instanceof InputFileError)) throw e;
      setError(e.message);
    }
  };
  return (
    <Field
      label="File name"
      width={200}
      value={text}
      error={error !== null}
      help={error ?? undefined}
      disabled={disabled}
      spellCheck={false}
      onChange={(e) => setText(e.target.value)}
      onBlur={commit}
      onKeyDown={(e) => e.key === "Enter" && commit()}
      data-testid="file-name"
    />
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
  onRename: (label: string) => void;
  onDelete: () => void;
  onDescription: (description: string) => void;
  locked: boolean;
}) {
  const [confirming, setConfirming] = useState(false);
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
            <NameField label={inputLabel(selected)} onRename={onRename} disabled={locked} />
          </>
        )}
        <Button variant="outline" onClick={onNew} disabled={locked} data-testid="file-new">
          <FilePlus data-icon="inline-start" />
          New
        </Button>
        {selected !== null && (
          <>
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
