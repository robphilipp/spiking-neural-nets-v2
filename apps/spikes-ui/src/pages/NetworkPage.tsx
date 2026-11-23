import CheckIcon from "@mui/icons-material/Check";
import { Alert, Autocomplete, Box, Button, Stack, TextField, Typography } from "@mui/material";
import { useColorScheme } from "@mui/material/styles";
import Editor, { type OnMount } from "@monaco-editor/react";
import { SpikesError } from "@spikes/client";
import { useEffect, useRef, useState } from "react";
import { useConnection } from "../connection";
import { examples } from "../examples";
import { useAppDispatch, useAppSelector } from "../hooks";
import { monaco } from "../monaco";
import { applied, parseFailed, setDescription } from "../store";

/** The network description editor: edit, then apply to the engine, which reports parse errors in place. */
export function NetworkPage() {
  const { client } = useConnection();
  const dispatch = useAppDispatch();
  const { id, name, description, applied: appliedText, errors, state } = useAppSelector((s) => s.session);
  const { mode, systemMode } = useColorScheme();
  const dark = (mode === "system" ? systemMode : mode) === "dark";
  const editorRef = useRef<Parameters<OnMount>[0] | null>(null);
  const [busy, setBusy] = useState(false);
  const [message, setMessage] = useState<string | null>(null);

  // Parse errors as editor markers.
  useEffect(() => {
    const model = editorRef.current?.getModel();
    if (!model) return;
    monaco.editor.setModelMarkers(
      model,
      "spikes",
      errors.map((e) => ({
        severity: monaco.MarkerSeverity.Error,
        message: e.message,
        startLineNumber: e.line,
        startColumn: e.column,
        endLineNumber: e.line,
        endColumn: e.column + 1,
      })),
    );
  }, [errors]);

  const apply = async () => {
    if (!client) return;
    setBusy(true);
    setMessage(null);
    try {
      if (id === null) {
        const session = await client.createSession(name, description);
        dispatch(applied({ id: session.id, description }));
      } else {
        await client.updateDescription(id, description);
        dispatch(applied({ id, description }));
      }
      setMessage("Applied. Build it on the Run page.");
    } catch (e) {
      if (e instanceof SpikesError && e.error.code === "parse") {
        dispatch(parseFailed(e.error.spans ?? []));
        setMessage(`${e.error.spans?.length ?? 0} error(s) in the description`);
      } else {
        setMessage(e instanceof Error ? e.message : String(e));
      }
    } finally {
      setBusy(false);
    }
  };

  const running = state === "running" || state === "paused";
  return (
    <Stack spacing={1.5} sx={{ height: "100%" }}>
      <Stack sx={{ alignItems: "center" }} direction="row" spacing={1.5}>
        <Autocomplete
          sx={{ width: 280 }}
          options={examples}
          getOptionLabel={(o) => o.name}
          renderInput={(params) => <TextField {...params} label="Open an example" />}
          onChange={(_, example) => example && dispatch(setDescription({ name: example.name, description: example.text }))}
        />
        <Typography variant="body2" color="text.secondary" sx={{ flex: 1 }}>
          {name}
          {appliedText !== null && appliedText !== description ? " (changed)" : ""}
        </Typography>
        <Button
          variant="contained"
          startIcon={<CheckIcon />}
          disabled={!client || busy || running || !description.trim()}
          onClick={apply}
          data-testid="apply"
        >
          Apply
        </Button>
      </Stack>
      {message && (
        <Alert severity={errors.length ? "error" : "success"} onClose={() => setMessage(null)}>
          {message}
        </Alert>
      )}
      {running && <Alert severity="info">Stop the run to change the network.</Alert>}
      <Box sx={{ flex: 1, minHeight: 300, border: 1, borderColor: "divider", borderRadius: 1, overflow: "hidden" }}>
        <Editor
          language="plaintext"
          theme={dark ? "vs-dark" : "vs"}
          value={description}
          onChange={(text) => dispatch(setDescription({ description: text ?? "" }))}
          onMount={(editor) => {
            editorRef.current = editor;
          }}
          options={{ minimap: { enabled: false }, fontSize: 13, wordWrap: "on", scrollBeyondLastLine: false }}
        />
      </Box>
    </Stack>
  );
}
