import { useId, useState } from "react";
import { Field, Heading, Notice, Panel } from "@/components/kit";
import { Button } from "@/components/ui/button";
import { Label } from "@/components/ui/label";
import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group";
import { useConnection } from "../connection";
import { updateSettings, useSettings } from "../store";
import { inTauri } from "../tauriClient";
import { applyTheme, type ThemeChoice } from "../theme";

export function SettingsPage() {
  const settings = useSettings();
  const { status, error } = useConnection();
  const [server, setServer] = useState(settings.server);
  const [token, setToken] = useState(settings.token);
  const [nodes, setNodes] = useState(settings.nodes);
  const themeId = useId();

  return (
    <div className="flex max-w-xl flex-col gap-3">
      <Panel>
        <Heading>Engine</Heading>
        <Field
          label="Server"
          placeholder={inTauri() ? "this computer (built-in engine)" : "this page's server"}
          help={
            inTauri()
              ? "Empty runs simulations on this computer; or a server, e.g. http://10.0.0.5:7070"
              : "e.g. http://127.0.0.1:7070, or empty for the server this page came from"
          }
          value={server}
          onChange={(e) => setServer(e.target.value)}
        />
        <Field label="Access token" type="password" value={token} onChange={(e) => setToken(e.target.value)} help="If the server was started with --token-file" />
        <div className="flex items-center gap-3">
          <Button onClick={() => updateSettings({ server: server.trim(), token: token.trim() })}>Connect</Button>
          <span className="text-sm text-muted-foreground">{status}</span>
        </div>
        {error && <Notice tone="error">{error}</Notice>}
      </Panel>
      <Panel>
        <Heading>Cluster</Heading>
        <Field
          label="Nodes"
          placeholder="none: the engine alone"
          value={nodes}
          onChange={(e) => setNodes(e.target.value)}
          onBlur={() => updateSettings({ nodes: nodes.trim() })}
          data-testid="cluster-nodes"
          help="Spread networks over other machines: their node agents (spikes-server --node --listen host:port), comma-separated. Each runs the Run page's shards. Applies at the next build."
        />
      </Panel>
      <Panel>
        <Heading>Appearance</Heading>
        <RadioGroup
          className="flex gap-6"
          value={settings.theme}
          aria-labelledby={themeId}
          onValueChange={(value) => {
            const theme = value as ThemeChoice;
            updateSettings({ theme });
            applyTheme(theme);
          }}
        >
          {(["system", "light", "dark"] as const).map((t) => (
            <div key={t} className="flex items-center gap-2">
              <RadioGroupItem value={t} id={`${themeId}-${t}`} />
              <Label htmlFor={`${themeId}-${t}`} className="font-normal capitalize">
                {t}
              </Label>
            </div>
          ))}
        </RadioGroup>
      </Panel>
    </div>
  );
}
