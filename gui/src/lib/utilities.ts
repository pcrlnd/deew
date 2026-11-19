// Rules of the Utilities tab: which commands may run now, and what the
// output tabs show. The backends enforce the same rules (deew-gui-core's
// info.rs), this only keeps the UI from offering what would be refused.

import type { InfoFlag } from "@/lib/deew-args"
import type { OutputView } from "@/lib/runner"

/** What the output tabs can show; "utilities" is never saved in the config. */
export type OutputTab = OutputView | "utilities"

export const activeTab = (
  view: OutputView,
  utilitiesShown: boolean
): OutputTab => (utilitiesShown ? "utilities" : view)

/** Commands that overwrite user data: they only run after an explicit confirmation. */
export const needsConfirmation = (flag: InfoFlag): boolean => flag === "-gc"

/** The commands that must not run next to an encode (and vice versa). */
const ENCODE_EXCLUSIVE: readonly InfoFlag[] = ["-gt", "-gc"]

export const excludesEncode = (flag: InfoFlag): boolean =>
  ENCODE_EXCLUSIVE.includes(flag)

const ENCODE_REASON: Partial<Record<InfoFlag, string>> = {
  "-gt":
    "Not while an encode is running: it replaces ffmpeg, truehdd and dee_shim, which the encode is using.",
  "-gc": "Not while an encode is running: it rewrites config.toml.",
}

/**
 * Why `flag` cannot run now (shown as the tooltip of the greyed-out menu
 * item), `null` when it can.
 */
export function utilityBlock(
  flag: InfoFlag,
  state: { encoding: boolean; utilityRunning: boolean }
): string | null {
  if (state.utilityRunning) return "Another utility command is still running."
  if (state.encoding && excludesEncode(flag)) return ENCODE_REASON[flag] ?? null
  return null
}

/** Why an encode cannot start while `utility` runs, `null` when it can. */
export function encodeBlock(utility: InfoFlag | null): string | null {
  return utility !== null && excludesEncode(utility)
    ? `Wait for deew ${utility} to finish before running.`
    : null
}
