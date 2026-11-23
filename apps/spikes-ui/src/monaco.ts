// Monaco from the bundle, not a CDN (the desktop app works offline), with its editor worker.
import { loader } from "@monaco-editor/react";
import * as monaco from "monaco-editor";
import EditorWorker from "monaco-editor/editor/editor.worker?worker";
import { registerSpikesLanguage } from "./language/register";

self.MonacoEnvironment = { getWorker: () => new EditorWorker() };
loader.config({ monaco });
registerSpikesLanguage(monaco);

export { monaco };
