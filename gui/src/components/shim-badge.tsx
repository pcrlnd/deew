import { Puzzle } from "lucide-react"

import { Hint } from "@/components/hint"
import { cn } from "@/lib/utils"

/** Small puzzle icon (with tooltip) for values that only work with dee_shim. */
export function ShimBadge({ className }: { className?: string }) {
  return (
    <Hint
      text="Uses dee_shim"
      render={
        <span
          className={cn("ml-auto inline-flex shrink-0 items-center", className)}
        />
      }
    >
      <Puzzle
        className="size-3 shrink-0 text-sidebar-primary"
        aria-label="Uses dee_shim"
      />
    </Hint>
  )
}
