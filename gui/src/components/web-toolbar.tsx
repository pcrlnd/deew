import { LogOut } from "lucide-react"

import { ThemeMenu } from "@/components/theme-menu"
import { UtilitiesMenu } from "@/components/utilities-menu"
import { Button } from "@/components/ui/button"
import type { InfoFlag } from "@/lib/deew-args"

/** Utilities menu, theme menu and log out; replaces the window bar. */
export function WebToolbar({
  encoding,
  utilityRunning,
  cliMissing = false,
  onRunInfo,
  onLogout,
}: {
  encoding: boolean
  utilityRunning: boolean
  cliMissing?: boolean
  onRunInfo: (flag: InfoFlag) => void
  onLogout?: () => void
}) {
  return (
    <div className="ml-auto flex items-center gap-1">
      <UtilitiesMenu
        encoding={encoding}
        utilityRunning={utilityRunning}
        cliMissing={cliMissing}
        onRun={onRunInfo}
      />
      <ThemeMenu />
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
