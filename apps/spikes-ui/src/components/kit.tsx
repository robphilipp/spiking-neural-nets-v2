// The app's building blocks on top of shadcn/ui: labelled fields and switches, panels, stats, notices and a neuron
// combobox. Labels are real <label>s tied to their controls, so they're accessible (and tests find them by label).

import { AlertCircle, CheckCircle2, Info, TriangleAlert, X } from "lucide-react";
import { type ComponentProps, type ReactNode, useId, useMemo, useRef, useState } from "react";
import { Alert, AlertDescription } from "@/components/ui/alert";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { Card } from "@/components/ui/card";
import { Input } from "@/components/ui/input";
import { Label } from "@/components/ui/label";
import { Select, SelectContent, SelectItem, SelectTrigger, SelectValue } from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { cn } from "@/lib/utils";

/** A section of a page. */
export function Panel({ className, ...props }: ComponentProps<"div">) {
  return <Card className={cn("gap-3 px-4 py-3 shadow-none", className)} {...props} />;
}

/** A panel's heading. */
export function Heading({ children, className }: { children: ReactNode; className?: string }) {
  return <h3 className={cn("text-sm font-medium", className)}>{children}</h3>;
}

/** A row of controls that wraps. */
export function Row({ className, ...props }: ComponentProps<"div">) {
  return <div className={cn("flex flex-wrap items-end gap-3", className)} {...props} />;
}

/** A labelled text or number input. */
export function Field({
  label,
  className,
  width,
  help,
  error,
  ...input
}: { label: string; width?: number; help?: ReactNode; error?: boolean } & ComponentProps<typeof Input>) {
  const id = useId();
  return (
    <div className={cn("grid gap-1", className)} style={width ? { width } : undefined}>
      <Label htmlFor={id} className="text-xs font-normal text-muted-foreground">
        {label}
      </Label>
      <Input id={id} aria-invalid={error || undefined} {...input} />
      {help && <p className={cn("text-xs", error ? "text-destructive" : "text-muted-foreground")}>{help}</p>}
    </div>
  );
}

/**
 * A labelled number, typed freely: the text is kept as it's typed (it can be emptied, or be "-" or "0." on the way
 * to a number), and the value changes only when the text is a valid number within the limits. Until then the field
 * shows it's invalid; leaving it shows the last valid value again. Changes made elsewhere (e.g. a reset) show too.
 */
export function NumberField({
  label,
  value,
  onChange,
  min = -Infinity,
  max = Infinity,
  integer = false,
  width,
  help,
  className,
  onBlur,
  title,
  ...input
}: {
  label: string;
  value: number;
  onChange: (value: number) => void;
  min?: number;
  max?: number;
  /** Whole numbers only. */
  integer?: boolean;
  width?: number;
  help?: ReactNode;
} & Omit<ComponentProps<typeof Input>, "value" | "onChange" | "type" | "min" | "max">) {
  const [text, setText] = useState(String(value));
  // Follow changes made elsewhere, while keeping what's being typed (adjusted during render, not in an effect).
  const [shown, setShown] = useState(value);
  if (value !== shown) {
    setShown(value);
    if (parse(text) !== value) setText(String(value));
  }
  function parse(t: string): number | null {
    if (t.trim() === "") return null;
    const v = Number(t);
    return Number.isFinite(v) && (!integer || Number.isInteger(v)) && v >= min && v <= max ? v : null;
  }
  const valid = parse(text) !== null;
  const limits =
    min > -Infinity && max < Infinity ? `from ${min} to ${max}` : min > -Infinity ? `at least ${min}` : max < Infinity ? `at most ${max}` : "";
  return (
    <Field
      label={label}
      width={width}
      className={className}
      inputMode={integer && min >= 0 ? "numeric" : "decimal"}
      value={text}
      error={!valid}
      help={help}
      title={!valid ? `${integer ? "A whole number" : "A number"}${limits ? ` ${limits}` : ""}` : title}
      onChange={(e) => {
        setText(e.target.value);
        const v = parse(e.target.value);
        if (v !== null) onChange(v);
      }}
      onBlur={(e) => {
        setText(String(value));
        onBlur?.(e);
      }}
      {...input}
    />
  );
}

/** A labelled choice from a list. */
export function Choice<T extends string | number>({
  label,
  value,
  options,
  onChange,
  width,
  disabled,
  testId,
}: {
  label: string;
  value: T;
  options: Array<{ value: T; label: string }>;
  onChange: (value: T) => void;
  width?: number;
  disabled?: boolean;
  testId?: string;
}) {
  const id = useId();
  return (
    <div className="grid gap-1" style={width ? { width } : undefined}>
      <Label htmlFor={id} className="text-xs font-normal text-muted-foreground">
        {label}
      </Label>
      <Select
        value={String(value)}
        disabled={disabled}
        onValueChange={(v) => onChange(options.find((o) => String(o.value) === v)!.value)}
      >
        <SelectTrigger id={id} className="w-full" aria-label={label} data-testid={testId}>
          <SelectValue />
        </SelectTrigger>
        <SelectContent>
          {options.map((o) => (
            <SelectItem key={String(o.value)} value={String(o.value)}>
              {o.label}
            </SelectItem>
          ))}
        </SelectContent>
      </Select>
    </div>
  );
}

/** A labelled switch. */
export function Toggle({
  label,
  checked,
  onChange,
  disabled,
  title,
  testId,
}: {
  label: string;
  checked: boolean;
  onChange: (on: boolean) => void;
  disabled?: boolean;
  title?: string;
  testId?: string;
}) {
  const id = useId();
  return (
    <div className="flex h-8 items-center gap-2" title={title}>
      <Switch id={id} checked={checked} disabled={disabled} onCheckedChange={onChange} data-testid={testId} />
      <Label htmlFor={id} className="font-normal">
        {label}
      </Label>
    </div>
  );
}

/** A small labelled figure. */
export function Stat({ label, value, warn, testId }: { label: string; value: ReactNode; warn?: boolean; testId?: string }) {
  return (
    <div className="min-w-0" data-testid={testId}>
      <div className="text-xs text-muted-foreground">{label}</div>
      <div className={cn("text-sm tabular-nums", warn && "text-warning")}>{value}</div>
    </div>
  );
}

/** A session state ("running", "paused", …) as a badge. */
export function StateBadge({ state, ...props }: { state: string | null } & ComponentProps<typeof Badge>) {
  const tone =
    state === "running" || state === "connected"
      ? "bg-success text-white"
      : state === "paused" || state === "connecting"
        ? "bg-warning text-black"
        : state === "failed" || state === "disconnected"
          ? "bg-destructive text-white"
          : "bg-secondary text-secondary-foreground";
  return (
    <Badge className={cn("border-0", tone)} {...props}>
      {state ?? "—"}
    </Badge>
  );
}

type Tone = "info" | "success" | "warning" | "error";
const icons: Record<Tone, ReactNode> = {
  info: <Info />,
  success: <CheckCircle2 />,
  warning: <TriangleAlert />,
  error: <AlertCircle />,
};

/** A message, optionally dismissible, optionally with an action. */
export function Notice({
  tone = "info",
  children,
  onClose,
  action,
  className,
  ...props
}: { tone?: Tone; onClose?: () => void; action?: ReactNode } & Omit<ComponentProps<"div">, "role">) {
  return (
    <Alert
      variant={tone === "error" ? "destructive" : "default"}
      className={cn(tone === "success" && "text-success", tone === "warning" && "text-warning", className)}
      {...props}
    >
      {icons[tone]}
      <AlertDescription className={cn("flex items-start gap-2", tone !== "error" && "text-foreground")}>
        <div className="flex-1">{children}</div>
        {action}
        {onClose && (
          <Button variant="ghost" size="icon-xs" aria-label="Close" onClick={onClose}>
            <X />
          </Button>
        )}
      </AlertDescription>
    </Alert>
  );
}

/** Picks neurons by name, as you type: one, or (with `multiple`) several, shown as removable chips. Its options are
 *  real `role="option"`s, so they can be chosen with the keyboard or the mouse. */
export function NeuronPicker<T extends { index: number; name: string }>({
  label,
  options,
  value,
  onChange,
  max = Infinity,
  width,
  className,
  testId,
}: {
  label: string;
  options: T[];
  value: T[];
  onChange: (value: T[]) => void;
  /** Allow this many (1: a single choice that replaces the previous one). */
  max?: number;
  width?: number;
  className?: string;
  testId?: string;
}) {
  const id = useId();
  const [text, setText] = useState("");
  const [open, setOpen] = useState(false);
  const [active, setActive] = useState(0);
  const input = useRef<HTMLInputElement>(null);
  const single = max === 1;
  const chosen = useMemo(() => new Set(value.map((v) => v.index)), [value]);
  const matches = useMemo(() => {
    const t = text.trim().toLowerCase();
    const list = t ? options.filter((o) => o.name.toLowerCase().includes(t)) : options;
    // Names that start with the text first; a short list, however big the network.
    return list
      .filter((o) => single || !chosen.has(o.index))
      .sort((a, b) => Number(!a.name.toLowerCase().startsWith(t)) - Number(!b.name.toLowerCase().startsWith(t)))
      .slice(0, 50);
  }, [options, text, chosen, single]);
  const pick = (o: T) => {
    if (single) {
      onChange([o]);
      setText(o.name);
      setOpen(false);
      return;
    }
    if (value.length < max) onChange([...value, o]);
    setText("");
    setActive(0);
    input.current?.focus();
  };
  const listId = `${id}-list`;
  return (
    <div className={cn("relative grid gap-1", className)} style={width ? { width } : undefined} data-testid={testId}>
      <Label htmlFor={id} className="text-xs font-normal text-muted-foreground">
        {label}
      </Label>
      <div className="flex min-h-8 flex-wrap items-center gap-1 rounded-lg border border-input bg-transparent px-1.5 py-0.5 dark:bg-input/30">
        {!single &&
          value.map((v) => (
            <Badge key={v.index} variant="secondary" className="gap-1 pr-0.5">
              {v.name}
              <button
                type="button"
                aria-label={`Remove ${v.name}`}
                className="rounded-sm opacity-60 hover:opacity-100"
                onClick={() => onChange(value.filter((x) => x.index !== v.index))}
              >
                <X className="size-3" />
              </button>
            </Badge>
          ))}
        <input
          ref={input}
          id={id}
          role="combobox"
          aria-expanded={open}
          aria-controls={listId}
          aria-autocomplete="list"
          autoComplete="off"
          className="h-6 min-w-24 flex-1 bg-transparent px-1 text-sm outline-none placeholder:text-muted-foreground"
          placeholder={single ? "name" : value.length >= max ? "" : "type a name"}
          disabled={!single && value.length >= max}
          value={text}
          onChange={(e) => {
            setText(e.target.value);
            setOpen(true);
            setActive(0);
            if (single && !e.target.value) onChange([]);
          }}
          onFocus={() => setOpen(true)}
          onBlur={() => setTimeout(() => setOpen(false), 150)}
          onKeyDown={(e) => {
            if (e.key === "Escape") setOpen(false);
            else if (e.key === "ArrowDown") setActive((a) => Math.min(a + 1, matches.length - 1));
            else if (e.key === "ArrowUp") setActive((a) => Math.max(a - 1, 0));
            else if (e.key === "Enter" && open && matches[active]) {
              e.preventDefault();
              pick(matches[active]);
            } else if (e.key === "Backspace" && !text && !single && value.length) onChange(value.slice(0, -1));
          }}
        />
      </div>
      {open && matches.length > 0 && (
        <div
          id={listId}
          role="listbox"
          className="absolute top-full z-50 mt-1 max-h-64 w-full overflow-auto rounded-lg border bg-popover p-1 text-sm text-popover-foreground shadow-md"
        >
          {matches.map((o, i) => (
            <div
              key={o.index}
              role="option"
              aria-selected={i === active}
              className={cn("cursor-pointer rounded-md px-2 py-1", i === active && "bg-accent text-accent-foreground")}
              onMouseDown={(e) => e.preventDefault()}
              onMouseEnter={() => setActive(i)}
              onClick={() => pick(o)}
            >
              {o.name}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}
