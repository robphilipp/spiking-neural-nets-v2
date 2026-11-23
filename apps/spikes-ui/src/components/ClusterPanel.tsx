import { Alert, Box, FormControlLabel, Paper, Stack, Switch, Table, TableBody, TableCell, TableHead, TableRow, Typography } from "@mui/material";
import type { Cluster, ProtocolClient } from "@spikes/client";
import { useEffect, useState } from "react";

function bytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 ** 2) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 ** 3) return `${(n / 1024 ** 2).toFixed(1)} MB`;
  return `${(n / 1024 ** 3).toFixed(2)} GB`;
}

/** A session built on node agents: its nodes, their traffic and how timely spikes are between them, refreshed every
 *  second; and whether runs keep the nodes in lockstep. */
export function ClusterPanel({
  client,
  session,
  running,
  lockstep,
  onLockstep,
}: {
  client: ProtocolClient;
  session: number;
  running: boolean;
  lockstep: boolean;
  onLockstep: (lockstep: boolean) => void;
}) {
  const [cluster, setCluster] = useState<Cluster | null>(null);
  const [error, setError] = useState<string | null>(null);
  useEffect(() => {
    let live = true;
    const poll = () =>
      client
        .getCluster(session)
        .then((c) => live && (setCluster(c), setError(null)))
        .catch((e: unknown) => live && setError(e instanceof Error ? e.message : String(e)));
    void poll();
    const timer = running ? setInterval(poll, 1000) : undefined;
    return () => {
      live = false;
      clearInterval(timer);
    };
  }, [client, session, running]);

  if (error) return <Alert severity="warning">Cluster: {error}</Alert>;
  if (!cluster) return null;
  return (
    <Paper variant="outlined" sx={{ p: 1.5 }} data-testid="cluster">
      <Stack direction="row" spacing={3} sx={{ alignItems: "baseline", flexWrap: "wrap", mb: 1 }} useFlexGap>
        <Typography variant="subtitle2">Cluster · {cluster.nodes.length} nodes</Typography>
        <Typography variant="body2" color="text.secondary" title="Windows agreed with every node">
          {cluster.rounds.toLocaleString()} windows
        </Typography>
        <Typography variant="body2" color="text.secondary" title="Wall time the coordinator waited for the other nodes">
          waited {(cluster.waitedMs / 1000).toFixed(2)} s
        </Typography>
        <Typography variant="body2" color="text.secondary">
          {cluster.received.toLocaleString()} events received
        </Typography>
        <Typography variant="body2" color={cluster.late > 0 ? "warning.main" : "text.secondary"} title="Spikes that arrived after their time and were delivered late">
          {cluster.late.toLocaleString()} late
        </Typography>
        <Box sx={{ flex: 1 }} />
        <FormControlLabel
          control={<Switch checked={lockstep} disabled={running} onChange={(e) => onLockstep(e.target.checked)} data-testid="lockstep" />}
          label="Lockstep (exact)"
          title="On: every window waits for every node, so results are exactly those of one machine. Off: each node follows its copy of the clock (faster over a network); spikes that arrive late are delivered late and counted below. Runs as fast as possible always use lockstep."
        />
      </Stack>
      {cluster.failure && (
        <Alert severity="error" sx={{ mb: 1 }}>
          The cluster failed: {cluster.failure}
        </Alert>
      )}
      <Box sx={{ overflowX: "auto" }}>
        <Table size="small">
          <TableHead>
            <TableRow>
              <TableCell>Node</TableCell>
              <TableCell>Address</TableCell>
              <TableCell align="right">Events sent to it</TableCell>
              <TableCell align="right">Data sent to it</TableCell>
            </TableRow>
          </TableHead>
          <TableBody>
            {cluster.nodes.map((n, i) => (
              <TableRow key={i} data-testid="cluster-node">
                <TableCell>{i}</TableCell>
                <TableCell>{n.address}</TableCell>
                <TableCell align="right">{i === 0 ? "—" : n.eventsSent.toLocaleString()}</TableCell>
                <TableCell align="right">{i === 0 ? "—" : bytes(n.bytesSent)}</TableCell>
              </TableRow>
            ))}
          </TableBody>
        </Table>
      </Box>
      {!cluster.lockstep && cluster.links.length > 0 && (
        <Box sx={{ overflowX: "auto", mt: 1.5 }}>
          <Typography variant="subtitle2" gutterBottom>
            Timeliness (best effort)
          </Typography>
          <Table size="small" data-testid="cluster-links">
            <TableHead>
              <TableRow>
                <TableCell>Link</TableCell>
                <TableCell align="right">Spikes</TableCell>
                <TableCell align="right">Late</TableCell>
                <TableCell align="right">p99 lateness</TableCell>
                <TableCell align="right">Max lateness</TableCell>
              </TableRow>
            </TableHead>
            <TableBody>
              {cluster.links
                .filter((l) => l.events > 0)
                .map((l) => (
                  <TableRow key={`${l.from}-${l.to}`} data-testid="cluster-link">
                    <TableCell>
                      {l.from} → {l.to}
                    </TableCell>
                    <TableCell align="right">{l.events.toLocaleString()}</TableCell>
                    <TableCell align="right" sx={{ color: l.late > 0 ? "warning.main" : undefined }}>
                      {((100 * l.late) / l.events).toFixed(1)}%
                    </TableCell>
                    <TableCell align="right">{l.p99LateMs.toFixed(2)} ms</TableCell>
                    <TableCell align="right">{l.maxLateMs.toFixed(2)} ms</TableCell>
                  </TableRow>
                ))}
            </TableBody>
          </Table>
          <Typography variant="caption" color="text.secondary">
            Slower runs give spikes more time to cross: lateness falls as the speed drops.
          </Typography>
        </Box>
      )}
    </Paper>
  );
}
