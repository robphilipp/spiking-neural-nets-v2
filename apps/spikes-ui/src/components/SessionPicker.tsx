import { MenuItem, Select } from "@mui/material";
import type { SessionSummary } from "@spikes/client";
import { useState } from "react";
import { useConnection } from "../connection";
import { useAppDispatch, useAppSelector } from "../hooks";
import { forget, opened } from "../store";

/** Switches between the sessions on the engine (including ones started elsewhere, e.g. by a sensor script). */
export function SessionPicker() {
  const { client } = useConnection();
  const dispatch = useAppDispatch();
  const current = useAppSelector((s) => s.session);
  const [sessions, setSessions] = useState<SessionSummary[]>([]);

  const choose = async (value: string) => {
    if (!client) return;
    if (value === "new") return void dispatch(forget());
    const id = Number(value);
    const summary = sessions.find((s) => s.id === id);
    if (!summary) return;
    const [description, network] = await Promise.all([
      client.getDescription(id),
      summary.state === "loaded" ? Promise.resolve(null) : client.getNetwork(id),
    ]);
    dispatch(opened({ id, name: summary.name, description, state: summary.state, simMs: summary.simMs, network }));
  };

  return (
    <Select
      size="small"
      variant="standard"
      disableUnderline
      value={current.id === null ? "new" : String(current.id)}
      onOpen={() => client?.listSessions().then(setSessions)}
      onChange={(e) => void choose(e.target.value)}
      sx={{ color: "inherit", minWidth: 220, "& .MuiSvgIcon-root": { color: "inherit" } }}
      data-testid="session-picker"
      renderValue={(v) => (v === "new" ? `${current.name} (not on the engine yet)` : `${current.name} · session ${v}`)}
    >
      <MenuItem value="new">New session</MenuItem>
      {current.id !== null && !sessions.some((s) => s.id === current.id) && (
        <MenuItem value={String(current.id)}>{current.name}</MenuItem>
      )}
      {sessions.map((s) => (
        <MenuItem key={s.id} value={String(s.id)}>
          {s.name} · {s.state}
          {s.neurons ? ` · ${s.neurons.toLocaleString()} neurons` : ""}
        </MenuItem>
      ))}
    </Select>
  );
}
