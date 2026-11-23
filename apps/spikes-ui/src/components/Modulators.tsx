import type { ProtocolClient } from "@spikes/client";
import { FlaskConical } from "lucide-react";
import { useState } from "react";
import { Field, Heading, Panel, Row } from "@/components/kit";
import { Button } from "@/components/ui/button";

/** A release button for each of the network's neuromodulators (e.g. a dopamine "reward"). */
export function Modulators(props: { client: ProtocolClient; session: number; modulators: string[]; onError: (e: string) => void }) {
  const { client, session, modulators, onError } = props;
  const [amounts, setAmounts] = useState<number[]>(() => modulators.map(() => 1));
  if (!modulators.length) return null;
  return (
    <Panel>
      <Row>
        <Heading className="pb-2">Neuromodulators</Heading>
        {modulators.map((name, i) => (
          <Row key={name} className="gap-2">
            <Field label="Amount" type="number" width={90} value={amounts[i] ?? 1} onChange={(e) => setAmounts((a) => a.map((x, j) => (j === i ? Number(e.target.value) : x)))} />
            <Button
              variant="outline"
              data-testid={`release-${name}`}
              onClick={() => client.releaseModulator(session, i, amounts[i] ?? 1).catch((e: Error) => onError(e.message))}
            >
              <FlaskConical data-icon="inline-start" />
              Release {name}
            </Button>
          </Row>
        ))}
      </Row>
    </Panel>
  );
}
