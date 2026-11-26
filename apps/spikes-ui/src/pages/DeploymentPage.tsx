import type { Cluster, DeployedGroup, Deployment } from "@spikes/client";
import { useEffect, useMemo, useState } from "react";
import { NEVER } from "rxjs";
import { Choice, Heading, Notice, Panel, Stat } from "@/components/kit";
import { Badge } from "@/components/ui/badge";
import { Table, TableBody, TableCell, TableHead, TableHeader, TableRow } from "@/components/ui/table";
import { Network3D } from "../components/Network3D";
import { useConnection } from "../connection";
import { useWidth } from "../hooks/useWidth";
import { useLive } from "../live";
import { useSession, useSettings } from "../store";
import { useThemeMode } from "../theme";

/** Colour-blind-safe colours (Okabe–Ito), then evenly spaced hues. */
const PALETTE = ["#0072b2", "#e69f00", "#009e73", "#cc79a7", "#56b4e9", "#d55e00", "#f0e442", "#999999"];
const colorOf = (k: number) => PALETTE[k] ?? `hsl(${(k * 137.5) % 360} 60% 50%)`;

type ColorBy = "node" | "shard" | "type";

const nodeLabel = (d: Deployment, k: number) => d.nodes[k]?.name || `node ${k}`;

/** Where the built network runs: its hosts and their shards, where each population and set landed (and what it was
 *  pinned to), the network coloured by node or shard, and, on a cluster, the traffic between the nodes. */
export function DeploymentPage() {
  const { client } = useConnection();
  const { id, network, build, state } = useSession();
  const tooltips = useSettings((s) => s.tooltips);
  const dark = useThemeMode() === "dark";
  const [deployment, setDeployment] = useState<Deployment | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [colorBy, setColorBy] = useState<ColorBy>("node");
  const [selected, setSelected] = useState<number[]>([]);
  const [group, setGroup] = useState<string | null>(null);
  const [ref, width] = useWidth();
  const running = state === "running" || state === "paused";

  useEffect(() => {
    setDeployment(null);
    setError(null);
    setSelected([]);
    setGroup(null);
    setMembers(null);
    if (!client || id === null || !network) return;
    let live = true;
    client
      .getDeployment(id)
      .then((d) => live && setDeployment(d))
      .catch((e: unknown) => live && setError(e instanceof Error ? e.message : String(e)));
    return () => {
      live = false;
    };
  }, [client, id, network, build]);

  // On a cluster: the traffic between nodes, refreshed every second while running.
  const [cluster, setCluster] = useState<Cluster | null>(null);
  const clustered = (deployment?.nodes.length ?? 0) > 1;
  useEffect(() => {
    setCluster(null);
    if (!client || id === null || !clustered) return;
    let live = true;
    const poll = () =>
      client
        .getCluster(id)
        .then((c) => live && setCluster(c))
        .catch(() => {});
    void poll();
    const timer = running ? setInterval(poll, 1000) : undefined;
    return () => {
      live = false;
      clearInterval(timer);
    };
  }, [client, id, clustered, running, build]);

  // Each node's shards numbered across the cluster, for colouring by shard.
  const shardBase = useMemo(() => {
    const base: number[] = [];
    let total = 0;
    for (const node of deployment?.nodes ?? []) {
      base.push(total);
      total += node.shardNeurons.length;
    }
    return base;
  }, [deployment]);
  /** The neurons of the population (or pinned neuron) picked in the placement table: the rest are dimmed. */
  const [members, setMembers] = useState<Set<number> | null>(null);
  const colors = useMemo(() => {
    if (!deployment || (colorBy === "type" && !members)) return null;
    const dim = dark ? "#3a3a3a" : "#d4d4d4";
    return deployment.neuronNodes.map((node, i) => {
      if (members && !members.has(i)) return dim;
      if (colorBy === "type") return network?.neurons[i]?.inhibitory ? "#cc79a7" : "#0072b2";
      return colorOf(colorBy === "node" ? node : shardBase[node]! + deployment.neuronShards[i]!);
    });
  }, [deployment, colorBy, shardBase, members, network, dark]);

  const live = useLive((s) => s.view);
  const queryState = useMemo(
    () => (client && id !== null ? (neuron: number) => client.queryNeuron(id, neuron) : undefined),
    [client, id],
  );

  if (id === null || !network) {
    return <Notice>Build the network (on the Network or Run page) to see where it runs.</Notice>;
  }
  if (error) return <Notice tone="error">{error}</Notice>;
  if (!deployment) return <Notice>Loading…</Notice>;

  const d = deployment;
  const shards = d.nodes.map((n) => n.shardNeurons.length);
  const totalNeurons = d.neuronNodes.length;
  const totalSynapses = d.nodes.reduce((a, n) => a + n.synapses, 0);
  const pick = (g: DeployedGroup) => {
    if (group === g.name) {
      setGroup(null);
      setMembers(null);
      return;
    }
    setGroup(g.name);
    // A set's members aren't known here: only populations and single neurons pick theirs.
    const picked = network.neurons.filter((n) => (g.kind === "population" ? n.population : n.name) === g.name);
    setMembers(new Set(picked.map((n) => n.index)));
  };
  const share = (n: number, of: number) => (of ? `${((100 * n) / of).toFixed(0)}%` : "—");

  return (
    <div className="flex flex-col gap-3" data-testid="deployment">
      <Panel>
        <div className="flex flex-wrap gap-x-8 gap-y-2">
          <Stat label="Nodes" value={d.nodes.length} />
          <Stat label="Shards" value={shards.every((s) => s === shards[0]) ? `${shards[0]} per node` : shards.join(" · ")} />
          <Stat label="Neurons" value={totalNeurons.toLocaleString()} />
          <Stat label="Synapses" value={totalSynapses.toLocaleString()} />
          {d.declaredNodes.length > 0 && <Stat label="Nodes the network names" value={d.declaredNodes.join(", ")} />}
        </div>
      </Panel>
      {d.warnings.map((w) => (
        <Notice key={w} tone="warning" data-testid="deployment-warning">
          {w}
        </Notice>
      ))}

      <Panel>
        <Heading>Hosts</Heading>
        <Table data-testid="deployment-hosts">
          <TableHeader>
            <TableRow>
              <TableHead>Node</TableHead>
              <TableHead>Address</TableHead>
              <TableHead className="text-right">Neurons</TableHead>
              <TableHead className="text-right">Synapses</TableHead>
              <TableHead>Neurons per shard</TableHead>
            </TableRow>
          </TableHeader>
          <TableBody>
            {d.nodes.map((node, k) => {
              const neurons = node.shardNeurons.reduce((a, b) => a + b, 0);
              const most = Math.max(1, ...node.shardNeurons);
              return (
                <TableRow key={k}>
                  <TableCell>
                    <span className="flex items-center gap-2">
                      <span className="inline-block size-2.5 rounded-full" style={{ background: colorOf(k) }} />
                      {nodeLabel(d, k)}
                    </span>
                  </TableCell>
                  <TableCell className="font-mono text-xs">{node.address || "this engine"}</TableCell>
                  <TableCell className="text-right tabular-nums">
                    {neurons.toLocaleString()} <span className="text-muted-foreground">({share(neurons, totalNeurons)})</span>
                  </TableCell>
                  <TableCell className="text-right tabular-nums">
                    {node.synapses.toLocaleString()} <span className="text-muted-foreground">({share(node.synapses, totalSynapses)})</span>
                  </TableCell>
                  <TableCell>
                    <div className="flex h-5 items-end gap-px" title={node.shardNeurons.map((n, s) => `shard ${s}: ${n.toLocaleString()}`).join("\n")}>
                      {node.shardNeurons.map((n, s) => (
                        <div
                          key={s}
                          className="w-2 rounded-sm"
                          style={{ height: `${Math.max(8, (100 * n) / most)}%`, background: colorBy === "shard" ? colorOf(shardBase[k]! + s) : colorOf(k), opacity: n ? 1 : 0.25 }}
                        />
                      ))}
                    </div>
                  </TableCell>
                </TableRow>
              );
            })}
          </TableBody>
        </Table>
      </Panel>

      <Panel>
        <Heading>Placement</Heading>
        {d.groups.length === 0 ? (
          <p className="text-sm text-muted-foreground">The network has no populations or sets.</p>
        ) : (
          <Table data-testid="deployment-groups">
            <TableHeader>
              <TableRow>
                <TableHead>Population or set</TableHead>
                <TableHead>Pinned to</TableHead>
                {d.nodes.map((_, k) => (
                  <TableHead key={k} className="text-right">
                    {nodeLabel(d, k)}
                  </TableHead>
                ))}
              </TableRow>
            </TableHeader>
            <TableBody>
              {d.groups.map((g) => {
                const total = g.perNode.reduce((a, b) => a + b, 0);
                const selectable = g.kind !== "set";
                return (
                  <TableRow
                    key={`${g.kind}:${g.name}`}
                    data-state={group === g.name ? "selected" : undefined}
                    className={selectable ? "cursor-pointer" : undefined}
                    title={selectable ? "Click to pick it out in the view" : undefined}
                    onClick={selectable ? () => pick(g) : undefined}
                  >
                    <TableCell>
                      {g.name} <span className="text-xs text-muted-foreground">{g.kind}</span>
                    </TableCell>
                    <TableCell>
                      {g.pinned ? <Badge variant="outline">{g.pinned}</Badge> : <span className="text-muted-foreground">auto</span>}
                    </TableCell>
                    {g.perNode.map((n, k) => (
                      <TableCell key={k} className={`text-right tabular-nums ${n ? "" : "text-muted-foreground/50"}`}>
                        {n ? `${n.toLocaleString()} (${share(n, total)})` : "—"}
                      </TableCell>
                    ))}
                  </TableRow>
                );
              })}
            </TableBody>
          </Table>
        )}
      </Panel>

      <Panel>
        <div className="flex flex-wrap items-end gap-3">
          <Heading className="flex-1 self-center">Network by {colorBy === "type" ? "type" : colorBy}</Heading>
          <Choice
            label="Colour by"
            width={160}
            value={colorBy}
            onChange={setColorBy}
            testId="deployment-color-by"
            options={[
              { value: "node", label: "Node" },
              { value: "shard", label: "Shard" },
              { value: "type", label: "Excitatory / inhibitory" },
            ]}
          />
        </div>
        {colorBy === "node" && (
          <div className="flex flex-wrap gap-3 text-xs text-muted-foreground">
            {d.nodes.map((_, k) => (
              <span key={k} className="flex items-center gap-1.5">
                <span className="inline-block size-2.5 rounded-full" style={{ background: colorOf(k) }} />
                {nodeLabel(d, k)}
              </span>
            ))}
          </div>
        )}
        <div ref={ref}>
          <Network3D
            network={network}
            frames={live?.frames ?? NEVER}
            width={width}
            height={420}
            selected={selected}
            onPick={(i) => setSelected([i])}
            onRegion={setSelected}
            queryState={queryState}
            tooltips={tooltips}
            colors={colors}
            pickHint="click a neuron or shift-drag a region to show its connections; click a population above to pick it out."
          />
        </div>
      </Panel>

      {clustered && cluster && (
        <Panel data-testid="deployment-links">
          <Heading>Traffic between nodes</Heading>
          <p className="text-xs text-muted-foreground">
            Events sent from each node (row) to each other node (column) in the current or last run; late ones arrived after their time.
          </p>
          <Table>
            <TableHeader>
              <TableRow>
                <TableHead>From \ to</TableHead>
                {d.nodes.map((_, k) => (
                  <TableHead key={k} className="text-right">
                    {nodeLabel(d, k)}
                  </TableHead>
                ))}
              </TableRow>
            </TableHeader>
            <TableBody>
              {d.nodes.map((_, from) => (
                <TableRow key={from}>
                  <TableCell>{nodeLabel(d, from)}</TableCell>
                  {d.nodes.map((_, to) => {
                    const link = cluster.links.find((l) => l.from === from && l.to === to);
                    return (
                      <TableCell key={to} className="text-right tabular-nums">
                        {from === to || !link ? (
                          <span className="text-muted-foreground/50">—</span>
                        ) : (
                          <>
                            {link.events.toLocaleString()}
                            {link.late > 0 && (
                              <span className="text-warning" title={`max ${link.maxLateMs.toFixed(2)} ms late, p99 ${link.p99LateMs.toFixed(2)} ms`}>
                                {" "}
                                · {link.late.toLocaleString()} late
                              </span>
                            )}
                          </>
                        )}
                      </TableCell>
                    );
                  })}
                </TableRow>
              ))}
            </TableBody>
          </Table>
          {cluster.failure && <Notice tone="error">{cluster.failure}</Notice>}
        </Panel>
      )}
    </div>
  );
}
