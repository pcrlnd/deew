import { forwardRef } from "react"
import { Square } from "lucide-react"

import { TerminalPane, type TerminalHandle } from "@/components/terminal-pane"
import { Button } from "@/components/ui/button"

interface UtilitiesPaneProps {
  /** A utility command runs: shows the Stop button. */
  running: boolean
  /** Nothing has run yet: shows the placeholder. */
  empty: boolean
  onStop: () => void
  /** Terminal replies (e.g. the ConPTY cursor report) and keystrokes. */
  onData: (data: string) => void
  onResize: (cols: number, rows: number) => void
  className?: string
}

/** Terminal with the output of the Utilities menu's commands. */
export const UtilitiesPane = forwardRef<TerminalHandle, UtilitiesPaneProps>(
  function UtilitiesPane(
    { running, empty, onStop, onData, onResize, className },
    ref
  ) {
    return (
      <div className={`relative ${className ?? ""}`}>
        {/* Input is forwarded: on Windows the ConPTY waits for the cursor position reply before it prints anything. */}
        <TerminalPane
          ref={ref}
          className="absolute inset-0 overflow-hidden rounded-lg border border-input p-2"
          onData={onData}
          onResize={onResize}
        />
        {empty && (
          <p className="pointer-events-none absolute inset-0 flex items-center justify-center text-sm text-neutral-400">
            Run a command from the Utilities menu
          </p>
        )}
        {running && (
          <Button
            size="xs"
            variant="destructive"
            className="absolute top-3 right-4"
            onClick={onStop}
          >
            <Square data-icon="inline-start" />
            Stop
          </Button>
        )}
      </div>
    )
  }
)
