import Editor, { type OnMount } from "@monaco-editor/react";
import { SpikesError } from "@spikes/client";
import { Check } from "lucide-react";
import { useEffect, useId, useRef, useState } from "react";
import { Notice } from "@/components/kit";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { useConnection } from "../connection";
import { examples } from "../examples";
import { LANGUAGE_ID } from "../language/register";
import { monaco } from "../monaco";
import { applied, parseFailed, setDescription, useSession } from "../store";
import { useThemeMode } from "../theme";

/** The network description editor: edit, then apply to the engine, which reports parse errors in place. */
export function NetworkPage() {
  const { client } = useConnection();
  const { id, name, description, applied: appliedText, errors, state } = useSession();
  const dark = useThemeMode() === "dark";
  const exampleId = useId();
  const editorRef = useRef<Parameters<OnMount>[0] | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  // Parse errors as editor markers.
  useEffect(() => {
    const model = editorRef.current?.getModel();
    if (!model) return;
    monaco.editor.setModelMarkers(
      model,
      "spikes",
      errors.map((e) => ({
        severity: monaco.MarkerSeverity.Error,
        message: e.message,
        startLineNumber: e.line,
        startColumn: e.column,
        endLineNumber: e.line,
        endColumn: e.column + 1,
      })),
    );
  }, [errors]);

  const apply = async () => {
    if (!client) return;
    setBusy(true);
    setMessage(null);
    try {
      if (id === null) {
        const session = await client.createSession(name, description);
        applied({ id: session.id, description });
      } else {
        await client.updateDescription(id, description);
        applied({ id, description });
      }
      setMessage("Applied. Build it on the Run page.");
    } catch (e) {
      if (e instanceof SpikesError && e.error.code === "parse") {
        parseFailed(e.error.spans ?? []);
        setMessage(`${e.error.spans?.length ?? 0} error(s) in the description`);
      } else {
        setMessage(e instanceof Error ? e.message : String(e));
      }
    } finally {
      setBusy(false);
    }
  };

  const running = state === "running" || state === "paused";
  return (
    <div className="flex h-[calc(100vh-5rem)] flex-col gap-3">
      <div className="flex items-end gap-3">
        <div className="grid w-72 gap-1">
          <Label htmlFor={exampleId} className="text-xs font-normal text-muted-foreground">
            Open an example
          </Label>
          <Select
            value=""
            onValueChange={(v) => {
              const example = examples.find((e) => e.name === v);
              if (example) setDescription({ name: example.name, description: example.text });
            }}
          >
            <SelectTrigger id={exampleId} className="w-full" aria-label="Open an example">
              <SelectValue placeholder="Choose…" />
            </SelectTrigger>
            <SelectContent>
              {examples.map((e) => (
                <SelectItem key={e.name} value={e.name}>
                  {e.name}
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
        <span className="flex-1 pb-1.5 text-sm text-muted-foreground">
          {name}
          {appliedText !== null && appliedText !== description ? " (changed)" : ""}
        </span>
        <Button disabled={!client || busy || running || !description.trim()} onClick={apply} data-testid="apply">
          <Check data-icon="inline-start" />
          Apply
        </Button>
      </div>
      {message && (
        <Notice tone={errors.length ? "error" : "success"} onClose={() => setMessage(null)}>
          {message}
        </Notice>
      )}
      {running && <Notice>Stop the run to change the network.</Notice>}
      <div className="min-h-[300px] flex-1 overflow-hidden rounded-lg border">
        <Editor
          language={LANGUAGE_ID}
          theme={dark ? "vs-dark" : "vs"}
          value={description}
          onChange={(text) => setDescription({ description: text ?? "" })}
          onMount={(editor) => {
            editorRef.current = editor;
          }}
          options={{ minimap: { enabled: false }, fontSize: 13, wordWrap: "on", scrollBeyondLastLine: false }}
        />
      </div>
    </div>
  );
}
