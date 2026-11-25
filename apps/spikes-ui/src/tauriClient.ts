// The desktop app's transport: the engine runs in the app's own process, reached through Tauri IPC.

import { ProtocolClient } from "@spikes/client";
import { Channel, invoke } from "@tauri-apps/api/core";

/** True inside the desktop app. */
export const inTauri = () => typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

export class TauriClient extends ProtocolClient {
  private id = -1;

  private constructor() {
    super();
  }

  /** Connects to the built-in engine; each client gets a connection of its own. */
  static async connect(): Promise<TauriClient> {
    const client = new TauriClient();
    const messages = new Channel<string>();
    messages.onmessage = (text) => client.receiveText(text);
    const frames = new Channel<ArrayBuffer>();
    frames.onmessage = (bytes) => client.receiveBinary(bytes);
    client.id = await invoke<number>("engine_connect", { messages, frames });
    return client;
  }

  protected sendText(text: string): void {
    invoke("engine_send", { connection: this.id, text }).catch((e: unknown) => console.error("engine_send failed", e));
  }

  protected sendBinary(data: ArrayBuffer): void {
    invoke("engine_send_binary", new Uint8Array(data), { headers: { "spikes-connection": String(this.id) } }).catch((e: unknown) => console.error("engine_send_binary failed", e));
  }
}
