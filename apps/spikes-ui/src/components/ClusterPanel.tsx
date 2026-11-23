import { Alert, Box, Paper, Stack, Table, TableBody, TableCell, TableHead, TableRow, Typography } from "@mui/material";
import type { Cluster, ProtocolClient } from "@spikes/client";
import { useEffect, useState } from "react";

function bytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 ** 2) return `${(n / 1024).toFixed(1)} KB`;
  if (n < 1024 ** 3) return `${(n / 1024 ** 2).toFixed(1)} MB`;
  return `${(n / 1024 ** 3).toFixed(2)} GB`;
}

/** A session built on node agents: its nodes and their traffic, refreshed every second. */
export function ClusterPanel({ client, session, running }: { client: ProtocolClient; session: number; running: boolean }) {
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
        <Typography variant="body2" color="text.secondary" title="The nodes run in exact lockstep: no spike is ever late">
          {cluster.late} late
        </Typography>
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
    </Paper>
  );
}
