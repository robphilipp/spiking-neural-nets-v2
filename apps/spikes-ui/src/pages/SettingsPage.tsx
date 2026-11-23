import { Alert, Button, FormControlLabel, Paper, Radio, RadioGroup, Stack, TextField, Typography } from "@mui/material";
import { useColorScheme } from "@mui/material/styles";
import { useState } from "react";
import { useConnection } from "../connection";
import { useAppDispatch, useAppSelector } from "../hooks";
import { updateSettings } from "../store";
import type { ThemeChoice } from "../theme";

export function SettingsPage() {
  const settings = useAppSelector((s) => s.settings);
  const dispatch = useAppDispatch();
  const { setMode } = useColorScheme();
  const { status, error } = useConnection();
  const [server, setServer] = useState(settings.server);
  const [token, setToken] = useState(settings.token);

  return (
    <Stack spacing={2} sx={{ maxWidth: 560 }}>
      <Paper variant="outlined" sx={{ p: 2 }}>
        <Typography variant="subtitle1" gutterBottom>
          Engine
        </Typography>
        <Stack spacing={1.5}>
          <TextField
            label="Server"
            placeholder="this page's server"
            helperText="e.g. http://127.0.0.1:7070, or empty for the server this page came from"
            value={server}
            onChange={(e) => setServer(e.target.value)}
          />
          <TextField label="Access token" type="password" value={token} onChange={(e) => setToken(e.target.value)} helperText="If the server was started with --token-file" />
          <Stack sx={{ alignItems: "center" }} direction="row" spacing={1.5}>
            <Button variant="contained" onClick={() => dispatch(updateSettings({ server: server.trim(), token: token.trim() }))}>
              Connect
            </Button>
            <Typography variant="body2" color="text.secondary">
              {status}
            </Typography>
          </Stack>
          {error && <Alert severity="error">{error}</Alert>}
        </Stack>
      </Paper>
      <Paper variant="outlined" sx={{ p: 2 }}>
        <Typography variant="subtitle1" gutterBottom>
          Appearance
        </Typography>
        <RadioGroup
          row
          value={settings.theme}
          onChange={(e) => {
            const theme = e.target.value as ThemeChoice;
            dispatch(updateSettings({ theme }));
            setMode(theme);
          }}
        >
          <FormControlLabel value="system" control={<Radio />} label="System" />
          <FormControlLabel value="light" control={<Radio />} label="Light" />
          <FormControlLabel value="dark" control={<Radio />} label="Dark" />
        </RadioGroup>
      </Paper>
    </Stack>
  );
}
