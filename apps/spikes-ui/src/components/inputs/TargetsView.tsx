import type { Network } from "@spikes/client";
import { useMemo } from "react";
import { NEVER } from "rxjs";
import { useSettings } from "../../store";
import { useChartColors } from "../../theme";
import { useWidth } from "../../hooks/useWidth";
import { Network3D } from "../Network3D";

const HEIGHT = 320;
/** The hint line under the view (it wraps in a narrow one). */
const HINT_HEIGHT = 40;
const NONE: number[] = [];

/** The built network in 3D with `targets` (neuron indices) in the highlight colour and the others muted. */
export function TargetsView({ network, targets }: { network: Network; targets: number[] | null }) {
  const colors = useChartColors();
  const tooltips = useSettings((s) => s.tooltips);
  const [ref, width] = useWidth();
  const painted = useMemo(() => {
    const on = new Set(targets ?? []);
    return network.neurons.map((_, i) => (on.has(i) ? colors.highlight : colors.color));
  }, [network, targets, colors]);
  return (
    <div ref={ref} className="overflow-hidden rounded-md border" style={{ height: HEIGHT }} data-testid="targets-view">
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
    </div>
  );
}
