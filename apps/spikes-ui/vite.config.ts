import { fileURLToPath } from "node:url";
import tailwindcss from "@tailwindcss/vite";
import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

const root = fileURLToPath(new URL("../..", import.meta.url));

export default defineConfig({
  plugins: [react(), tailwindcss()],
  resolve: {
    // monaco-editor's package exports don't expose its CSS; the slim entry (src/monacoSlim.ts) imports its sources.
    alias: {
      "monaco-vs": fileURLToPath(new URL("../../node_modules/monaco-editor/esm/vs", import.meta.url)),
      "@": fileURLToPath(new URL("./src", import.meta.url)),
    },
  },
  // Tauri serves the built files from disk; relative paths work there and under spikes-server alike.
  base: "./",
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    // Example networks are bundled from test-data/ at the repository root.
    fs: { allow: [root] },
  },
  build: { outDir: "dist", chunkSizeWarningLimit: 6000 },
});
