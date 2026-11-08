/** How many columns of at least `minItemWidth` (separated by `gap`) fit in `width`. */
export function columnsThatFit(
  width: number,
  minItemWidth: number,
  gap: number
): number {
  if (!Number.isFinite(width) || width <= 0) return 1
  return Math.max(1, Math.floor((width + gap) / (minItemWidth + gap)))
}

/**
 * Row sizes that spread `count` items evenly over as few rows as `maxColumns`
 * allows; the first rows take the extra items.
 */
export function balancedRowSizes(count: number, maxColumns: number): number[] {
  if (count <= 0) return []
  const cols = Math.max(1, Math.min(maxColumns, count))
  const rows = Math.ceil(count / cols)
  const base = Math.floor(count / rows)
  const extra = count % rows
  return Array.from({ length: rows }, (_, i) => (i < extra ? base + 1 : base))
}
