// The network description language in Monaco: colouring, completions and hover help (from spikes.ts).

import type * as Monaco from "monaco-editor";
import { hover, suggestions } from "./spikes";

export const LANGUAGE_ID = "spikes";

export function registerSpikesLanguage(monaco: typeof Monaco): void {
  if (monaco.languages.getLanguages().some((l) => l.id === LANGUAGE_ID)) return;
  monaco.languages.register({ id: LANGUAGE_ID, extensions: [".boo"] });

  monaco.languages.setLanguageConfiguration(LANGUAGE_ID, {
    comments: { lineComment: "//", blockComment: ["/*", "*/"] },
    brackets: [
      ["(", ")"],
      ["[", "]"],
      ["{", "}"],
    ],
    autoClosingPairs: [
      { open: "(", close: ")" },
      { open: "[", close: "]" },
      { open: "{", close: "}" },
    ],
    wordPattern: /[\w\-.µμ]+/,
  });

  monaco.languages.setMonarchTokensProvider(LANGUAGE_ID, {
    tokenizer: {
      root: [
        [/\/\/.*$/, "comment"],
        [/\/\*/, "comment", "@comment"],
        // Keys: a word before `=` (the section keys of the whole network are keys too).
        [/[a-z][a-z0-9_]*(?=\s*=)/, "variable"],
        [/\b(true|false)\b/, "keyword"],
        [/(uniform|normal)(?=\()/, "keyword"],
        [/-?\d+(\.\d+)?([eE][-+]?\d+)?(\s*(µ|μ|u|m|n|k)?(V|s|m|Hz|Wb|m\/s|rad|deg|min|h)\b)?/, "number"],
        [/[{}()[\]]/, "@brackets"],
        [/[A-Za-z_][\w\-.]*/, "identifier"],
      ],
      comment: [
        [/[^/*]+/, "comment"],
        [/\*\//, "comment", "@pop"],
        [/[/*]/, "comment"],
      ],
    },
  });

  monaco.languages.registerCompletionItemProvider(LANGUAGE_ID, {
    triggerCharacters: ["=", ",", "(", "[", " "],
    provideCompletionItems(model, position) {
      const offset = model.getOffsetAt(position);
      const word = model.getWordUntilPosition(position);
      const range = new monaco.Range(position.lineNumber, word.startColumn, position.lineNumber, position.column);
      const kinds = {
        key: monaco.languages.CompletionItemKind.Property,
        record: monaco.languages.CompletionItemKind.Struct,
        value: monaco.languages.CompletionItemKind.Value,
        snippet: monaco.languages.CompletionItemKind.Snippet,
      };
      return {
        suggestions: suggestions(model.getValue(), offset).map((s) => ({
          label: s.label,
          kind: kinds[s.kind],
          detail: s.detail,
          documentation: { value: s.doc },
          insertText: s.insert,
          insertTextRules: monaco.languages.CompletionItemInsertTextRule.InsertAsSnippet,
          range,
        })),
      };
    },
  });

  monaco.languages.registerHoverProvider(LANGUAGE_ID, {
    provideHover(model, position) {
      const h = hover(model.getValue(), model.getOffsetAt(position));
      if (!h) return null;
      const start = model.getPositionAt(h.start);
      const end = model.getPositionAt(h.end);
      return {
        contents: [{ value: h.markdown }],
        range: new monaco.Range(start.lineNumber, start.column, end.lineNumber, end.column),
      };
    },
  });
}
