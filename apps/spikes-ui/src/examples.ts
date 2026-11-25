// Example networks, bundled: the examples and the test networks.
const files = {
  ...import.meta.glob("../../../test-data/examples/*.boo", { query: "?raw", import: "default", eager: true }),
  ...import.meta.glob("../../../test-data/networks/*.boo", { query: "?raw", import: "default", eager: true }),
} as Record<string, string>;

export const examples: Array<{ name: string; text: string }> = Object.entries(files)
  .map(([path, text]) => ({ name: path.split("/").pop()!.replace(/\.boo$/, ""), text }))
  .sort((a, b) => a.name.localeCompare(b.name));
