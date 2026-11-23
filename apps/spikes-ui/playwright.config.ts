import { defineConfig } from "@playwright/test";

// The UI as a web app, served by a real spikes-server (build both first:
// `cargo build -p spikes-server` and `npm run build`).
const PORT = 17090;
/** The server binary (default: the workspace's debug build). */
const SERVER = process.env.SPIKES_SERVER ?? "../../target/debug/spikes-server";

export default defineConfig({
  testDir: "e2e",
  timeout: 60_000,
  use: {
    baseURL: `http://127.0.0.1:${PORT}`,
    viewport: { width: 1400, height: 900 },
    // WebGL on the GPU (ANGLE over Metal on macOS), as in a real window; headless Chromium otherwise draws it in
    // software, which says nothing about the 3D view's real cost.
    launchOptions: { args: ["--use-angle=metal", "--ignore-gpu-blocklist", "--enable-gpu"] },
  },
  webServer: {
    command: `${SERVER} --listen 127.0.0.1:${PORT} --ui dist --recordings ../../target/e2e-recordings`,
    url: `http://127.0.0.1:${PORT}/api/health`,
    reuseExistingServer: false,
  },
});
