import type { Cluster, ProtocolClient } from "@spikes/client";
import { useEffect, useState } from "react";
import { Heading, Notice, Panel, Toggle } from "@/components/kit";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";

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

  if (error) return <Notice tone="warning">Cluster: {error}</Notice>;
  if (!cluster) return null;
  const muted = "text-sm text-muted-foreground";
  return (
    <Panel data-testid="cluster">
      <div className="flex flex-wrap items-center gap-x-6 gap-y-1">
        <Heading>Cluster · {cluster.nodes.length} nodes</Heading>
        <span className={muted} title="Windows agreed with every node">
          {cluster.rounds.toLocaleString()} windows
        </span>
        <span className={muted} title="Wall time the coordinator waited for the other nodes">
          waited {(cluster.waitedMs / 1000).toFixed(2)} s
        </span>
        <span className={muted}>{cluster.received.toLocaleString()} events received</span>
        <span className={cluster.late > 0 ? "text-sm text-warning" : muted} title="Spikes that arrived after their time and were delivered late">
          {cluster.late.toLocaleString()} late
        </span>
        <div className="flex-1" />
        <Toggle
          label="Lockstep (exact)"
          checked={lockstep}
          disabled={running}
          onChange={onLockstep}
          testId="lockstep"
          title="On: every window waits for every node, so results are exactly those of one machine. Off: each node follows its copy of the clock (faster over a network); spikes that arrive late are delivered late and counted below. Runs as fast as possible always use lockstep."
        />
      </div>
      {cluster.failure && <Notice tone="error">The cluster failed: {cluster.failure}</Notice>}
      <Table>
        <TableHeader>
          <TableRow>
            <TableHead>Node</TableHead>
            <TableHead>Address</TableHead>
            <TableHead className="text-right">Events sent to it</TableHead>
            <TableHead className="text-right">Data sent to it</TableHead>
          </TableRow>
        </TableHeader>
        <TableBody>
          {cluster.nodes.map((n, i) => (
            <TableRow key={i} data-testid="cluster-node">
              <TableCell>{i}</TableCell>
              <TableCell>{n.address}</TableCell>
              <TableCell className="text-right">{i === 0 ? "—" : n.eventsSent.toLocaleString()}</TableCell>
              <TableCell className="text-right">{i === 0 ? "—" : bytes(n.bytesSent)}</TableCell>
            </TableRow>
          ))}
        </TableBody>
      </Table>
      {!cluster.lockstep && cluster.links.length > 0 && (
        <div>
          <Heading className="mb-1">Timeliness (best effort)</Heading>
          <Table data-testid="cluster-links">
            <TableHeader>
              <TableRow>
                <TableHead>Link</TableHead>
                <TableHead className="text-right">Spikes</TableHead>
                <TableHead className="text-right">Late</TableHead>
                <TableHead className="text-right">p99 lateness</TableHead>
                <TableHead className="text-right">Max lateness</TableHead>
              </TableRow>
            </TableHeader>
            <TableBody>
              {cluster.links
                .filter((l) => l.events > 0)
                .map((l) => (
                  <TableRow key={`${l.from}-${l.to}`} data-testid="cluster-link">
                    <TableCell>
                      {l.from} → {l.to}
                    </TableCell>
                    <TableCell className="text-right">{l.events.toLocaleString()}</TableCell>
                    <TableCell className={l.late > 0 ? "text-right text-warning" : "text-right"}>{((100 * l.late) / l.events).toFixed(1)}%</TableCell>
                    <TableCell className="text-right">{l.p99LateMs.toFixed(2)} ms</TableCell>
                    <TableCell className="text-right">{l.maxLateMs.toFixed(2)} ms</TableCell>
                  </TableRow>
                ))}
            </TableBody>
          </Table>
          <p className="text-xs text-muted-foreground">Slower runs give spikes more time to cross: lateness falls as the speed drops.</p>
        </div>
      )}
    </Panel>
  );
}
