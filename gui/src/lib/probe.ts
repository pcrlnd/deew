// Types and pure helpers for the input probing (see probe_inputs in
// src-tauri/src/lib.rs). No Tauri / DOM dependencies.

/** Result for one input file; either the properties or `error`. */
export interface FileProbe {
  path: string
  channels: number | null
  sampleRate: number | null
  /** Raw bit depth, as the CLI compares it. */
  bitDepth: number | null
  error: string | null
}

export interface ProbeResult {
  ffprobe: string
  files: FileProbe[]
}

export interface ProbeSummary {
  /** Number of files (after expanding folders). */
  files: number
  /** Common values; `null` if unknown or the files differ. */
  channels: number | null
  sampleRate: number | null
  bitDepth: number | null
  /** True if the (normalised) bit depths differ between files. */
  mixedBitDepth: boolean
  /** Like the CLI's mismatch errors. */
  mismatches: string[]
  errors: { path: string; error: string }[]
}

function unique(values: number[]): number[] {
  return [...new Set(values)]
}

/** Python style list, as printed by the CLI: `[6, 2]`. */
function intList(values: number[]): string {
  return `[${values.join(", ")}]`
}

/** Condenses the per-file results into what the CLI would derive from them. */
export function summarizeProbe(files: readonly FileProbe[]): ProbeSummary {
  const errors = files
    .filter((f) => f.error !== null)
    .map((f) => ({ path: f.path, error: f.error ?? "" }))
  const ok = files.filter((f) => f.error === null)
  const pick = (get: (f: FileProbe) => number | null): number[] =>
    ok.map(get).filter((v): v is number => v !== null)

  const rates = pick((f) => f.sampleRate)
  const channels = pick((f) => f.channels)
  const depths = pick((f) => f.bitDepth)

  const mismatches: string[] = []
  const check = (values: number[], what: string) => {
    if (unique(values).length > 1) {
      mismatches.push(
        `each input has to have the same ${what}: ${intList(values)}`
      )
    }
  }
  check(rates, "sample rate")
  check(channels, "channel count")

  const normDepths = unique(depths.map(normalizeBitDepth))
  const common = (values: number[]): number | null =>
    unique(values).length === 1 ? values[0] : null
  return {
    files: files.length,
    // Without every file probed the layout is not known.
    channels: errors.length === 0 ? common(channels) : null,
    sampleRate: common(rates),
    // Compared after the CLI's normalisation.
    bitDepth: normDepths.length === 1 ? normDepths[0] : null,
    mixedBitDepth: normDepths.length > 1,
    mismatches,
    errors,
  }
}

/** `1.0`, `2.0`, `5.1`, `7.1`; other counts as `N ch`. */
export function formatChannels(channels: number): string {
  const names: Record<number, string> = {
    1: "1.0",
    2: "2.0",
    6: "5.1",
    8: "7.1",
  }
  return names[channels] ?? `${channels} ch`
}

/** The CLI's bit depth normalisation (src/plan.rs). */
export function normalizeBitDepth(depth: number): 16 | 24 | 32 {
  if (depth === 16 || depth === 24 || depth === 32) return depth
  if (depth < 16) return 16
  if (depth > 16 && depth < 24) return 24
  return 32
}

export function formatSampleRate(hz: number): string {
  return `${Number((hz / 1000).toFixed(3))} kHz`
}

/** E.g. `5.1 - 48 kHz - 24-bit - 3 files`, or `mixed bit depth` in place of the depth. */
export function formatSummary(s: ProbeSummary): string {
  const parts: string[] = []
  if (s.channels !== null) parts.push(formatChannels(s.channels))
  if (s.sampleRate !== null) parts.push(formatSampleRate(s.sampleRate))
  if (s.mixedBitDepth) parts.push("mixed bit depth")
  else if (s.bitDepth !== null) parts.push(`${s.bitDepth}-bit`)
  parts.push(s.files === 1 ? "1 file" : `${s.files} files`)
  return parts.join(" - ")
}
