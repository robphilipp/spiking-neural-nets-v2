import { CssBaseline } from "@mui/material";
import { ThemeProvider } from "@mui/material/styles";
import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import { Provider } from "react-redux";
import { App } from "./App";
import { ConnectionProvider } from "./connection";
import { store } from "./store";
import { createAppTheme } from "./theme";
import "./monaco";

const theme = {
  ...createAppTheme("light"),
  colorSchemes: { light: createAppTheme("light").palette, dark: createAppTheme("dark").palette },
};

createRoot(document.getElementById("root")!).render(
  <StrictMode>
    <Provider store={store}>
      <ThemeProvider theme={theme} defaultMode={store.getState().settings.theme}>
        <CssBaseline />
        <ConnectionProvider>
          <App />
        </ConnectionProvider>
      </ThemeProvider>
    </Provider>
  </StrictMode>,
);
