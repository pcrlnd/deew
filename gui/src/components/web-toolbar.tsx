import { LogOut } from "lucide-react"

import { ThemeToggle } from "@/components/theme-toggle"
import { UtilitiesMenu } from "@/components/utilities-menu"
import { Button } from "@/components/ui/button"
import type { InfoFlag } from "@/lib/deew-args"

/** Utilities menu, theme toggle and log out; replaces the window bar. */
export function WebToolbar({
  encoding,
  utilityRunning,
  onRunInfo,
  onLogout,
}: {
  encoding: boolean
  utilityRunning: boolean
  onRunInfo: (flag: InfoFlag) => void
  onLogout?: () => void
}) {
  return (
    <div className="ml-auto flex items-center gap-1">
      <UtilitiesMenu
        encoding={encoding}
        utilityRunning={utilityRunning}
        onRun={onRunInfo}
      />
      <ThemeToggle />
      {onLogout && (
        <Button
          variant="ghost"
          size="icon-xs"
          className="size-5"
          aria-label="Log out"
          title="Log out"
          tabIndex={-1}
          onClick={onLogout}
        >
          <LogOut />
        </Button>
      )}
    </div>
  )
}
