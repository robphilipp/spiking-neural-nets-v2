import { Plus, Trash2 } from "lucide-react";
import { Choice, Heading, NumberField, Panel, Row, Toggle } from "@/components/kit";
import { Button } from "@/components/ui/button";
import { formatTime, type InputFile, meanOf, type TrialSchedule } from "../../inputFile";
import { changeTrials, trialTotals, withTrials } from "../../inputEdit";
import { uniqueName } from "../../inputEdit";
import { ParsedField, SettingField } from "./ParsedField";

/**
 * The file's trials: how many, how long, the gaps (fixed or random), the order, and the conditions (which trial
 * sources are on in each). Trial sources are the ones whose "Runs" is "in each trial".
 */
export function TrialsEditor({ file, onChange, locked }: { file: InputFile; onChange: (file: InputFile) => void; locked: boolean }) {
  const t = file.trials;
  const trialSources = file.sources.filter((s) => s.scope === "trial");
  const set = (change: Partial<TrialSchedule>) => onChange(changeTrials(file, change));
  const totals = t ? trialTotals(t) : null;
  return (
    <Panel data-testid="trials-editor">
      <div className="flex items-center gap-3">
        <Heading>Trials</Heading>
        <Toggle
          label="Repeat trials"
          checked={t !== undefined}
          onChange={(on) => onChange(withTrials(file, on))}
          disabled={locked}
          testId="trials-toggle"
          title="Repeat the file's trial sources, with the same or random settings, in a number of trials. Off: trial sources become sources of the whole run."
        />
      </div>
      {t && (
        <fieldset disabled={locked} className="m-0 flex min-w-0 flex-col gap-3 border-0 p-0">
          <Row>
            <NumberField
              label={t.conditions.length > 0 ? "Trials per condition" : "Trials"}
              width={140}
              integer
              min={1}
              value={t.count}
              onChange={(count) => set({ count })}
              data-testid="trials-count"
            />
            <SettingField label="Length of a trial" time width={150} min={1} setting={{ value: t.length }} onChange={(x) => set({ length: x.value ?? t.length })} testId="trials-length" />
            <SettingField
              label="Gap between trials"
              time
              width={220}
              setting={{ value: typeof t.gap === "number" ? t.gap : meanOf(t.gap), random: typeof t.gap === "number" ? undefined : t.gap }}
              onChange={(x) => set({ gap: x.random ?? x.value ?? 0 })}
              testId="trials-gap"
            />
            <SettingField label="First trial at" time width={130} setting={{ value: t.start }} onChange={(x) => set({ start: x.value ?? 0 })} />
            {t.conditions.length > 1 && (
              <Choice
                label="Order"
                width={140}
                value={t.order}
                onChange={(order) => set({ order })}
                options={[
                  { value: "shuffled", label: "shuffled" },
                  { value: "blocked", label: "blocked" },
                  { value: "alternating", label: "alternating" },
                ]}
                testId="trials-order"
              />
            )}
          </Row>
          <p className="text-xs text-muted-foreground" data-testid="trials-summary">
            {totals!.trials} trial{totals!.trials === 1 ? "" : "s"}, ending at about {formatTime(Math.round(totals!.endMs))}.
            {trialSources.length === 0 && " No source runs in the trials yet: set a source's Runs to \"in each trial\"."}
          </p>
          <div className="flex flex-col gap-2">
            {t.conditions.map((c, i) => (
              <div key={i} className="flex flex-wrap items-end gap-3 rounded-md border px-3 py-2" data-testid="condition">
                <ParsedField<string>
                  label="Condition"
                  width={170}
                  value={c.name}
                  format={(v) => v}
                  parse={(text) => text}
                  onChange={(name) => set({ conditions: t.conditions.map((x, j) => (j === i ? { ...x, name } : x)) })}
                  data-testid="condition-name"
                />
                <div className="flex min-h-8 flex-1 flex-wrap items-center gap-x-4 gap-y-1">
                  {trialSources.length === 0 && <span className="text-xs text-muted-foreground">no trial sources</span>}
                  {trialSources.map((s) => (
                    <label key={s.name} className="flex items-center gap-1.5 text-sm">
                      <input
                        type="checkbox"
                        checked={c.sources.includes(s.name)}
                        onChange={(e) =>
                          set({
                            conditions: t.conditions.map((x, j) =>
                              j === i ? { ...x, sources: e.target.checked ? [...x.sources, s.name] : x.sources.filter((n) => n !== s.name) } : x,
                            ),
                          })
                        }
                      />
                      {s.name}
                    </label>
                  ))}
                </div>
                <Button variant="ghost" size="icon-sm" aria-label={`Remove condition ${c.name}`} onClick={() => set({ conditions: t.conditions.filter((_, j) => j !== i) })}>
                  <Trash2 />
                </Button>
              </div>
            ))}
            <div>
              <Button
                variant="outline"
                size="sm"
                data-testid="add-condition"
                onClick={() => set({ conditions: [...t.conditions, { name: uniqueName("condition", t.conditions.map((c) => c.name)), sources: [] }] })}
              >
                <Plus data-icon="inline-start" />
                Add condition
              </Button>
              {t.conditions.length === 0 && <span className="ml-3 text-xs text-muted-foreground">Without conditions, every trial source is on in every trial.</span>}
            </div>
          </div>
        </fieldset>
      )}
    </Panel>
  );
}
