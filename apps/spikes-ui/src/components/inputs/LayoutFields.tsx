import type { Network } from "@spikes/client";
import { Choice, NumberField, Row } from "@/components/kit";
import { AXES, type Axis, type ChannelSource, type Layout, type LayoutKind, PLANES, type Plane, captureGroups } from "../../channelLayout";
import { type LoadedData } from "../../inputData";
import { type Grid, InputFileError, type Source, formatGrid, parseGrid } from "../../inputFile";
import { mappingWarning, sourceChannels } from "../../sourceChannels";
import { ParsedField } from "./ParsedField";

const KINDS: Array<{ value: LayoutKind; label: string }> = [
  { value: "order", label: "order (the i-th target)" },
  { value: "name", label: "by name" },
  { value: "grid", label: "grid (by position)" },
  { value: "axis", label: "axis (slabs by position)" },
];

const LAYOUT_HELP: Record<LayoutKind, string> = {
  order: "channel i drives the i-th target neuron in network order",
  name: "channels from the neurons' names: natural name order, or a pattern's capture groups",
  grid: "the targets' box in the plane is cut into cells; a channel drives the neurons in its cell",
  axis: "the targets' extent along the axis is cut into slabs, one per channel",
};

/**
 * The position mapping of a source with channels (microphone, camera, signal, file): which target neurons each channel
 * drives (see channelLayout.ts), and below it what that comes to in the built network: the channels, those that get no
 * neuron, the neurons ignored, or why it can't run. `data`: the loaded data files (for a file source's channels).
 */
export function LayoutFields({ s, set, network, data }: { s: ChannelSource; set: (change: Partial<Source>) => void; network: Network | null; data: LoadedData | null }) {
  const layout = s.layout;
  const kind: LayoutKind = layout?.kind ?? "order";
  const own = s.kind === "camera"; // a camera's grid is its own setting, above
  const put = (l: Layout | undefined) => set({ layout: l } as Partial<Source>);
  const gridField = (l: Extract<Layout, { kind: "grid" | "name" }>) =>
    !own &&
    s.kind !== "microphone" && (
      <ParsedField<Grid | undefined>
        label="Grid (columns×rows)"
        width={150}
        value={l.grid}
        format={(g) => (g ? formatGrid(g) : "")}
        parse={(text) => (text.trim() === "" ? undefined : parseGrid(text, "the grid"))}
        onChange={(grid) => put({ ...l, grid })}
        placeholder={l.kind === "grid" ? "1 × channels" : "if the pattern has two groups"}
        data-testid="layout-grid"
      />
    );
  const info = network ? sourceChannels(s, network, data) : null;
  const text = (() => {
    if (!info) return null;
    if ("error" in info) return { error: info.error };
    const r = info.result;
    if (!r) return null;
    if (!r.ok) return { error: r.error };
    const n = r.channels.length;
    const T = info.targets.length;
    const base = `${n} channel${n === 1 ? "" : "s"} for ${T} target neuron${T === 1 ? "" : "s"}`;
    const extra = kind === "order" && n < T ? ` (the other ${T - n} get nothing)` : "";
    const warn = mappingWarning(r);
    return { ok: base + extra, warn };
  })();
  return (
    <>
      <Row>
        <Choice<LayoutKind>
          label="Channels to neurons"
          width={210}
          value={kind}
          onChange={(k) => {
            const grid = layout && "grid" in layout ? { grid: layout.grid } : {};
            put(k === "order" ? undefined : k === "grid" ? { kind: "grid", plane: "xy", ...grid } : k === "axis" ? { kind: "axis", axis: "x" } : { kind: "name", base: 0, ...grid });
          }}
          options={s.kind === "microphone" ? KINDS.filter((k) => k.value !== "grid") : KINDS}
          testId="layout"
        />
        {layout?.kind === "grid" && (
          <Choice<Plane> label="Plane" width={90} value={layout.plane} onChange={(plane) => put({ ...layout, plane })} options={PLANES.map((p) => ({ value: p, label: p }))} testId="layout-plane" />
        )}
        {layout?.kind === "axis" && (
          <Choice<Axis> label="Axis" width={80} value={layout.axis} onChange={(axis) => put({ ...layout, axis })} options={AXES.map((a) => ({ value: a, label: a }))} testId="layout-axis" />
        )}
        {layout?.kind === "grid" && gridField(layout)}
        {layout?.kind === "name" && (
          <>
            <ParsedField<string | undefined>
              label="Pattern (regular expression)"
              width={230}
              mono
              value={layout.pattern}
              format={(v) => v ?? ""}
              parse={(t) => {
                if (t === "") return undefined;
                try {
                  const g = captureGroups(t);
                  if (g < 1 || g > 2) throw new InputFileError(`needs one capture group (the channel) or two (column, row); it has ${g}`);
                } catch (e) {
                  if (e instanceof InputFileError) throw e;
                  throw new InputFileError(`not a valid regular expression: ${e instanceof Error ? e.message : String(e)}`);
                }
                return t;
              }}
              onChange={(pattern) => put({ ...layout, pattern })}
              placeholder="empty: natural name order"
              data-testid="layout-pattern"
            />
            {layout.pattern !== undefined && (
              <NumberField label="Names count from" width={120} integer value={layout.base} onChange={(base) => put({ ...layout, base })} />
            )}
            {gridField(layout)}
          </>
        )}
      </Row>
      <p className="text-xs text-muted-foreground">{LAYOUT_HELP[kind]}.</p>
      {text && "error" in text && (
        <p className="text-xs text-destructive" data-testid="channel-plan">
          {text.error}: fix the targets or the mapping. The source won't start.
        </p>
      )}
      {text && "ok" in text && (
        <p className="text-xs text-muted-foreground" data-testid="channel-plan">
          {text.ok}
          {text.warn && (
            <span className="text-warning" data-testid="layout-warning">
              {" "}
              · {text.warn}
            </span>
          )}
        </p>
      )}
    </>
  );
}
