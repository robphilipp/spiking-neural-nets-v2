// Picking neurons by name, for inputs (stimulation, background input, signals): names starting with a prefix, or
// matching a regular expression.

export type MatchMode = "prefix" | "regex";

/** Which neurons: names starting with `pattern`, or matching it as a regular expression. Empty: every neuron. */
export interface NeuronMatch {
  mode: MatchMode;
  pattern: string;
}

/** A name test, or why the pattern isn't one. */
export type CompiledMatch = { test: (name: string) => boolean } | { error: string };

/**
 * Compiles a match. A regular expression is JavaScript's, matched anywhere in the name (`^` and `$` anchor it):
 * `^AS[HK]` matches ASHL, ASHR, ASKL and ASKR.
 */
export function compileMatch({ mode, pattern }: NeuronMatch): CompiledMatch {
  if (!pattern) return { test: () => true };
  if (mode === "prefix") return { test: (name) => name.startsWith(pattern) };
  try {
    const re = new RegExp(pattern, "u");
    return { test: (name) => re.test(name) };
  } catch (e) {
    return { error: `not a regular expression: ${e instanceof Error ? e.message.replace(/^Invalid regular expression: /, "") : String(e)}` };
  }
}

/** The neurons a match picks (in network order). Throws for an invalid regular expression. */
export function matchNeurons<N extends { name: string }>(neurons: N[], match: NeuronMatch): N[] {
  const compiled = compileMatch(match);
  if ("error" in compiled) throw new Error(compiled.error);
  return neurons.filter((n) => compiled.test(n.name));
}

/** How a match reads in a message: "names starting with 'in-'", "names matching /^AS[HK]/". */
export function describeMatch({ mode, pattern }: NeuronMatch): string {
  if (!pattern) return "every neuron";
  return mode === "prefix" ? `names starting with '${pattern}'` : `names matching /${pattern}/`;
}
