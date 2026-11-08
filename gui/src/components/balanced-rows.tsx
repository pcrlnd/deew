import * as React from "react"

import { balancedRowSizes, columnsThatFit } from "@/lib/balanced-rows"

/** Horizontal gap between columns in px (Tailwind gap-x-3). */
const GAP = 12

/**
 * Lays children out in as few rows as fit `minItemWidth`, spreading them
 * evenly so every row is full width.
 */
export function BalancedRows({
  minItemWidth,
  children,
}: {
  minItemWidth: number
  children: React.ReactNode
}) {
  const ref = React.useRef<HTMLDivElement>(null)
  const [width, setWidth] = React.useState(0)

  React.useLayoutEffect(() => {
    const el = ref.current
    if (!el) return
    setWidth(el.getBoundingClientRect().width)
    const observer = new ResizeObserver((entries) => {
      const entry = entries[entries.length - 1]
      if (entry) setWidth(entry.contentRect.width)
    })
    observer.observe(el)
    return () => observer.disconnect()
  }, [])

  const items = React.Children.toArray(children)
  const sizes = balancedRowSizes(
    items.length,
    columnsThatFit(width, minItemWidth, GAP)
  )
  const starts = sizes.map((_, i) =>
    sizes.slice(0, i).reduce((a, b) => a + b, 0)
  )
  return (
    <div ref={ref} className="flex flex-col gap-y-2">
      {sizes.map((size, row) => {
        const slice = items.slice(starts[row], starts[row] + size)
        return (
          <div
            key={row}
            className="grid gap-x-3"
            style={{ gridTemplateColumns: `repeat(${size}, minmax(0, 1fr))` }}
          >
            {slice}
          </div>
        )
      })}
    </div>
  )
}
