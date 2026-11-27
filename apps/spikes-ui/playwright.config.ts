import { writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { defineConfig } from "@playwright/test";

/** A steady 440 Hz tone for the fake microphone (Chromium's own fake audio is a short beep now and then). */
function toneWav(): string {
  const rate = 48_000;
  const samples = rate; // one second, looped
  const data = Buffer.alloc(samples * 2);
  for (let i = 0; i < samples; i++) data.writeInt16LE(Math.round(12_000 * Math.sin((2 * Math.PI * 440 * i) / rate)), i * 2);
  const header = Buffer.alloc(44);
  header.write("RIFF", 0);
  header.writeUInt32LE(36 + data.length, 4);
  header.write("WAVEfmt ", 8);
  header.writeUInt32LE(16, 16);
  header.writeUInt16LE(1, 20); // PCM
  header.writeUInt16LE(1, 22); // mono
  header.writeUInt32LE(rate, 24);
  header.writeUInt32LE(rate * 2, 28);
  header.writeUInt16LE(2, 32);
  header.writeUInt16LE(16, 34);
  header.write("data", 36);
  header.writeUInt32LE(data.length, 40);
  const path = join(tmpdir(), "spikes-e2e-tone.wav");
  writeFileSync(path, Buffer.concat([header, data]));
  return path;
}

// The UI as a web app, served by a real spikes-server (build both first:
// `cargo build -p spikes-server` and `npm run build`).
const PORT = 17090;
/** Node agents for the cluster test. */
export const NODE_PORTS = [17091, 17092];
/** The server binary (default: the workspace's debug build). */
const SERVER = process.env.SPIKES_SERVER ?? "../../target/debug/spikes-server";

export default defineConfig({
  testDir: "e2e",
  timeout: 60_000,
  use: {
    baseURL: `http://127.0.0.1:${PORT}`,
    viewport: { width: 1400, height: 900 },
    permissions: ["microphone", "camera"],
    // WebGL on the GPU (ANGLE over Metal on macOS), as in a real window; headless Chromium otherwise draws it in
    // software, which says nothing about the 3D view's real cost.
    // A fake microphone (a steady tone) and camera (a moving test pattern), allowed without asking, for the
    // microphone and camera sources.
    launchOptions: {
      args: [
        "--use-angle=metal",
        "--ignore-gpu-blocklist",
        "--enable-gpu",
        "--use-fake-ui-for-media-stream",
        "--use-fake-device-for-media-stream",
        `--use-file-for-fake-audio-capture=${toneWav()}`,
      ],
    },
  },
  webServer: [
    {
      command: `${SERVER} --listen 127.0.0.1:${PORT} --ui dist --recordings ../../target/e2e-recordings`,
      url: `http://127.0.0.1:${PORT}/api/health`,
      reuseExistingServer: false,
    },
    // Two node agents, for the cluster test (separate processes, as on separate machines).
    ...NODE_PORTS.map((port) => ({ command: `${SERVER} --node --listen 127.0.0.1:${port}`, port, reuseExistingServer: false })),
  ],
});
