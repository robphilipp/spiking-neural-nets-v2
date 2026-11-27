import { Zap } from "lucide-react";
import { Button } from "@/components/ui/button";
import { useConnection } from "../connection";
import type { InputFile } from "../inputFile";
import { useSession } from "../store";

/** Whether pulse sources of `file` (the run's input file, as named) can be fired now: the network is built, and
 *  the engine has the source as it is now. It knows what it was given at Build, so a source whose targets or size
 *  changed since needs a rebuild; other changes to the file (a background input switched on, another source edited)
 *  don't matter to it. Returns the check, per source name. */
export function useCanFire(name: string | null, file: InputFile | null): (source: string) => boolean {
  const network = useSession((s) => s.network);
  const replay = useSession((s) => s.replayEndMs !== null);
  const builtInputs = useSession((s) => s.builtInputs);
  return (source) => {
    if (network === null || replay || file === null) return false;
    if (builtInputs === null) return true; // opened as it was: not known
    // The engine's version: the same source in the file the network was built with.
    let builtFile: InputFile | undefined;
    try {
      const [builtName, parsed] = JSON.parse(builtInputs) as [string | null, InputFile | null];
      if (builtName !== name) return false;
      builtFile = parsed ?? undefined;
    } catch {
      return false;
    }
    const now = file.sources.find((s) => s.name === source);
    const then = builtFile?.sources.find((s) => s.name === source);
    return now !== undefined && then !== undefined && JSON.stringify(now) === JSON.stringify(then);
  };
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
