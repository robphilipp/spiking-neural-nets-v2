import Editor, { type OnMount } from "@monaco-editor/react";
import { SpikesError } from "@spikes/client";
import { Check } from "lucide-react";
import { useEffect, useId, useRef, useState } from "react";
import { toast } from "sonner";
import { Notice } from "@/components/kit";
import { NetworkViewPanel } from "@/components/NetworkViewPanel";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { useConnection } from "../connection";
import { examples } from "../examples";
import { LANGUAGE_ID } from "../language/register";
import { monaco } from "../monaco";
import { applied, networkLoaded, parseFailed, setDescription, useSession } from "../store";
import { useThemeMode } from "../theme";

/** The network description editor: edit, then apply to the engine, which reports parse errors in place. A badge
 *  says whether the network in the editor is applied (the engine has it as written), modified (edited since), or not
 *  applied (a network that hasn't been: new, or just opened); applying reports in a toast (dismissed on leaving the
 *  page). `onRun` goes to the Run page. */
export function NetworkPage({ onRun }: { onRun?: () => void }) {
  const { client } = useConnection();
  const { id, name, description, applied: appliedText, errors, state } = useSession();
  const dark = useThemeMode() === "dark";
  const exampleId = useId();
  const editorRef = useRef<Parameters<OnMount>[0] | null>(null);
  const [busy, setBusy] = useState(false);
  /** The last apply's toast: a new apply replaces it. */
  const lastToast = useRef<string | number | undefined>(undefined);

  // The apply toast is about this page: leaving it (to the Run page, say) dismisses it.
  useEffect(
    () => () => {
      if (lastToast.current !== undefined) toast.dismiss(lastToast.current);
    },
    [],
  );

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
    if (lastToast.current !== undefined) toast.dismiss(lastToast.current);
    try {
      if (id === null) {
        const session = await client.createSession(name, description);
        applied({ id: session.id, description });
      } else {
        await client.updateDescription(id, description);
        applied({ id, description });
      }
      lastToast.current = toast.success("Applied.", {
        description: "Next: build and start it on the Run page.",
        action: onRun ? { label: "Go to Run", onClick: onRun } : undefined,
      });
    } catch (e) {
      if (e instanceof SpikesError && e.error.code === "parse") {
        const n = e.error.spans?.length ?? 0;
        parseFailed(e.error.spans ?? []);
        lastToast.current = toast.error(`${n} error${n === 1 ? "" : "s"} in the description`, {
          description: "Not applied. The errors are underlined in the editor: hover them for details.",
          duration: 10_000,
        });
      } else {
        lastToast.current = toast.error("Not applied", { description: e instanceof Error ? e.message : String(e), duration: 10_000 });
      }
    } finally {
      setBusy(false);
    }
  };

  const running = state === "running" || state === "paused";
  // Applied: the engine has the text as it is. Modified: edited since it was applied. Not applied: a network that
  // hasn't been (a new one, an example or project just opened).
  const status = appliedText === null ? "not-applied" : appliedText === description ? "applied" : "modified";
  return (
    <div className="flex h-[calc(100vh-5rem)] flex-col gap-3">
      <div className="flex items-end gap-3">
        <div className="grid w-72 gap-1">
          <Label htmlFor={exampleId} className="text-xs font-normal text-muted-foreground">
            Open an example
          </Label>
          <Select
            // The example open now, if the network is one (its name); otherwise the placeholder.
            value={examples.some((e) => e.name === name) ? name : ""}
            onValueChange={(v) => {
              const example = examples.find((e) => e.name === v);
              if (example) networkLoaded({ name: example.name, description: example.text });
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
        <div className="flex flex-1 items-center gap-2 pb-1.5">
          <Badge
            variant="outline"
            data-testid="network-status"
            data-status={status}
            className={
              status === "applied"
                ? "border-success/40 bg-success/10 text-success"
                : status === "modified"
                  ? "border-warning/50 bg-warning/15 text-foreground"
                  : "text-muted-foreground"
            }
            title={
              status === "applied"
                ? "The engine has this network, as written"
                : status === "modified"
                  ? "Edited since it was applied: Apply to send the changes to the engine"
                  : "This network hasn't been applied: Apply to send it to the engine"
            }
          >
            {status === "applied" ? "Applied" : status === "modified" ? "Modified" : "Not applied"}
          </Badge>
        </div>
        <Button disabled={!client || busy || running || !description.trim()} onClick={apply} data-testid="apply">
          <Check data-icon="inline-start" />
          Apply
        </Button>
      </div>
      {running && <Notice>Stop the run to change the network.</Notice>}
      <div className="min-h-[200px] flex-1 overflow-hidden rounded-lg border">
        <Editor
          language={LANGUAGE_ID}
          theme={dark ? "vs-dark" : "vs"}
          value={description}
          onChange={(text) => setDescription({ description: text ?? "" })}
          onMount={(editor) => {
            editorRef.current = editor;
          }}
          options={{
            minimap: { enabled: false },
            fontSize: 13,
            wordWrap: "on",
            scrollBeyondLastLine: false,
            // Hover help and suggestions float above the page instead of inside the editor's (clipping) box, so
            // long help near the editor's top or bottom edge isn't cut off.
            fixedOverflowWidgets: true,
          }}
        />
      </div>
      <NetworkViewPanel />
    </div>
  );
}
