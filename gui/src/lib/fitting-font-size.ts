/**
 * The font size at which at least `minCols` columns fit, given that
 * `proposedCols` columns fit at `currentFont`. Cell width scales linearly with
 * the font size. Never exceeds `maxFont`; rounded down to 0.5px steps.
 */
export function fittingFontSize(
  currentFont: number,
  proposedCols: number,
  minCols = 60,
  maxFont = 13
): number {
  if (!(currentFont > 0) || !(proposedCols > 0))
    return Math.min(currentFont, maxFont)
  // Width available in units of "columns at the current font".
  const ideal = (currentFont * proposedCols) / minCols
  const stepped = Math.floor(ideal * 2) / 2
  return Math.max(1, Math.min(maxFont, stepped))
}
