// Types and pure helpers for the input probing (see probe_inputs in
// src-tauri/src/lib.rs). No Tauri / DOM dependencies.

/** Result for one input file; either the properties or `error`. */
export interface FileProbe {
  path: string
  channels: number | null
  sampleRate: number | null
  /** Raw bit depth, as the CLI compares it. */
  bitDepth: number | null
  /** ffprobe's `codec_name`, e.g. `truehd`. */
  codec: string | null
  /** ffprobe's stream profile mentions Dolby Atmos. */
  atmos: boolean
  /** A DTS:X track (its `channels` are those of the bed); missing in the output of an older CLI. */
  dtsx?: boolean
  /** An Auro-3D track, `layout` `null` when unknown; `null` or missing otherwise (also for an older CLI). */
  auro?: { layout: string | null; carrier: string | null } | null
  /** The `language` tag of the selected track; missing or `null` without one. */
  language?: string | null
  /** The `title` tag of the selected track; missing or `null` without one. */
  title?: string | null
  /** An Atmos master (ADM BWF / DAMF); `codec` is its label. */
  master: boolean
  error: string | null
  /** A file of a folder that isn't audio: left out, not an error. */
  skipped: boolean
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
  /** The distinct (normalised) bit depths, sorted; more than one if mixed. */
  bitDepths: number[]
  /** The distinct codec names of the probed files (not the masters), sorted. */
  codecs: string[]
  /** The distinct labels of the Atmos masters, sorted. */
  masters: string[]
  /** Every probed file is an Atmos master (and there is at least one). */
  onlyMasters: boolean
  /** Every probed file is Atmos (and there is at least one). */
  atmos: boolean
  /** Every probed file is DTS:X (and there is at least one). */
  dtsx: boolean
  /** Every probed file is Auro-3D with the same layout and carrier (and there is at least one). */
  auro: { layout: string | null; carrier: string | null } | null
  /** The distinct (codec, Atmos, DTS:X or Auro-3D) kinds of the probed files. */
  inputs: { codec: string; atmos: boolean; master: boolean; spatial: boolean }[]
  /** The probed files that can be encoded, for their output names. */
  entries: {
    path: string
    channels: number | null
    language: string | null
    title: string | null
    master: boolean
  }[]
  /** Like the CLI's mismatch errors. */
  mismatches: string[]
  errors: { path: string; error: string }[]
  /** Files of folders that aren't audio; not counted anywhere else. */
  skipped: { path: string; error: string }[]
}

function unique(values: number[]): number[] {
  return [...new Set(values)]
}

/** Python style list, as printed by the CLI: `[6, 2]`. */
function intList(values: number[]): string {
  return `[${values.join(", ")}]`
}

function distinctInputs(
  ok: readonly FileProbe[]
): { codec: string; atmos: boolean; master: boolean; spatial: boolean }[] {
  const seen = new Map<
    string,
    { codec: string; atmos: boolean; master: boolean; spatial: boolean }
  >()
  for (const f of ok) {
    if (f.codec === null) continue
    const spatial = f.dtsx === true || fileAuro(f) !== null
    seen.set(`${f.codec}/${f.atmos}/${f.master}/${spatial}`, {
      codec: f.codec,
      atmos: f.atmos,
      master: f.master,
      spatial,
    })
  }
  return [...seen.values()].sort(
    (a, b) =>
      a.codec.localeCompare(b.codec) || Number(a.atmos) - Number(b.atmos)
  )
}

/** The `auro` of a file, read defensively (the output of an older CLI has none). */
function fileAuro(
  f: FileProbe
): { layout: string | null; carrier: string | null } | null {
  const a = f.auro as unknown
  if (typeof a !== "object" || a === null) return null
  const { layout, carrier } = a as Record<string, unknown>
  return {
    layout: typeof layout === "string" && layout !== "" ? layout : null,
    carrier: typeof carrier === "string" && carrier !== "" ? carrier : null,
  }
}

/** The Auro-3D of all the files when it is the same for each, else `null`. */
function commonAuro(
  ok: readonly FileProbe[]
): { layout: string | null; carrier: string | null } | null {
  const all = ok.map(fileAuro)
  const first = all[0]
  if (!first) return null
  return all.every(
    (a) =>
      a !== null && a.layout === first.layout && a.carrier === first.carrier
  )
    ? first
    : null
}

/** `Auro-3D 13.1 (carried in 7.1)`, without what is not known. */
export function formatAuro(a: {
  layout: string | null
  carrier: string | null
}): string {
  return `Auro-3D${a.layout ? ` ${a.layout}` : ""}${
    a.carrier ? ` (carried in ${a.carrier})` : ""
  }`
}

/** Condenses the per-file results into what the CLI would derive from them. */
export function summarizeProbe(all: readonly FileProbe[]): ProbeSummary {
  const skipped = all
    .filter((f) => f.skipped)
    .map((f) => ({ path: f.path, error: f.error ?? "" }))
  const files = all.filter((f) => !f.skipped)
  const errors = files
    .filter((f) => f.error !== null)
    .map((f) => ({ path: f.path, error: f.error ?? "" }))
  const ok = files.filter((f) => f.error === null)
  // Like the CLI: a master has no say in the rate and channels of the others
  // (it counts as 5.1 at 48 kHz only when every input is a master).
  const real = ok.filter((f) => !f.master)
  const onlyMasters = ok.length > 0 && real.length === 0
  const pick = (get: (f: FileProbe) => number | null): number[] =>
    (onlyMasters ? ok : real).map(get).filter((v): v is number => v !== null)

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

  const common = (values: number[]): number | null =>
    unique(values).length === 1 ? values[0] : null
  return {
    files: files.length,
    // Without every file probed the layout is not known.
    channels: errors.length === 0 ? common(channels) : null,
    sampleRate: common(rates),
    // Compared after the CLI's normalisation.
    bitDepths: unique(depths.map(normalizeBitDepth)).sort((a, b) => a - b),
    masters: [
      ...new Set(
        ok
          .filter((f) => f.master)
          .map((f) => f.codec)
          .filter((c): c is string => c !== null)
      ),
    ].sort(),
    onlyMasters,
    codecs: [
      ...new Set(
        real.map((f) => f.codec).filter((c): c is string => c !== null)
      ),
    ].sort(),
    atmos: ok.length > 0 && ok.every((f) => f.atmos),
    dtsx: ok.length > 0 && ok.every((f) => f.dtsx === true),
    auro: commonAuro(ok),
    inputs: distinctInputs(ok),
    entries: ok.map((f) => ({
      path: f.path,
      channels: f.channels,
      language: f.language ?? null,
      title: f.title ?? null,
      master: f.master,
    })),
    mismatches,
    errors,
    skipped,
  }
}

/** `1.0`, `2.0`, `5.1`, `7.1`; other counts as `N ch`. */
export function formatChannels(channels: number): string {
  const names: Record<number, string> = {
    1: "1.0",
    2: "2.0",
    5: "5.0",
    6: "5.1",
    7: "7.0",
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

/** E.g. `Auro-3D 13.1 (carried in 7.1) - 48 kHz - 24-bit - 1 file`, `Atmos 7.1 - 48 kHz - 24-bit - 1 file`, `5.1 - 48 kHz - 24-bit - 3 files`, or `16/24/32-bit` for mixed bit depths. */
export function formatSummary(s: ProbeSummary): string {
  const parts: string[] = []
  if (s.onlyMasters) {
    // A master has no channel layout of its own to show.
    parts.push(s.masters.join(" + "))
    parts.push(s.files === 1 ? "1 file" : `${s.files} files`)
    return parts.join(" - ")
  }
  if (s.channels !== null) {
    const channels = formatChannels(s.channels)
    parts.push(
      s.atmos
        ? `Atmos ${channels}`
        : s.auro
          ? formatAuro(s.auro)
          : s.dtsx
            ? `DTS:X ${channels}`
            : channels
    )
  }
  if (s.sampleRate !== null) parts.push(formatSampleRate(s.sampleRate))
  if (s.bitDepths.length > 0) parts.push(`${s.bitDepths.join("/")}-bit`)
  parts.push(s.files === 1 ? "1 file" : `${s.files} files`)
  if (s.masters.length > 0) parts.push(`with ${s.masters.join(" + ")}`)
  return parts.join(" - ")
}
