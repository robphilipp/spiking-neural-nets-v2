import { createTheme, type Theme } from "@mui/material/styles";

export type ThemeChoice = "system" | "light" | "dark";

export function createAppTheme(mode: "light" | "dark"): Theme {
  return createTheme({
    palette: {
      mode,
      primary: { main: mode === "dark" ? "#7cb7ff" : "#1f5fbf" },
      secondary: { main: mode === "dark" ? "#ffb86b" : "#b85c00" },
      background: mode === "dark" ? { default: "#121418", paper: "#1a1d23" } : { default: "#f6f7f9", paper: "#ffffff" },
    },
    shape: { borderRadius: 8 },
    typography: { fontSize: 13 },
    components: {
      MuiButton: { defaultProps: { size: "small", disableElevation: true } },
      MuiTextField: { defaultProps: { size: "small" } },
    },
  });
}

/** Colours for stream-charts, from the MUI theme, so charts follow light and dark mode. */
export interface ChartColors {
  color: string;
  backgroundColor: string;
  series: string;
  highlight: string;
  tooltip: { fontColor: string; backgroundColor: string; borderColor: string };
}

export function chartColors(theme: Theme): ChartColors {
  const p = theme.palette;
  return {
    color: p.text.secondary,
    backgroundColor: p.background.paper,
    series: p.primary.main,
    highlight: p.secondary.main,
    tooltip: { fontColor: p.text.primary, backgroundColor: p.background.paper, borderColor: p.divider },
  };
}
