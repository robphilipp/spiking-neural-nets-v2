import { useState } from "react";
import { Field } from "@/components/kit";
import type { ComponentProps } from "react";
import { InputFileError, type Random } from "../../inputFile";
import { parseSetting, settingText } from "../../inputEdit";

/**
 * A labelled text field for a value typed in a notation (a time like "500 ms", a list of times, a random value):
 * the text is kept as typed, and the value changes only when it parses; until then the field says why it doesn't.
 * Leaving it shows the value again. Changes made elsewhere show too.
 */
export function ParsedField<T>({
  label,
  value,
  format,
  parse,
  onChange,
  width,
  help,
  mono,
  ...input
}: {
  label: string;
  value: T;
  format: (value: T) => string;
  /** Throws an InputFileError (its message is shown) for text that isn't a value. */
  parse: (text: string) => T;
  onChange: (value: T) => void;
  width?: number;
  help?: string;
  mono?: boolean;
} & Omit<ComponentProps<"input">, "value" | "onChange" | "ref" | "width">) {
  const shown = format(value);
  const [text, setText] = useState(shown);
  const [last, setLast] = useState(shown);
  const attempt = (t: string): { value: T } | { error: string } => {
    try {
      return { value: parse(t) };
    } catch (e) {
      if (e instanceof InputFileError) return { error: e.message };
      throw e;
    }
  };
  // Follow changes made elsewhere, but not the reformatting of what's being typed (adjusted during render).
  if (shown !== last) {
    setLast(shown);
    const now = attempt(text);
    if (!("value" in now) || format(now.value) !== shown) setText(shown);
  }
  const result = attempt(text);
  const error = "error" in result ? result.error : null;
  return (
    <Field
      label={label}
      width={width}
      value={text}
      error={error !== null}
      help={error ?? help}
      spellCheck={false}
      className={mono ? "[&_input]:font-mono" : undefined}
      onChange={(e) => {
        setText(e.target.value);
        const r = attempt(e.target.value);
        if ("value" in r) onChange(r.value);
      }}
      onBlur={() => setText(shown)}
      {...input}
    />
  );
}

/** A setting that is a number or a time, which can also be random per trial ("uniform(500 ms, 1 s)"); `value` null
 *  (with `optional`) is the empty text. */
export interface Setting {
  value: number | null;
  random?: Random;
}

export function SettingField({
  label,
  setting,
  onChange,
  time = false,
  optional = false,
  min = -Infinity,
  width,
  help,
  testId,
  placeholder,
}: {
  label: string;
  setting: Setting;
  onChange: (setting: Setting) => void;
  /** A time ("500 ms") rather than a plain number. */
  time?: boolean;
  /** May be empty (no value). */
  optional?: boolean;
  min?: number;
  width?: number;
  help?: string;
  testId?: string;
  placeholder?: string;
}) {
  return (
    <ParsedField<Setting>
      label={label}
      value={setting}
      width={width}
      help={help}
      data-testid={testId}
      placeholder={placeholder}
      format={(s) => (s.value === null ? "" : settingText(s.value, s.random, time))}
      parse={(text) => {
        if (optional && text.trim() === "") return { value: null };
        const s = parseSetting(text, time, label);
        if (s.value < min) throw new InputFileError(`${label} must be at least ${min}`);
        return s;
      }}
      onChange={onChange}
    />
  );
}
