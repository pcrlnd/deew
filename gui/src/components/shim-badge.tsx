import { Puzzle } from "lucide-react"

import { Hint } from "@/components/hint"

/** Small puzzle icon (with tooltip) for values that only work with dee_shim. */
export function ShimBadge() {
  return (
    <Hint
      text="Uses dee_shim"
      render={<span className="ml-auto inline-flex shrink-0 items-center" />}
    >
      <Puzzle
        className="size-3 shrink-0 text-sidebar-primary"
        aria-label="Uses dee_shim"
      />
    </Hint>
  )
}
