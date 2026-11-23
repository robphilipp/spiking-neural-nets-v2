import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { Provider } from "react-redux";
import { App } from "./App";
import { ConnectionProvider } from "./connection";
import { store } from "./store";
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

applyTheme(store.getState().settings.theme);

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Provider store={store}>
      <ConnectionProvider>
        <App />
      </ConnectionProvider>
    </Provider>
  </StrictMode>,
);
