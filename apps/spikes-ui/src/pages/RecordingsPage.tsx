import type { Recording, SavedRun } from "@spikes/client";
import { Download, History, PlayCircle, RefreshCw, Table2 } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { Heading, Notice, Panel } from "@/components/kit";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { defaultServer, useConnection } from "../connection";
import { useAppDispatch, useAppSelector } from "../hooks";
import { opened } from "../store";
import { inTauri } from "../tauriClient";

const size = (bytes: number) =>
  bytes < 1024 ? `${bytes} B` : bytes < 1 << 20 ? `${(bytes / 1024).toFixed(1)} kB` : `${(bytes / (1 << 20)).toFixed(1)} MB`;

/** Partitioned tables (`spikes/part-00000.parquet`, …), one entry each. */
function datasets(files: Array<{ name: string; bytes: number }>) {
  const all = new Map<string, { name: string; parts: number; bytes: number }>();
  for (const f of files) {
    const slash = f.name.indexOf("/");
    if (slash < 0) continue;
    const name = f.name.slice(0, slash);
    const d = all.get(name) ?? { name, parts: 0, bytes: 0 };
    d.parts += 1;
    d.bytes += f.bytes;
    all.set(name, d);
  }
  return [...all.values()];
}

/** When a recording's id starts with a millisecond timestamp (the engine names them so), when it was made. */
function made(id: string): string {
  const ms = Number(id.split("-")[0]);
  return Number.isFinite(ms) && ms > 1e12 ? new Date(ms).toLocaleString() : "";
}

/** Recorded runs: replay one in the live views, export it to Parquet, or download its tables. Saved runs: continue
 *  one where it was saved. */
export function RecordingsPage({ onOpen }: { onOpen: () => void }) {
  const { client } = useConnection();
  const dispatch = useAppDispatch();
  const server = useAppSelector((s) => s.settings.server);
  const [recordings, setRecordings] = useState<Recording[]>([]);
  const [saves, setSaves] = useState<SavedRun[]>([]);
  const shards = useAppSelector((s) => s.project.project.run.shards);
  const nodes = useAppSelector((s) => s.settings.nodes)
    .split(",")
    .map((n) => n.trim())
    .filter(Boolean);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);

  const refresh = useCallback(() => {
    client?.listRecordings().then((r) => setRecordings([...r].reverse())).catch((e: Error) => setMessage({ ok: false, text: e.message }));
    client?.listSaves().then((s) => setSaves([...s].reverse())).catch((e: Error) => setMessage({ ok: false, text: e.message }));
  }, [client]);
  useEffect(refresh, [refresh]);

  const attempt = (f: () => Promise<void>) => async () => {
    setMessage(null);
    try {
      await f();
    } catch (e) {
      setMessage({ ok: false, text: e instanceof Error ? e.message : String(e) });
    }
  };
  // Downloads come straight from the server; the desktop app's engine has no server, so it shows the folder.
  const downloads = !inTauri() || Boolean(server);
  const base = (server || defaultServer()).replace(/\/$/, "");

  return (
    <div className="flex max-w-4xl flex-col gap-3">
      <div className="flex items-center gap-2">
        <h2 className="flex-1 text-base font-medium">Recorded runs</h2>
        <Button variant="ghost" size="icon" onClick={refresh} title="Refresh" aria-label="Refresh">
          <RefreshCw />
        </Button>
      </div>
      {message && (
        <Notice tone={message.ok ? "success" : "error"} onClose={() => setMessage(null)}>
          {message.text}
        </Notice>
      )}
      {saves.length > 0 && (
        <>
          <h3 className="text-sm font-medium">Saved runs</h3>
          {saves.map((s) => (
            <Panel key={s.id} data-testid="saved-run" className="flex-row items-center gap-3">
              <Heading className="flex-1">
                {s.name} <span className="font-normal text-muted-foreground">at {(s.atMs / 1000).toFixed(2)} s · {size(s.bytes)} · {made(s.id)}</span>
              </Heading>
              <Button
                data-testid="continue-run"
                title={`Open it as a new session on ${shards} shard${shards === 1 ? "" : "s"}${nodes.length ? ` of each of ${nodes.length} nodes` : ""} (the Run page's and Settings' choices), ready to continue`}
                onClick={attempt(async () => {
                  const opened_ = await client!.openSave(s.id, shards, nodes);
                  const [description, network] = await Promise.all([client!.getDescription(opened_.id), client!.getNetwork(opened_.id)]);
                  dispatch(opened({ id: opened_.id, name: opened_.name, description, state: opened_.state, simMs: opened_.simMs, network }));
                  onOpen();
                })}
              >
                <History data-icon="inline-start" />
                Continue
              </Button>
            </Panel>
          ))}
          <h3 className="text-sm font-medium">Recordings</h3>
        </>
      )}
      {!recordings.length && <p className="text-sm text-muted-foreground">No recordings yet. Turn on Record on the Run page to record a run.</p>}
      {recordings.map((r) => (
        <Panel key={r.id} data-testid="recording">
          <div className="flex items-center gap-3">
            <Heading className="flex-1">
              {r.id.split("-").slice(1, -1).join("-") || r.id} <span className="font-normal text-muted-foreground">{made(r.id)}</span>
            </Heading>
            <Button
              data-testid="replay"
              onClick={attempt(async () => {
                const s = await client!.openRecording(r.id);
                dispatch(opened({ id: s.id, name: s.name, description: await client!.getDescription(s.id), state: s.state, simMs: s.simMs, network: null, replayEndMs: s.replayEndMs }));
                onOpen();
              })}
            >
              <PlayCircle data-icon="inline-start" />
              Replay
            </Button>
            <Button
              variant="outline"
              onClick={attempt(async () => {
                await client!.exportParquet(r.id);
                setMessage({ ok: true, text: `Exported ${r.id} to Parquet.` });
                refresh();
              })}
            >
              <Table2 data-icon="inline-start" />
              Export to Parquet
            </Button>
          </div>
          <div className="flex flex-wrap gap-1.5">
            {datasets(r.files).map((d) => (
              <Badge key={d.name} variant="outline" title="A partitioned Parquet dataset: read the folder with DuckDB, polars or pyarrow" data-testid="dataset">
                <Table2 />
                {d.name}/ · {d.parts} parts · {size(d.bytes)}
              </Badge>
            ))}
            {r.files.filter((f) => !f.name.includes("/")).map((f) =>
              downloads ? (
                <Badge key={f.name} variant="outline" asChild>
                  <a href={`${base}/api/recordings/${encodeURIComponent(r.id)}/${encodeURIComponent(f.name)}`}>
                    <Download />
                    {f.name} · {size(f.bytes)}
                  </a>
                </Badge>
              ) : (
                <Badge key={f.name} variant="outline">
                  {f.name} · {size(f.bytes)}
                </Badge>
              ),
            )}
          </div>
          {!downloads && <p className="text-xs text-muted-foreground">{r.path}</p>}
        </Panel>
      ))}
    </div>
  );
}
