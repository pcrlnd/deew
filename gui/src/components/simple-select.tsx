import { ShimBadge } from "@/components/shim-badge"
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select"

export interface SelectChoice {
  value: string
  label: string
  /** Only possible with dee_shim: shown with a puzzle icon. */
  shim?: boolean
  /** Listed but not selectable (greyed out). */
  disabled?: boolean
}

export function SimpleSelect({
  id,
  value,
  onChange,
  choices,
  disabled,
}: {
  id?: string
  value: string
  onChange: (value: string) => void
  choices: SelectChoice[]
  disabled?: boolean
}) {
  return (
    <Select
      value={value}
      onValueChange={(v) => {
        if (v !== null) onChange(String(v))
      }}
      items={choices}
      disabled={disabled}
    >
      <SelectTrigger id={id} size="sm" className="w-full">
        <SelectValue className="min-w-0 overflow-hidden">
          {(v: string | null) => {
            const choice = choices.find((c) => c.value === v)
            return (
              <span className="flex w-full min-w-0 items-center gap-2">
                <span className="min-w-0 truncate">{choice?.label ?? v}</span>
                {choice?.shim && <ShimBadge />}
              </span>
            )
          }}
        </SelectValue>
      </SelectTrigger>
      <SelectContent alignItemWithTrigger={false}>
        {choices.map((c) => (
          <SelectItem key={c.value} value={c.value} disabled={c.disabled}>
            {c.label}
            {c.shim && <ShimBadge />}
          </SelectItem>
        ))}
      </SelectContent>
    </Select>
  )
}
