import { Code2, PlayCircle } from "lucide-react";
import { useId, useState } from "react";
import { cn } from "cn";
import { Notice } from "@/components/kit";
import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogDescription, DialogFooter, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Label } from "@/components/ui/label";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { inputLabel } from "../inputEdit";

/**
 * What to do with a project just opened: run it (applied, built with the chosen input file, ready on the Run page) or
 * edit its network. The input files are listed only when there's a choice; files found in the folder but not in the
 * project are marked so.
 */
export function OpenProjectDialog({
  open,
  project,
  names,
  loose,
  selected,
  busy,
  error,
  onRun,
  onEdit,
  onClose,
}: {
  open: boolean;
  project: string;
  names: string[];
  loose: string[];
  selected: string | null;
  busy: boolean;
  error: string | null;
  onRun: (inputs: string | null) => void;
  onEdit: () => void;
  onClose: () => void;
}) {
  const [choice, setChoice] = useState<string | null>(selected ?? names[0] ?? null);
  const [shownFor, setShownFor] = useState<boolean>(false);
  // Each time the dialog opens, start from the project's selection.
  if (open !== shownFor) {
    setShownFor(open);
    if (open) setChoice(selected ?? names[0] ?? null);
  }
  const groupId = useId();
  return (
    <Dialog open={open} onOpenChange={(o) => !o && !busy && onClose()}>
      <DialogContent data-testid="open-project-dialog">
        <DialogHeader>
          <DialogTitle>Opened {project}</DialogTitle>
          <DialogDescription>Run it now, or edit its network first.</DialogDescription>
        </DialogHeader>
        {names.length > 1 && (
          <div className="flex flex-col gap-2">
            <Label id={groupId} className="text-xs font-normal text-muted-foreground">
              Input file for the run
            </Label>
            {/* A list that scrolls: a project can have many input files. */}
            <RadioGroup
              aria-labelledby={groupId}
              value={choice ?? ""}
              onValueChange={setChoice}
              disabled={busy}
              className="max-h-60 gap-0 overflow-y-auto rounded-md border p-1"
              data-testid="open-project-inputs"
            >
              {names.map((n) => (
                <Label
                  key={n}
                  className={cn(
                    "flex cursor-pointer items-center gap-2 rounded-sm px-2 py-1.5 font-normal hover:bg-muted/60",
                    choice === n && "bg-accent",
                  )}
                >
                  <RadioGroupItem value={n} data-testid={`open-project-input-${inputLabel(n)}`} />
                  <span className="min-w-0 truncate">{inputLabel(n)}</span>
                  {loose.includes(n) && <span className="shrink-0 text-xs text-muted-foreground">(not in the project)</span>}
                </Label>
              ))}
            </RadioGroup>
          </div>
        )}
        {names.length === 1 && <p className="text-sm text-muted-foreground">Input file: {inputLabel(names[0]!)}</p>}
        {names.length === 0 && <p className="text-sm text-muted-foreground">The project has no input file: the network runs without input.</p>}
        {error && (
          <Notice tone="error" data-testid="open-project-error">
            {error}
          </Notice>
        )}
        <DialogFooter>
          <Button variant="outline" onClick={onEdit} disabled={busy} data-testid="open-project-edit">
            <Code2 data-icon="inline-start" />
            Edit network
          </Button>
          <Button onClick={() => onRun(names.length === 0 ? null : choice)} disabled={busy} data-testid="open-project-run">
            <PlayCircle data-icon="inline-start" />
            {busy ? "Building…" : "Run"}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}
