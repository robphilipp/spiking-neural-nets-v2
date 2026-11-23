import DownloadIcon from "@mui/icons-material/Download";
import PlayCircleIcon from "@mui/icons-material/PlayCircle";
import RefreshIcon from "@mui/icons-material/Refresh";
import TableChartIcon from "@mui/icons-material/TableChart";
import { Alert, Button, Chip, IconButton, Paper, Stack, Typography } from "@mui/material";
import type { Recording } from "@spikes/client";
import { useCallback, useEffect, useState } from "react";
import { defaultServer, useConnection } from "../connection";
import { useAppDispatch, useAppSelector } from "../hooks";
import { opened } from "../store";
import { inTauri } from "../tauriClient";

const size = (bytes: number) =>
  bytes < 1024 ? `${bytes} B` : bytes < 1 << 20 ? `${(bytes / 1024).toFixed(1)} kB` : `${(bytes / (1 << 20)).toFixed(1)} MB`;

/** When a recording's id starts with a millisecond timestamp (the engine names them so), when it was made. */
function made(id: string): string {
  const ms = Number(id.split("-")[0]);
  return Number.isFinite(ms) && ms > 1e12 ? new Date(ms).toLocaleString() : "";
}

/** Recorded runs: replay one in the live views, export it to Parquet, or download its tables. */
export function RecordingsPage({ onReplay }: { onReplay: () => void }) {
  const { client } = useConnection();
  const dispatch = useAppDispatch();
  const server = useAppSelector((s) => s.settings.server);
  const [recordings, setRecordings] = useState<Recording[]>([]);
  const [message, setMessage] = useState<{ ok: boolean; text: string } | null>(null);

  const refresh = useCallback(() => {
    client?.listRecordings().then((r) => setRecordings([...r].reverse())).catch((e: Error) => setMessage({ ok: false, text: e.message }));
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
    <Stack spacing={2} sx={{ maxWidth: 900 }}>
      <Stack direction="row" spacing={1} sx={{ alignItems: "center" }}>
        <Typography variant="subtitle1" sx={{ flex: 1 }}>
          Recorded runs
        </Typography>
        <IconButton onClick={refresh} title="Refresh">
          <RefreshIcon />
        </IconButton>
      </Stack>
      {message && (
        <Alert severity={message.ok ? "success" : "error"} onClose={() => setMessage(null)}>
          {message.text}
        </Alert>
      )}
      {!recordings.length && (
        <Typography variant="body2" color="text.secondary">
          No recordings yet. Turn on Record on the Run page to record a run.
        </Typography>
      )}
      {recordings.map((r) => (
        <Paper key={r.id} variant="outlined" sx={{ p: 1.5 }} data-testid="recording">
          <Stack direction="row" spacing={1.5} sx={{ alignItems: "center", mb: 1 }}>
            <Typography variant="subtitle2" sx={{ flex: 1 }}>
              {r.id.split("-").slice(1, -1).join("-") || r.id}{" "}
              <Typography component="span" variant="body2" color="text.secondary">
                {made(r.id)}
              </Typography>
            </Typography>
            <Button
              variant="contained"
              startIcon={<PlayCircleIcon />}
              data-testid="replay"
              onClick={attempt(async () => {
                const s = await client!.openRecording(r.id);
                dispatch(opened({ id: s.id, name: s.name, description: await client!.getDescription(s.id), state: s.state, simMs: s.simMs, network: null, replayEndMs: s.replayEndMs }));
                onReplay();
              })}
            >
              Replay
            </Button>
            <Button
              startIcon={<TableChartIcon />}
              onClick={attempt(async () => {
                await client!.exportParquet(r.id);
                setMessage({ ok: true, text: `Exported ${r.id} to Parquet.` });
                refresh();
              })}
            >
              Export to Parquet
            </Button>
          </Stack>
          <Stack direction="row" spacing={1} sx={{ flexWrap: "wrap" }} useFlexGap>
            {r.files.map((f) =>
              downloads ? (
                <Chip
                  key={f.name}
                  size="small"
                  icon={<DownloadIcon />}
                  label={`${f.name} · ${size(f.bytes)}`}
                  component="a"
                  href={`${base}/api/recordings/${encodeURIComponent(r.id)}/${encodeURIComponent(f.name)}`}
                  clickable
                />
              ) : (
                <Chip key={f.name} size="small" label={`${f.name} · ${size(f.bytes)}`} />
              ),
            )}
          </Stack>
          {!downloads && (
            <Typography variant="caption" color="text.secondary" sx={{ display: "block", mt: 1 }}>
              {r.path}
            </Typography>
          )}
        </Paper>
      ))}
    </Stack>
  );
}
