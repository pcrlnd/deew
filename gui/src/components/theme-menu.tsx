import { Moon, Sun, SunMoon } from "lucide-react"

import { Button } from "@/components/ui/button"
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuRadioGroup,
  DropdownMenuRadioItem,
  DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import {
  parseThemeMode,
  setThemeMode,
  useTheme,
  useThemeMode,
  type ThemeMode,
} from "@/lib/theme"

const ICONS = { light: Sun, dark: Moon, system: SunMoon } as const

/**
 * Light / Dark / System menu; the icon shows the picked mode. `onChange` runs
 * after the pick is applied (deew-gui saves it into the config).
 */
export function ThemeMenu({
  onChange,
}: {
  onChange?: (mode: ThemeMode) => void
}) {
  const mode = useThemeMode()
  const theme = useTheme()
  const Icon = ICONS[mode]
  const label =
    mode === "system"
      ? `Theme: System (${theme})`
      : `Theme: ${mode === "dark" ? "Dark" : "Light"}`
  return (
    <DropdownMenu>
      <DropdownMenuTrigger
        render={
          <Button
            variant="ghost"
            size="icon-xs"
            className="size-5"
            aria-label={label}
            title={label}
            tabIndex={-1}
          />
        }
      >
        <Icon />
      </DropdownMenuTrigger>
      <DropdownMenuContent align="end" className="min-w-32">
        <DropdownMenuRadioGroup
          value={mode}
          onValueChange={(value) => {
            const next = parseThemeMode(value)
            setThemeMode(next, { persist: true })
            onChange?.(next)
          }}
        >
          <DropdownMenuRadioItem value="light">
            <Sun />
            Light
          </DropdownMenuRadioItem>
          <DropdownMenuRadioItem value="dark">
            <Moon />
            Dark
          </DropdownMenuRadioItem>
          <DropdownMenuRadioItem value="system">
            <SunMoon />
            System
          </DropdownMenuRadioItem>
        </DropdownMenuRadioGroup>
      </DropdownMenuContent>
    </DropdownMenu>
  )
}
