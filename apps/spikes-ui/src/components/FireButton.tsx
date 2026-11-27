import { Zap } from "lucide-react";
import { Button } from "@/components/ui/button";
import { inputsChanged } from "../buildNetwork";
import { useConnection } from "../connection";
import type { InputFile } from "../inputFile";
import { useSession } from "../store";

/** Whether pulse sources of `file` (the run's input file, as named) can be fired now: the network is built, with
 *  this file as it is now (the engine only knows the sources it was given at Build). */
export function useCanFire(name: string | null, file: InputFile | null): boolean {
  const network = useSession((s) => s.network);
  const replay = useSession((s) => s.replayEndMs !== null);
  const builtInputs = useSession((s) => s.builtInputs);
  return network !== null && !replay && file !== null && !inputsChanged(builtInputs, name, file);
}

/** Fires a pulse source now (what Stimulate did): the engine sends its pulse to its targets, and records it. */
export function FireButton({ source, ready, onError, compact }: { source: string; ready: boolean; onError: (message: string) => void; compact?: boolean }) {
  const { client } = useConnection();
  const id = useSession((s) => s.id);
  return (
    <Button
      variant="outline"
      size="sm"
      disabled={!client || id === null || !ready}
      data-testid={`fire-${source}`}
      title={ready ? `Fire '${source}' now` : "Build the network with this input file to fire its pulses"}
      onClick={() => {
        client!.fire(id!, source).catch((e: unknown) => onError(e instanceof Error ? e.message : String(e)));
      }}
    >
      <Zap data-icon="inline-start" />
      {compact ? "Fire" : `Fire ${source}`}
    </Button>
  );
}
