import { useId } from "react";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { cn } from "@/lib/utils";
import { compileMatch, type MatchMode, type NeuronMatch } from "../neuronMatch";

const MODES: Array<{ mode: MatchMode; label: string; title: string }> = [
  { mode: "prefix", label: "Starts with", title: "Neurons whose names start with this text" },
  {
    mode: "regex",
    label: "Regex",
    title: "Neurons whose names match this regular expression (anywhere in the name: use ^ and $ to anchor), e.g. ^AS[HK]",
  },
];

/**
 * Picks neurons by name, for an input: a prefix, or a regular expression (the toggle in the label row). Shows how
 * many of `neurons` match (when given), and says so when a regular expression is invalid.
 */
export function NeuronMatchField({
  label,
  value,
  onChange,
  neurons,
  width,
  placeholder,
  testId,
}: {
  label: string;
  value: NeuronMatch;
  onChange: (match: NeuronMatch) => void;
  /** The network's neuron names, to count the matches. */
  neurons?: Array<{ name: string }>;
  width?: number;
  placeholder?: string;
  testId?: string;
}) {
  const id = useId();
  const compiled = compileMatch(value);
  const error = "error" in compiled ? compiled.error : null;
  const count = neurons && !error ? neurons.filter((n) => (compiled as { test: (s: string) => boolean }).test(n.name)).length : null;
  return (
    <div className="grid gap-1" style={width ? { width } : undefined} data-testid={testId}>
      <div className="flex items-center gap-1.5 whitespace-nowrap">
        <Label htmlFor={id} className="text-xs font-normal whitespace-nowrap text-muted-foreground">
          {label}
        </Label>
        {count !== null && (
          <span className={cn("text-xs tabular-nums", count === 0 ? "text-destructive" : "text-muted-foreground")} data-testid={testId && `${testId}-count`} title={`${count} neuron${count === 1 ? "" : "s"} match`}>
            · {count}
          </span>
        )}
        <div className="ml-auto flex shrink-0 overflow-hidden rounded-md border text-[11px]" role="radiogroup" aria-label={`${label}: match by`}>
          {MODES.map((m) => (
            <button
              key={m.mode}
              type="button"
              role="radio"
              aria-checked={value.mode === m.mode}
              title={m.title}
              className={cn(
                "px-1.5 leading-4 whitespace-nowrap",
                value.mode === m.mode ? "bg-primary text-primary-foreground" : "text-muted-foreground hover:bg-muted",
              )}
              onClick={() => onChange({ ...value, mode: m.mode })}
            >
              {m.label}
            </button>
          ))}
        </div>
      </div>
      <Input
        id={id}
        value={value.pattern}
        placeholder={placeholder ?? (value.mode === "regex" ? "e.g. ^AS[HK]" : undefined)}
        spellCheck={false}
        aria-invalid={error ? true : undefined}
        title={error ?? undefined}
        className={value.mode === "regex" ? "font-mono" : undefined}
        onChange={(e) => onChange({ ...value, pattern: e.target.value })}
      />
      {error && <p className="text-xs text-destructive">{error}</p>}
    </div>
  );
}
