import {
  type ProtocolClient,
  type Status,
  WebSocketClient,
} from "@spikes/client";
import {
  createContext,
  type ReactNode,
  useContext,
  useEffect,
  useState,
} from "react";
import { networkUpdated, stateChanged, useSession, useSettings } from "./store";
import { inTauri, TauriClient } from "./tauriClient";

interface Connection {
  client: ProtocolClient | null;
  status: Status;
  error: string | null;
}

/** The latest error the engine reported outside any request (e.g. rejected input), and how often in a row. */
export interface EngineError {
  message: string;
  count: number;
}

const ConnectionContext = createContext<Connection>({
  client: null,
  status: "disconnected",
  error: null,
});
const EngineErrorContext = createContext<{
  error: EngineError | null;
  dismiss: () => void;
}>({ error: null, dismiss: () => {} });

/** The server this page came from, or the dev server's backend. */
export function defaultServer(): string {
  const { protocol, host, port } = window.location;
  if (port === "5173") return "http://127.0.0.1:7070"; // vite dev server: the engine runs separately
  return `${protocol}//${host}`;
}

/** Connects to the engine named in the settings and keeps the session's state in the store. */
export function ConnectionProvider({ children }: { children: ReactNode }) {
  const { server, token } = useSettings();
  const sessionId = useSession((s) => s.id);
  const [connection, setConnection] = useState<Connection>({
    client: null,
    status: "connecting",
    error: null,
  });
  const [engineError, setEngineError] = useState<EngineError | null>(null);

  useEffect(() => {
    let client: WebSocketClient | null = null;
    let cancelled = false;
    setConnection({ client: null, status: "connecting", error: null });
    if (inTauri() && !server) {
      // The desktop app's built-in engine.
      TauriClient.connect()
        .then(
          (c) =>
            !cancelled &&
            setConnection({ client: c, status: "connected", error: null }),
        )
        .catch(
          (e: Error) =>
            !cancelled &&
            setConnection({
              client: null,
              status: "disconnected",
              error: String(e),
            }),
        );
      return () => {
        cancelled = true;
      };
    }
    WebSocketClient.connect(server || defaultServer(), {
      token: token || undefined,
    })
      .then((c) => {
        if (cancelled) {
          c.close();
          return;
        }
        client = c;
        c.onStatus((status) => setConnection((prev) => ({ ...prev, status })));
        setConnection({ client: c, status: "connected", error: null });
      })
      .catch(
        (e: Error) =>
          !cancelled &&
          setConnection({
            client: null,
            status: "disconnected",
            error: e.message,
          }),
      );
    return () => {
      cancelled = true;
      client?.close();
    };
  }, [server, token]);

  // Errors outside any request: until now they went nowhere, so input the engine rejected failed silently.
  useEffect(() => {
    if (!connection.client) return;
    Object.assign(window, { __spikesClient: connection.client }); // for tests and the browser console
    return connection.client.onError((e) =>
      setEngineError((prev) =>
        prev?.message === e.message
          ? { message: e.message, count: prev.count + 1 }
          : { message: e.message, count: 1 },
      ),
    );
  }, [connection.client]);

  useEffect(() => {
    if (!connection.client || sessionId === null) return;
    return connection.client.onNotice((session, notice) => {
      if (session !== sessionId) return;
      if (notice.type === "stateChanged") stateChanged(notice.state);
      if (notice.type === "networkChanged") {
        void connection
          .client!.getNetwork(session)
          .then((n) => networkUpdated(n));
      }
    });
  }, [connection.client, sessionId]);

  return (
    <ConnectionContext value={connection}>
      <EngineErrorContext
        value={{ error: engineError, dismiss: () => setEngineError(null) }}
      >
        {children}
      </EngineErrorContext>
    </ConnectionContext>
  );
}

export const useConnection = () => useContext(ConnectionContext);
export const useEngineError = () => useContext(EngineErrorContext);
