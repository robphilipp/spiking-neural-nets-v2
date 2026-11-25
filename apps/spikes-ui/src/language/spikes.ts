// The network description language, for the editor: where the cursor is in the record tree, what to suggest
// there, and what to show on hover. Everything comes from the parser's schema (schema.json, written by
// `cargo test -p spikes-dna`), so the editor knows exactly the keys the engine accepts. No Monaco here: these are
// plain functions, tested on their own.

import schemaJson from "./schema.json";

export type Value =
  | { type: "quantity"; quantity: string; unit: string }
  | { type: "number" }
  | { type: "choice"; values: string[] }
  | { type: "name" }
  | { type: "reference"; to: string }
  | { type: "ids" }
  | { type: "bool" }
  | { type: "weight" }
  | { type: "point" }
  | { type: "points" }
  | { type: "record"; record: string }
  | { type: "list"; record: string };

export interface Key {
  key: string;
  doc: string;
  value: Value;
  default?: string;
  required?: boolean;
  only?: string[];
  hidden?: boolean;
}

export interface RecordSchema {
  name: string;
  doc: string;
  kind_key?: string;
  keys: Key[];
}

export const schema = schemaJson as RecordSchema[];
const byName = new Map(schema.map((r) => [r.name, r]));
export const recordSchema = (name: string): RecordSchema | undefined => byName.get(name);

// ----- scanning -----

/** A record as far as it has been written. */
export interface RecordNode {
  record: string;
  /** Scalar values written so far, by key. */
  values: Map<string, string>;
  children: RecordNode[];
  /** The record this one is nested in. */
  parent?: RecordNode;
}

interface RecordFrame {
  type: "record";
  node: RecordNode;
  /** The key being written, or whose value is being written (after `=`). */
  key: string;
  afterEquals: boolean;
  value: string;
}

interface ListFrame {
  type: "list";
  record: string;
  parent: RecordNode;
}

type Frame = RecordFrame | ListFrame;

const isWord = (c: string) => /[\w.\-µμ°+:/]/.test(c);

/** Where the scan ended: what's being written at that point. */
export interface Context {
  /** The record the cursor is in (undefined: before the first `(`, or directly in a list). */
  node?: RecordNode;
  /** Writing a key, or a key's value. */
  mode: "key" | "value" | "list" | "outside";
  /** The key whose value is being written. */
  key?: string;
  /** The word written so far at the cursor. */
  prefix: string;
  /** For `list`: the element record type. */
  listOf?: string;
  /** The whole document's records, for references (names of learning rules, modulators, neurons…). */
  root?: RecordNode;
}

/** Scans `text` up to `end` (default: all of it), building the record tree and the context at the end. */
export function scan(text: string, end = text.length): Context {
  const stack: Frame[] = [];
  let root: RecordNode | undefined;
  let i = 0;
  const top = () => stack[stack.length - 1];
  const pushRecord = (record: string, parent?: RecordNode) => {
    const node: RecordNode = { record, values: new Map(), children: [], parent };
    parent?.children.push(node);
    if (!root) root = node;
    stack.push({ type: "record", node, key: "", afterEquals: false, value: "" });
  };
  const keySchema = (frame: RecordFrame) => recordSchema(frame.node.record)?.keys.find((k) => k.key === frame.key);
  const finishValue = (frame: RecordFrame) => {
    if (frame.afterEquals && frame.key) frame.node.values.set(frame.key, frame.value.trim());
    frame.key = "";
    frame.value = "";
    frame.afterEquals = false;
  };

  while (i < end) {
    const c = text[i]!;
    // Comments: `// …` to the end of the line, and `/* … */`.
    if (c === "/" && text[i + 1] === "/") {
      while (i < end && text[i] !== "\n") i++;
      continue;
    }
    if (c === "/" && text[i + 1] === "*") {
      const close = text.indexOf("*/", i + 2);
      i = close < 0 ? end : Math.min(close + 2, end);
      continue;
    }
    const frame = top();
    // A bracketed value that isn't a record or list of records (a point [x, y, z], uniform(0, 2), (r=…, phi=…),
    // a list of names): part of the value, up to the matching bracket.
    if ((c === "(" || c === "[") && frame?.type === "record" && frame.afterEquals) {
      const t = keySchema(frame)?.value.type;
      if (t !== "record" && t !== "list") {
        let depth = 0;
        do {
          const d = text[i]!;
          if (d === "(" || d === "[") depth++;
          else if (d === ")" || d === "]") depth--;
          frame.value += d;
          i++;
        } while (i < end && depth > 0);
        continue;
      }
    }
    if (c === "(") {
      if (!frame) pushRecord("network");
      else if (frame.type === "list") pushRecord(frame.record, frame.parent);
      else if (frame.afterEquals) {
        const k = keySchema(frame);
        if (k?.value.type === "record") {
          const parent = frame.node;
          const record = k.value.record;
          frame.key = "";
          frame.afterEquals = false;
          pushRecord(record, parent);
        } else frame.value += c; // e.g. a parenthesised value we don't model
      }
      i++;
      continue;
    }
    if (c === "[" && frame?.type === "record" && frame.afterEquals) {
      const k = keySchema(frame);
      if (k?.value.type === "list") {
        stack.push({ type: "list", record: k.value.record, parent: frame.node });
        frame.key = "";
        frame.afterEquals = false;
      }
      i++;
      continue;
    }
    if (c === "{" && frame?.type === "record") {
      // An id set: part of the value, up to the matching brace.
      while (i < end && text[i] !== "}") frame.value += text[i++];
      if (i < end) frame.value += text[i++];
      continue;
    }
    if (c === ")" && frame?.type === "record") {
      finishValue(frame);
      stack.pop();
      i++;
      continue;
    }
    if (c === "]" && frame?.type === "list") {
      stack.pop();
      i++;
      continue;
    }
    if (frame?.type === "record") {
      if (c === ",") finishValue(frame);
      else if (c === "=" && !frame.afterEquals) {
        frame.key = frame.key.trim();
        frame.afterEquals = true;
      } else if (frame.afterEquals) frame.value += c;
      else if (isWord(c)) frame.key += c;
      else if (/\s/.test(c) && frame.key) {
        // a space after a key: keep it (a value may follow `=`)
      }
    }
    i++;
  }

  const frame = top();
  if (!frame) return { mode: "outside", prefix: "", root };
  if (frame.type === "list") return { mode: "list", prefix: "", listOf: frame.record, root };
  if (frame.afterEquals) {
    const written = frame.value.replace(/^\s+/, "");
    return { node: frame.node, mode: "value", key: frame.key, prefix: written, root };
  }
  return { node: frame.node, mode: "key", prefix: frame.key.trim(), root };
}

// ----- what applies where -----

/** The value of a record's kind key (`dynamics`, `rule`), if written. A resonance's kind is its neuron's dynamics. */
export function kindOf(node: RecordNode): string | undefined {
  if (node.record === "resonance") return node.parent?.values.get("dynamics") ?? "monostable_resonator";
  const kindKey = recordSchema(node.record)?.kind_key;
  return kindKey ? node.values.get(kindKey) : undefined;
}

/** The keys that apply to a record, given its kind (all of them while the kind isn't written yet). */
export function applicableKeys(node: RecordNode): Key[] {
  const kind = kindOf(node);
  return (recordSchema(node.record)?.keys ?? []).filter((k) => !k.hidden && (!k.only || !kind || k.only.includes(kind)));
}

function* walk(node: RecordNode | undefined): Generator<RecordNode> {
  if (!node) return;
  yield node;
  for (const child of node.children) yield* walk(child);
}

/** Names defined in the document that a reference can point to. */
export function referenceNames(root: RecordNode | undefined, to: string): string[] {
  const names = new Set<string>();
  const add = (v?: string) => v && names.add(v);
  // Which records define the names each kind of reference points to ("neurons": anything connections and sets take).
  const definedBy: Record<string, string[]> = {
    neurons: ["neuron", "population", "set"],
    neuronType: ["neuronType"],
    synapseType: ["synapseType"],
    node: ["node"],
    modulator: ["modulator"],
    learning: ["learningRule"],
    stickiness: ["stickiness"],
  };
  for (const n of walk(root)) {
    if ((definedBy[to] ?? []).includes(n.record)) add(n.values.get("name"));
  }
  if (to === "learning") names.add("flat");
  if (to === "stickiness") names.add("none");
  if (to === "node") names.add("main");
  return [...names];
}

// ----- completions and hover -----

export interface Suggestion {
  label: string;
  /** Text to insert; `$0`/`${1:…}` are snippet placeholders. */
  insert: string;
  detail: string;
  doc: string;
  kind: "key" | "record" | "value" | "snippet";
}

/** How a key's value is described in one line, e.g. `time (ms), default 20 ms`. */
export function describeValue(k: Key): string {
  const v = k.value;
  const what =
    v.type === "quantity"
      ? `${v.quantity} (${v.unit})`
      : v.type === "choice"
        ? v.values.filter((x) => x !== "stdp").join(" | ")
        : v.type === "reference"
          ? `name of a ${v.to}`
          : v.type === "record"
            ? `record (…)`
            : v.type === "list"
              ? `list [(…), …]`
              : v.type === "ids"
                ? "neurons: a name, a population, a set, n{1:10:1}, or a list [a, b]"
                : v.type === "weight"
                  ? "number, uniform(low, high) or normal(mean, sd)"
                  : v.type === "bool"
                    ? "true | false"
                    : v.type === "point"
                      ? "point: [x, y, z], (r=…, phi=…, z=…) or (r=…, theta=…, phi=…)"
                      : v.type === "points"
                        ? "points: [[x, y, z], …] or [(name=…, at=…), …]"
                        : v.type;
  return [what, k.default ? `default ${k.default}` : k.required ? "required" : ""].filter(Boolean).join(", ");
}

/** A new record of this type with its required keys as placeholders. */
export function recordSnippet(record: string, kind?: string): string {
  const keys = (recordSchema(record)?.keys ?? []).filter((k) => k.required && !k.hidden && (!k.only || !kind || k.only.includes(kind)));
  return `(${keys.map((k, n) => `${k.key}=${placeholder(k, n + 1)}`).join(", ")}$0)`;
}

function placeholder(k: Key, n: number): string {
  const v = k.value;
  if (v.type === "record") return `${recordSnippet(v.record).replace("$0", "")}`;
  if (v.type === "list") return `[${recordSnippet(v.record).replace("$0", "")}]`;
  if (v.type === "choice") return `\${${n}|${v.values.filter((x) => x !== "stdp").join(",")}|}`;
  return `\${${n}:${k.default ?? (v.type === "quantity" ? `1 ${v.unit}` : "")}}`;
}

export function suggestions(text: string, offset: number): Suggestion[] {
  const ctx = scan(text, offset);
  if (ctx.mode === "outside") {
    return [{ label: "network", insert: "(\n\tneurons=[\n\t\t$0\n\t],\n)", detail: "a new network", doc: recordSchema("network")!.doc, kind: "snippet" }];
  }
  if (ctx.mode === "list") {
    const r = recordSchema(ctx.listOf!)!;
    return [{ label: `new ${r.name}`, insert: recordSnippet(r.name), detail: r.doc, doc: r.doc, kind: "snippet" }];
  }
  const node = ctx.node!;
  if (ctx.mode === "key") {
    const present = new Set(node.values.keys());
    return applicableKeys(node)
      .filter((k) => !present.has(k.key))
      .map((k): Suggestion => {
        const v = k.value;
        const insert =
          v.type === "record"
            ? `${k.key}=${recordSnippet(v.record, k.key === "resonance" ? node.values.get("dynamics") : undefined)}`
            : v.type === "list"
              ? `${k.key}=[\n\t${recordSnippet(v.record)}\n]`
              : `${k.key}=${placeholder(k, 1)}`;
        return { label: k.key, insert, detail: describeValue(k), doc: k.doc, kind: v.type === "record" || v.type === "list" ? "record" : "key" };
      });
  }
  // A value.
  const k = recordSchema(node.record)?.keys.find((x) => x.key === ctx.key);
  if (!k) return [];
  const value = (label: string, doc = k.doc): Suggestion => ({ label, insert: label, detail: describeValue(k), doc, kind: "value" });
  switch (k.value.type) {
    case "choice":
      return k.value.values.filter((x) => x !== "stdp").map((x) => value(x));
    case "bool":
      return [value("true"), value("false")];
    case "reference":
      return referenceNames(ctx.root, k.value.to).map((x) => value(x));
    case "ids":
      return referenceNames(ctx.root, "neurons").map((x) => value(x));
    case "point":
      return [value("[0, 0, 0]")];
    case "quantity":
      return [value(k.default ?? `1 ${k.value.unit}`)];
    default:
      return k.default ? [value(k.default)] : [];
  }
}

/** Hover help for the word at `offset`: markdown, or null. */
export function hover(text: string, offset: number): { markdown: string; start: number; end: number } | null {
  let start = offset;
  let end = offset;
  while (start > 0 && /[A-Za-z0-9_]/.test(text[start - 1]!)) start--;
  while (end < text.length && /[A-Za-z0-9_]/.test(text[end]!)) end++;
  const word = text.slice(start, end);
  if (!word || text[end] !== "=") return null; // only keys (followed by `=`)
  const ctx = scan(text, start);
  if (!ctx.node) return null;
  const k = recordSchema(ctx.node.record)?.keys.find((x) => x.key === word);
  if (!k) return null;
  const record = recordSchema(ctx.node.record)!;
  const lines = [`**${k.key}** · ${record.name}`, "", k.doc, "", `*${describeValue(k)}*`];
  if (k.only) lines.push("", `Only for ${record.kind_key ?? "dynamics"} = ${k.only.join(", ")}`);
  return { markdown: lines.join("\n"), start, end };
}
