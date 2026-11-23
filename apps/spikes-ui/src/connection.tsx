import { type ProtocolClient, type Status, WebSocketClient } from "@spikes/client";
import { createContext, type ReactNode, useContext, useEffect, useState } from "react";
import { useAppDispatch, useAppSelector } from "./hooks";
import { networkUpdated, stateChanged } from "./store";
import { inTauri, TauriClient } from "./tauriClient";

interface Connection {
  client: ProtocolClient | null;
  status: Status;
  error: string | null;
}

const ConnectionContext = createContext<Connection>({ client: null, status: "disconnected", error: null });

/** The server this page came from, or the dev server's backend. */
export function defaultServer(): string {
  const { protocol, host, port } = window.location;
  if (port === "5173") return "http://127.0.0.1:7070"; // vite dev server: the engine runs separately
  return `${protocol}//${host}`;
}

/** Connects to the engine named in the settings and keeps the session's state in the store. */
export function ConnectionProvider({ children }: { children: ReactNode }) {
  const { server, token } = useAppSelector((s) => s.settings);
  const sessionId = useAppSelector((s) => s.session.id);
  const dispatch = useAppDispatch();
  const [connection, setConnection] = useState<Connection>({ client: null, status: "connecting", error: null });

  useEffect(() => {
    let client: WebSocketClient | null = null;
    let cancelled = false;
    setConnection({ client: null, status: "connecting", error: null });
    if (inTauri() && !server) {
      // The desktop app's built-in engine.
      TauriClient.connect()
        .then((c) => !cancelled && setConnection({ client: c, status: "connected", error: null }))
        .catch((e: Error) => !cancelled && setConnection({ client: null, status: "disconnected", error: String(e) }));
      return () => {
        cancelled = true;
      };
    }
    WebSocketClient.connect(server || defaultServer(), { token: token || undefined })
      .then((c) => {
        if (cancelled) {
          c.close();
          return;
        }
        client = c;
        c.onStatus((status) => setConnection((prev) => ({ ...prev, status })));
        setConnection({ client: c, status: "connected", error: null });
      })
      .catch((e: Error) => !cancelled && setConnection({ client: null, status: "disconnected", error: e.message }));
    return () => {
      cancelled = true;
      client?.close();
    };
  }, [server, token]);

  useEffect(() => {
    if (!connection.client || sessionId === null) return;
    return connection.client.onNotice((session, notice) => {
      if (session !== sessionId) return;
      if (notice.type === "stateChanged") dispatch(stateChanged(notice.state));
      if (notice.type === "networkChanged") {
        void connection.client!.getNetwork(session).then((n) => dispatch(networkUpdated(n)));
      }
    });
  }, [connection.client, sessionId, dispatch]);

  return <ConnectionContext value={connection}>{children}</ConnectionContext>;
}

export const useConnection = () => useContext(ConnectionContext);
