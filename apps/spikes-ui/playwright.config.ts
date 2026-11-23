import { defineConfig } from "@playwright/test";

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
    permissions: ["microphone"],
    // WebGL on the GPU (ANGLE over Metal on macOS), as in a real window; headless Chromium otherwise draws it in
    // software, which says nothing about the 3D view's real cost.
    // A fake microphone (a tone), allowed without asking, for the microphone input.
    launchOptions: {
      args: [
        "--use-angle=metal",
        "--ignore-gpu-blocklist",
        "--enable-gpu",
        "--use-fake-ui-for-media-stream",
        "--use-fake-device-for-media-stream",
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
