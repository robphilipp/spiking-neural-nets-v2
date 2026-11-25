import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { App } from "./App";
import { ConnectionProvider } from "./connection";
import { useSettings } from "./store";
import { applyTheme } from "./theme";
import "./index.css";
import "./monaco";
import { invoke } from "@tauri-apps/api/core";
import { selftest } from "./selftest";
import { inTauri } from "./tauriClient";

if (inTauri()) {
  invoke<boolean>("selftest_enabled")
    .then((on) => (on ? selftest() : undefined))
    .catch(() => {});
}

applyTheme(useSettings.getState().theme);

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <ConnectionProvider>
      <App />
    </ConnectionProvider>
  </StrictMode>,
);
