import { Trash2, Upload } from "lucide-react";
import { useRef } from "react";
import { Heading, Panel, Row } from "@/components/kit";
import { Button } from "@/components/ui/button";
import { DataFileError, parseData } from "../../dataFile";
import { type LoadedData } from "../../inputData";
import { type InputFile } from "../../inputFile";
import { describeData } from "./FileFields";

/**
 * The input file's data files (`[[data]]`): CSV or WAV files of the project that formulas look up with
 * `data("name", t, c)` and `file` sources play. Each is a read-only reference to a file in the project folder:
 * "Open" adds one (the desktop app picks a file inside the project folder; the browser keeps the bytes in memory, and
 * the Projects page downloads them), "Remove" drops the reference, and saving never rewrites the files. `onOpen`
 * gets the file's path and bytes; it throws for what can't be used (its message is shown by the page).
 */
export function DataPanel({
  file,
  loaded,
  inTauri,
  locked,
  onOpen,
  onRemove,
  onError,
}: {
  file: InputFile;
  loaded: LoadedData;
  inTauri: boolean;
  locked: boolean;
  /** Desktop: asks for a file in the project folder. Browser: not used. */
  onOpen: (picked?: { path: string; bytes: Uint8Array }) => Promise<void>;
  onRemove: (name: string) => void;
  onError: (message: string) => void;
}) {
  const input = useRef<HTMLInputElement>(null);
  const refs = file.data ?? [];
  const fail = (e: unknown) => onError(e instanceof Error ? e.message : String(e));
  return (
    <Panel data-testid="data-panel">
      <Heading>Data files</Heading>
      <p className="text-xs text-muted-foreground">
        CSV (a time column, in ms, then a column per channel) or WAV files that formulas read with <code>data("name", t, c)</code> and
        file sources play. They are references to files in the project folder: saving doesn't rewrite them.
      </p>
      {refs.length === 0 && <p className="text-sm text-muted-foreground">No data files.</p>}
      <div className="flex flex-col gap-1.5">
        {refs.map((d) => {
          const set = loaded.sets.get(d.name);
          const error = loaded.errors.get(d.name);
          return (
            <div key={d.name} className="flex items-center gap-2 rounded-md border px-2.5 py-1.5" data-testid={`data-row-${d.name}`}>
              <div className="min-w-0 flex-1">
                <div className="text-sm font-medium">
                  {d.name} <span className="font-normal text-muted-foreground">· {d.file}</span>
                </div>
                <div className={error ? "text-xs text-destructive" : "text-xs text-muted-foreground"} data-testid={`data-info-${d.name}`}>
                  {error ?? (set ? describeData(set) : "")}
                </div>
              </div>
              <Button variant="ghost" size="icon-sm" aria-label={`Remove data file ${d.name}`} disabled={locked} onClick={() => onRemove(d.name)} data-testid={`data-remove-${d.name}`}>
                <Trash2 />
              </Button>
            </div>
          );
        })}
      </div>
      <Row>
        <Button
          variant="outline"
          disabled={locked}
          data-testid="data-open"
          onClick={() => (inTauri ? onOpen().catch(fail) : input.current?.click())}
        >
          <Upload data-icon="inline-start" />
          Open a data file…
        </Button>
        <input
          ref={input}
          type="file"
          accept=".csv,.tsv,.txt,.wav,text/csv,audio/wav"
          hidden
          data-testid="data-file-input"
          onChange={(e) => {
            const f = e.target.files?.[0];
            e.target.value = "";
            if (!f) return;
            void f
              .arrayBuffer()
              .then((b) => onOpen({ path: f.name, bytes: new Uint8Array(b) }))
              .catch(fail);
          }}
        />
      </Row>
    </Panel>
  );
}

/** Checks a data file's bytes read, before it is added: throws an Error saying what's wrong with it. */
export function checkData(path: string, bytes: Uint8Array): void {
  try {
    parseData(path, bytes);
  } catch (e) {
    if (e instanceof DataFileError) throw new Error(e.message);
    throw e;
  }
}
