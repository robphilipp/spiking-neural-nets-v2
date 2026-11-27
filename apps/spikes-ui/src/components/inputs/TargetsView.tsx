import type { Network } from "@spikes/client";
import { useMemo } from "react";
import { NEVER } from "rxjs";
import { channelColor, channelOfNeurons } from "../../channelLayout";
import { useSettings } from "../../store";
import { useChartColors } from "../../theme";
import { useWidth } from "../../hooks/useWidth";
import { Network3D } from "../Network3D";

const HEIGHT = 320;
/** The channel legend under the view. */
const LEGEND_HEIGHT = 24;
/** The hint line under the view (it wraps in a narrow one). */
const HINT_HEIGHT = 40;
const NONE: number[] = [];

/**
 * The built network in 3D with `targets` (neuron indices) in the highlight colour and the others muted. With
 * `channels` (each channel's neurons: a source's position mapping), the targets are coloured by channel instead, with a
 * legend, so the mapping can be checked.
 */
export function TargetsView({ network, targets, channels }: { network: Network; targets: number[] | null; channels?: number[][] | null }) {
  const colors = useChartColors();
  const tooltips = useSettings((s) => s.tooltips);
  const [ref, width] = useWidth();
  const painted = useMemo(() => {
    const on = new Set(targets ?? []);
    const of = channels ? channelOfNeurons(channels, network.neurons.length) : null;
    return network.neurons.map((_, i) => {
      if (of && of[i]! >= 0) return channelColor(of[i]!, channels!.length);
      return on.has(i) ? colors.highlight : colors.color;
    });
  }, [network, targets, channels, colors]);
  return (
    <div ref={ref} className="overflow-hidden rounded-md border" style={{ height: HEIGHT + (channels?.length ? LEGEND_HEIGHT : 0) }} data-testid="targets-view">
      <Network3D
        network={network}
        frames={NEVER}
        width={width}
        height={HEIGHT - HINT_HEIGHT}
        selected={NONE}
        onPick={() => {}}
        onRegion={() => {}}
        tooltips={tooltips}
        colors={painted}
        viewKey="inputs"
        pickHint="its targets are highlighted."
      />
      {channels && channels.length > 0 && (
        <div className="flex items-center gap-2 px-2 text-xs text-muted-foreground" data-testid="channel-legend">
          <span>channel 0</span>
          <span
            className="h-2 flex-1 rounded"
            style={{ background: `linear-gradient(to right, ${Array.from({ length: 8 }, (_, k) => channelColor(k, 8)).join(", ")})` }}
          />
          <span>{channels.length - 1}</span>
        </div>
      )}
    </div>
  );
}
