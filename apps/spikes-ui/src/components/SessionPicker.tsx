import type { SessionSummary } from "@spikes/client";
import { useState } from "react";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
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
    dispatch(
      opened({ id, name: summary.name, description, state: summary.state, simMs: summary.simMs, network, replayEndMs: summary.replayEndMs }),
    );
  };

  const value = current.id === null ? "new" : String(current.id);
  return (
    <Select value={value} onOpenChange={(open) => open && client?.listSessions().then(setSessions)} onValueChange={(v) => void choose(v)}>
      <SelectTrigger size="sm" className="min-w-56 border-0 shadow-none dark:bg-transparent" data-testid="session-picker" aria-label="Session">
        <SelectValue>{value === "new" ? `${current.name} (not on the engine yet)` : `${current.name} · session ${value}`}</SelectValue>
      </SelectTrigger>
      <SelectContent>
        <SelectItem value="new">New session</SelectItem>
        {current.id !== null && !sessions.some((s) => s.id === current.id) && <SelectItem value={String(current.id)}>{current.name}</SelectItem>}
        {sessions.map((s) => (
          <SelectItem key={s.id} value={String(s.id)}>
            {s.name} · {s.state}
            {s.neurons ? ` · ${s.neurons.toLocaleString()} neurons` : ""}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  );
}
