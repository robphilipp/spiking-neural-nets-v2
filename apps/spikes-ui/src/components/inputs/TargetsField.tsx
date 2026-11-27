import type { Network } from "@spikes/client";
import { Choice, Field } from "@/components/kit";
import { fromMatch, type Targets, toMatch } from "../../inputFile";
import { NeuronMatchField } from "../NeuronMatchField";

/**
 * A source's target neurons: by name (a prefix or a regular expression, with the number matched), or a set of the
 * network (chosen from the built network's sets; typed if there's no network).
 */
export function TargetsField({ targets, onChange, network }: { targets: Targets; onChange: (targets: Targets) => void; network: Network | null }) {
  const isSet = "set" in targets;
  const sets = network?.sets ?? [];
  return (
    <div className="flex flex-wrap items-end gap-3" data-testid="targets">
      {(sets.length > 0 || isSet) && (
        <Choice
          label="Targets"
          width={120}
          value={isSet ? "set" : "neurons"}
          onChange={(v) => onChange(v === "set" ? { set: sets[0]?.name ?? "" } : { prefix: "" })}
          options={[
            { value: "neurons", label: "Neurons" },
            { value: "set", label: "A set" },
          ]}
          testId="targets-kind"
        />
      )}
      {isSet ? (
        sets.length > 0 ? (
          <Choice
            label="Set"
            width={220}
            value={targets.set}
            onChange={(set) => onChange({ set })}
            options={[
              ...(sets.some((s) => s.name === targets.set) ? [] : [{ value: targets.set, label: `${targets.set} (not in this network)` }]),
              ...sets.map((s) => ({ value: s.name, label: `${s.name} (${s.members.length})` })),
            ]}
            testId="target-set"
          />
        ) : (
          <Field label="Set" width={220} value={targets.set} onChange={(e) => onChange({ set: e.target.value })} data-testid="target-set" />
        )
      ) : (
        <NeuronMatchField
          label="Target neurons"
          width={290}
          value={toMatch(targets)}
          onChange={(m) => onChange(fromMatch(m))}
          neurons={network?.neurons}
          placeholder="all"
          testId="target-neurons"
        />
      )}
    </div>
  );
}
