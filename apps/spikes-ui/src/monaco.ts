// Monaco from the bundle, not a CDN (the desktop app works offline), with its editor worker. The code comes from
// the slim entry (no bundled languages); the types from the full package.
import { loader } from "@monaco-editor/react";
import type * as Monaco from "monaco-editor";
import EditorWorker from "monaco-editor/editor/editor.worker?worker";
import { registerSpikesLanguage } from "./language/register";
import * as slim from "./monacoSlim";

const monaco = slim as unknown as typeof Monaco;

self.MonacoEnvironment = { getWorker: () => new EditorWorker() };
loader.config({ monaco });
registerSpikesLanguage(monaco);

export { monaco };
