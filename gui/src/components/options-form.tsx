import { useId, type ReactNode } from "react"
import {
  FileAudio,
  Folder,
  FolderOpen,
  Loader2,
  Trash2,
  X,
} from "lucide-react"

import { Hint } from "@/components/hint"
import { NoteLine } from "@/components/note-line"
import { SimpleSelect, type SelectChoice } from "@/components/simple-select"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import {
  bitrateChoices as buildBitrateChoices,
  DRC_VALUES,
  FORMATS,
  setFlag,
  setFormat,
  type BooleanOption,
  type DeewOptions,
  type Downmix,
  type Drc,
  type BitrateInfo,
  type Format,
} from "@/lib/deew-args"
import { HELP } from "@/lib/help-text"
import { formatSummary } from "@/lib/probe"
import { inWeb } from "@/lib/runner"
import { useFittingPlaceholder } from "@/lib/use-fitting-placeholder"
import type { ProbeState } from "@/lib/use-probe"
import { cn } from "@/lib/utils"

/** Longest first; the Delay placeholder is shortened only when it is clipped. */
const DELAY_PLACEHOLDERS = [
  "-5.1ms, +1,52s, -24@pal",
  "-5.1ms, +1,52s",
  "-5.1ms",
]

const FORMAT_CHOICES: SelectChoice[] = FORMATS.map((f) => ({
  value: f,
  label: f,
}))
const DOWNMIX_LABELS: Record<Downmix, string> = {
  none: "no (keep layout)",
  "1": "1.0 (mono)",
  "2": "2.0 (stereo)",
  "6": "5.1",
}
const DRC_CHOICES: SelectChoice[] = DRC_VALUES.map((d) => ({
  value: d,
  label: d,
}))

const CHECKBOXES: {
  key: BooleanOption
  flag: string
  label: string
  help: string
}[] = [
  {
    key: "keepTemp",
    flag: "-k",
    label: "Keep temp files",
    help: HELP.keepTemp,
  },
  {
    key: "measureOnly",
    flag: "-mo",
    label: "Measure only",
    help: HELP.measureOnly,
  },
  {
    key: "forceStandard",
    flag: "-fs",
    label: "Force standard",
    help: HELP.forceStandard,
  },
  {
    key: "forceBluray",
    flag: "-fb",
    label: "Force bluray",
    help: HELP.forceBluray,
  },
  {
    key: "longArgument",
    flag: "-la",
    label: "Long argument",
    help: HELP.longArgument,
  },
  { key: "noPrompt", flag: "-np", label: "No prompt", help: HELP.noPrompt },
]

function FlagCode({ flag }: { flag: string }) {
  return (
    <code className="font-mono text-xs font-normal text-muted-foreground">
      ({flag})
    </code>
  )
}

function Field({
  label,
  flag,
  help,
  htmlFor,
  disabled,
  className,
  children,
}: {
  label: string
  flag: string
  help: string
  htmlFor: string
  disabled?: boolean
  className?: string
  children: React.ReactNode
}) {
  return (
    <div className={cn("flex min-w-0 flex-col gap-1", className)}>
      <Hint
        text={help}
        render={
          <Label htmlFor={htmlFor} className={cn(disabled && "opacity-50")} />
        }
      >
        {label}
        <FlagCode flag={flag} />
      </Hint>
      {children}
    </div>
  )
}

const MAX_LISTED_ERRORS = 3

/** Summary of the probed inputs, under the input list. */
function ProbeSummaryLine({
  probe,
  warnings,
}: {
  probe: ProbeState
  warnings: string[]
}) {
  const notes: { text: string; title?: string }[] = []
  let summary: string | null = null
  if (probe.status === "done") {
    const s = probe.summary
    summary = s.files === 0 ? "No files found." : formatSummary(s)
    for (const m of s.mismatches) notes.push({ text: m })
    for (const e of s.errors.slice(0, MAX_LISTED_ERRORS)) {
      const name = e.path.split(/[\\/]/).pop() ?? e.path
      notes.push({ text: `${name}: ${e.error}`, title: e.path })
    }
    if (s.errors.length > MAX_LISTED_ERRORS) {
      notes.push({
        text: `and ${s.errors.length - MAX_LISTED_ERRORS} more failed to probe`,
      })
    }
  } else if (probe.status === "error") {
    notes.push({ text: `Probing failed: ${probe.error}` })
  }
  for (const w of warnings) notes.push({ text: w })
  return (
    <div className="max-w-full min-w-0 flex-auto text-xs">
      {summary !== null && (
        <p className="truncate font-mono whitespace-nowrap text-muted-foreground">{summary}</p>
      )}
      {notes.map((n, i) => (
        <NoteLine
          key={i}
          tone="warning"
          text={n.text}
          title={n.title}
          truncate
        />
      ))}
    </div>
  )
}

interface OptionsFormProps {
  value: DeewOptions
  onChange: (next: DeewOptions) => void
  onAddFiles: () => void
  onAddFolders: () => void
  onPickOutput: () => void
  dragging: boolean
  disabled: boolean
  probe: ProbeState
  bitrateInfo: BitrateInfo
  /** `[default_bitrates]` of the config: key -> kbps. */
  defaultBitrates: Record<string, number>
  /** Downmix values that fit the probed input. */
  downmixChoices: Downmix[]
  /** Right-aligned in the "Input(s)" row (web mode, no window bar). */
  toolbar?: ReactNode
}

export function OptionsForm({
  value,
  onChange,
  onAddFiles,
  onAddFolders,
  onPickOutput,
  dragging,
  disabled,
  probe,
  bitrateInfo,
  defaultBitrates,
  downmixChoices,
  toolbar,
}: OptionsFormProps) {
  const uid = useId()
  const { ref: delayRef, placeholder: delayPlaceholder } =
    useFittingPlaceholder<HTMLInputElement>(DELAY_PLACEHOLDERS)
  const id = (name: string) => `${uid}-${name}`
  const patch = (p: Partial<DeewOptions>) => onChange({ ...value, ...p })

  const downmixSelectChoices: SelectChoice[] = downmixChoices.map((d) => ({
    value: d,
    label: DOWNMIX_LABELS[d],
  }))

  // Fields that only make sense once there is something to encode.
  const waiting = disabled || value.inputs.length === 0

  const hasList =
    bitrateInfo.status === "ok" || bitrateInfo.status === "fallback"
  const bitrateChoices: SelectChoice[] = buildBitrateChoices(
    bitrateInfo,
    defaultBitrates
  )

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-col gap-1">
        <div className="flex items-center gap-2">
          <Hint text={HELP.input} render={<Label />}>
            <span className="flex items-center gap-2">
              Input(s)
              {probe.status === "probing" && (
                <span className="flex items-center gap-1 text-xs font-normal text-muted-foreground">
                  <Loader2 className="size-3 animate-spin" />
                  probing…
                </span>
              )}
            </span>
            <FlagCode flag="-i" />
          </Hint>
          {toolbar}
        </div>
        <div
          className={cn(
            "ui-selectable max-h-24 min-h-12 overflow-y-auto rounded-lg border border-dashed border-input p-1 text-xs",
            dragging && "border-primary bg-primary/5"
          )}
        >
          {value.inputs.length === 0 ? (
            <p className="px-2 py-3 text-center text-muted-foreground">
              {inWeb()
                ? "Use the buttons above to add files or folders."
                : "Drop audio files or folders here, or use the buttons above."}
            </p>
          ) : (
            <ul>
              {value.inputs.map((path, i) => (
                <li
                  key={path}
                  className="group flex items-center gap-1 rounded px-1.5 py-0.5 hover:bg-muted"
                >
                  <span
                    className="min-w-0 flex-1 truncate font-mono"
                    title={path}
                  >
                    {path}
                  </span>
                  <button
                    type="button"
                    aria-label={`Remove ${path}`}
                    className="rounded p-0.5 text-muted-foreground hover:text-foreground disabled:opacity-50"
                    disabled={disabled}
                    onClick={() =>
                      patch({ inputs: value.inputs.filter((_, j) => j !== i) })
                    }
                  >
                    <X className="size-3.5" />
                  </button>
                </li>
              ))}
            </ul>
          )}
        </div>
        <div className="flex flex-wrap items-start gap-x-2 gap-y-1">
          <ProbeSummaryLine probe={probe} warnings={bitrateInfo.warnings} />
          <div className="ml-auto flex shrink-0 gap-1.5">
            <Button
              size="sm"
              variant="outline"
              onClick={onAddFiles}
              disabled={disabled}
            >
              <FileAudio data-icon="inline-start" />
              Add files
            </Button>
            <Button
              size="sm"
              variant="outline"
              onClick={onAddFolders}
              disabled={disabled}
            >
              <FolderOpen data-icon="inline-start" />
              Add folder
            </Button>
            <Button
              size="sm"
              variant="destructive"
              onClick={() => patch({ inputs: [] })}
              disabled={disabled || value.inputs.length === 0}
            >
              <Trash2 data-icon="inline-start" />
              Clear
            </Button>
          </div>
        </div>
      </div>

      <div className="@container">
        <div className="grid grid-cols-2 gap-x-3 gap-y-2 @4xl:grid-cols-4">
          <Field
            label="Track index"
            flag="-ti"
            disabled={waiting}
            help={HELP.trackIndex}
            htmlFor={id("ti")}
          >
            <Input
              id={id("ti")}
              type="number"
              min={0}
              step={1}
              className="h-7"
              value={value.trackIndex}
              onChange={(e) => patch({ trackIndex: e.target.value })}
              disabled={waiting}
            />
          </Field>
          <Field
            label="Format"
            flag="-f"
            disabled={waiting}
            help={HELP.format}
            htmlFor={id("f")}
          >
            <SimpleSelect
              id={id("f")}
              value={value.format}
              choices={FORMAT_CHOICES}
              onChange={(v) => onChange(setFormat(value, v as Format))}
              disabled={waiting}
            />
          </Field>
          <Field
            label="Bitrate"
            flag="-b"
            disabled={waiting}
            help={HELP.bitrate}
            htmlFor={id("b")}
          >
            <SimpleSelect
              id={id("b")}
              value={value.bitrate}
              choices={bitrateChoices}
              onChange={(v) => patch({ bitrate: v })}
              disabled={waiting || !hasList}
            />
          </Field>
          <Field
            label="Downmix"
            flag="-dm"
            disabled={waiting}
            help={HELP.downmix}
            htmlFor={id("dm")}
          >
            <SimpleSelect
              id={id("dm")}
              value={value.downmix}
              choices={downmixSelectChoices}
              onChange={(v) => patch({ downmix: v as Downmix })}
              disabled={waiting}
            />
          </Field>
          <Field
            label="Delay"
            flag="-d"
            disabled={waiting}
            help={HELP.delay}
            htmlFor={id("d")}
          >
            <Input
              id={id("d")}
              className="h-7"
              ref={delayRef}
              placeholder={delayPlaceholder}
              value={value.delay}
              onChange={(e) => patch({ delay: e.target.value })}
              disabled={waiting}
              spellCheck={false}
            />
          </Field>
          <Field
            label="DRC"
            flag="-r"
            disabled={waiting}
            help={HELP.drc}
            htmlFor={id("r")}
          >
            <SimpleSelect
              id={id("r")}
              value={value.drc}
              choices={DRC_CHOICES}
              onChange={(v) => patch({ drc: v as Drc })}
              disabled={waiting}
            />
          </Field>
          <Field
            label="Dialnorm"
            flag="-dn"
            disabled={waiting}
            help={HELP.dialnorm}
            htmlFor={id("dn")}
          >
            <Input
              id={id("dn")}
              type="number"
              min={-31}
              max={0}
              step={1}
              className="h-7"
              value={value.dialnorm}
              onChange={(e) => patch({ dialnorm: e.target.value })}
              disabled={waiting}
            />
          </Field>
          <Field
            label="Instances"
            flag="-in"
            disabled={waiting}
            help={HELP.instances}
            htmlFor={id("in")}
          >
            <Input
              id={id("in")}
              className="h-7"
              placeholder="50%"
              value={value.instances}
              onChange={(e) => patch({ instances: e.target.value })}
              disabled={waiting}
              spellCheck={false}
            />
          </Field>
          <Field
            label="Output directory"
            flag="-o"
            help={HELP.output}
            htmlFor={id("o")}
            className="col-span-2 @4xl:col-span-4"
          >
            <div className="flex gap-1.5">
              <Input
                id={id("o")}
                className="h-7 flex-1 font-mono"
                placeholder="default (current directory)"
                value={value.output}
                onChange={(e) => patch({ output: e.target.value })}
                disabled={disabled}
                spellCheck={false}
              />
              <Button
                size="sm"
                variant="outline"
                onClick={onPickOutput}
                disabled={disabled}
              >
                <Folder data-icon="inline-start" />
                Browse
              </Button>
            </div>
          </Field>
        </div>
      </div>

      <div className="flex flex-wrap items-center gap-x-5 gap-y-1.5">
        {CHECKBOXES.map(({ key, flag, label, help }) => (
          <div key={key} className="flex items-center gap-2">
            <Checkbox
              id={id(key)}
              checked={value[key]}
              onCheckedChange={(checked) =>
                onChange(setFlag(value, key, checked === true))
              }
              disabled={disabled}
            />
            <Hint text={help} render={<Label htmlFor={id(key)} />}>
              {label}
              <FlagCode flag={flag} />
            </Hint>
          </div>
        ))}
      </div>
    </div>
  )
}
