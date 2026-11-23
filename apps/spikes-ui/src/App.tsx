import CodeIcon from "@mui/icons-material/Code";
import PlayCircleIcon from "@mui/icons-material/PlayCircle";
import SettingsIcon from "@mui/icons-material/Settings";
import { AppBar, Box, Chip, Drawer, List, ListItemButton, ListItemIcon, ListItemText, Toolbar, Typography } from "@mui/material";
import { type ReactNode, useState } from "react";
import { useConnection } from "./connection";
import { useAppSelector } from "./hooks";
import { NetworkPage } from "./pages/NetworkPage";
import { RunPage } from "./pages/RunPage";
import { SettingsPage } from "./pages/SettingsPage";

type Page = "network" | "run" | "settings";
const PAGES: Array<{ id: Page; label: string; icon: ReactNode }> = [
  { id: "network", label: "Network", icon: <CodeIcon /> },
  { id: "run", label: "Run", icon: <PlayCircleIcon /> },
  { id: "settings", label: "Settings", icon: <SettingsIcon /> },
];
const DRAWER = 168;

export function App() {
  const [page, setPage] = useState<Page>("network");
  const { status } = useConnection();
  const name = useAppSelector((s) => s.session.name);
  return (
    <Box sx={{ display: "flex", height: "100vh" }}>
      <AppBar position="fixed" elevation={0} sx={{ zIndex: (t) => t.zIndex.drawer + 1 }}>
        <Toolbar variant="dense">
          <Typography variant="h6" sx={{ mr: 2 }}>
            Spikes
          </Typography>
          <Typography variant="body2" sx={{ flex: 1, opacity: 0.8 }}>
            {name}
          </Typography>
          <Chip
            size="small"
            label={status}
            color={status === "connected" ? "success" : status === "connecting" ? "warning" : "error"}
            data-testid="connection"
          />
        </Toolbar>
      </AppBar>
      <Drawer variant="permanent" sx={{ width: DRAWER, [`& .MuiDrawer-paper`]: { width: DRAWER, boxSizing: "border-box" } }}>
        <Toolbar variant="dense" />
        <List dense>
          {PAGES.map((p) => (
            <ListItemButton key={p.id} selected={page === p.id} onClick={() => setPage(p.id)} data-testid={`nav-${p.id}`}>
              <ListItemIcon sx={{ minWidth: 36 }}>{p.icon}</ListItemIcon>
              <ListItemText primary={p.label} />
            </ListItemButton>
          ))}
        </List>
      </Drawer>
      <Box component="main" sx={{ flex: 1, p: 2, display: "flex", flexDirection: "column", minWidth: 0, overflow: "auto" }}>
        <Toolbar variant="dense" />
        <Box sx={{ flex: 1, minHeight: 0 }}>
          {page === "network" && <NetworkPage />}
          {page === "run" && <RunPage />}
          {page === "settings" && <SettingsPage />}
        </Box>
      </Box>
    </Box>
  );
}
