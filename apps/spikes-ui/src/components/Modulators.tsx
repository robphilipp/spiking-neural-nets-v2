import ScienceIcon from "@mui/icons-material/Science";
import { Button, Paper, Stack, TextField, Typography } from "@mui/material";
import type { ProtocolClient } from "@spikes/client";
import { useState } from "react";

/** A release button for each of the network's neuromodulators (e.g. a dopamine "reward"). */
export function Modulators(props: { client: ProtocolClient; session: number; modulators: string[]; onError: (e: string) => void }) {
  const { client, session, modulators, onError } = props;
  const [amounts, setAmounts] = useState<number[]>(() => modulators.map(() => 1));
  if (!modulators.length) return null;
  return (
    <Paper variant="outlined" sx={{ p: 1.5 }}>
      <Stack direction="row" spacing={1.5} sx={{ alignItems: "center", flexWrap: "wrap" }} useFlexGap>
        <Typography variant="subtitle2">Neuromodulators</Typography>
        {modulators.map((name, i) => (
          <Stack key={name} direction="row" spacing={1} sx={{ alignItems: "center" }}>
            <TextField label="Amount" type="number" value={amounts[i] ?? 1} sx={{ width: 90 }} onChange={(e) => setAmounts((a) => a.map((x, j) => (j === i ? Number(e.target.value) : x)))} />
            <Button
              variant="outlined"
              startIcon={<ScienceIcon />}
              data-testid={`release-${name}`}
              onClick={() => client.releaseModulator(session, i, amounts[i] ?? 1).catch((e: Error) => onError(e.message))}
            >
              Release {name}
            </Button>
          </Stack>
        ))}
      </Stack>
    </Paper>
  );
}
