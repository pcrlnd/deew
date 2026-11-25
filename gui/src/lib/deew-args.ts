// Pure helpers that turn the GUI option state into a deew argv and into a
// shell-quoted command line for display. No Tauri / DOM dependencies.

import { DEFAULT_TEMPLATE, parseTemplate } from "@/lib/output-name"
import { shimBitratesDdp71 } from "@/lib/shim"

export const FORMATS = [
  "dd",
  "ddp",
  "ddp-atmos",
  "thd",
  "thd-atmos",
  "ac4",
  "ac4-atmos",
  "ac4-ims",
  "aac",
] as const
export type Format = (typeof FORMATS)[number]

/**
 * The format as it is spelled in the bitrate tables and in the config's
 * `[default_bitrates]` keys: underscores (`ddp_atmos`), unlike the `-f` value.
 */
export function tomlKey(format: Format): string {
  return format.replaceAll("-", "_")
}

/** Labels of the format dropdown. */
export const FORMAT_LABELS: Readonly<Record<Format, string>> = {
  dd: "DD",
  ddp: "DDP",
  "ddp-atmos": "DDP Atmos",
  thd: "TrueHD",
  "thd-atmos": "TrueHD Atmos",
  ac4: "AC4",
  "ac4-atmos": "AC4 Atmos",
  "ac4-ims": "AC-4 IMS",
  aac: "AAC",
}

/** Formats that are encoded from a TrueHD Atmos input with truehdd. */
export function isAtmos(format: Format): boolean {
  return (
    format === "ddp-atmos" || format === "ac4-atmos" || format === "thd-atmos"
  )
}

/** Whether `-dn` works: everything but AAC, AC4 Atmos and AC-4 IMS. */
export function supportsDialnorm(format: Format): boolean {
  return format !== "aac" && format !== "ac4-atmos" && format !== "ac4-ims"
}

/** Whether `-dm` works: not for AC-4 IMS (always immersive stereo). */
export function supportsDownmix(format: Format): boolean {
  return format !== "ac4-ims"
}

/** Whether `-r` has an effect (AAC has no DRC). */
export function supportsDrc(format: Format): boolean {
  return format !== "aac"
}

/** Whether `-r none` works (everything but AC4; AC4 Atmos accepts it). */
export function supportsDrcNone(format: Format): boolean {
  return format !== "ac4"
}

/**
 * The DRC the CLI uses when `-r` is not given: music_light for AC4 and for
 * AC-4 IMS in music mode, else none.
 */
export function defaultDrc(
  format: Format,
  imsMode: Ac4ImsMode = "general"
): Drc {
  return format === "ac4" || (format === "ac4-ims" && imsMode === "music")
    ? "music_light"
    : "none"
}

/** The DRC values listed for `format` (all of them; see `drcChoiceDisabled`). */
export function drcChoices(): Drc[] {
  return [...DRC_VALUES]
}

/**
 * Whether a DRC value is listed but not selectable for `format`: none for AC4,
 * everything but music_light for AC-4 IMS in music mode.
 */
export function drcChoiceDisabled(
  format: Format,
  d: Drc,
  imsMode: Ac4ImsMode = "general"
): boolean {
  if (format === "ac4-ims" && imsMode === "music") return d !== "music_light"
  return d === "none" && !supportsDrcNone(format)
}

export const DOWNMIX_VALUES = ["none", "1", "2", "6"] as const
export type Downmix = (typeof DOWNMIX_VALUES)[number]

export const DRC_VALUES = [
  "none",
  "film_light",
  "film_standard",
  "music_light",
  "music_standard",
  "speech",
] as const
export type Drc = (typeof DRC_VALUES)[number]

export const DEFAULT_BITRATE = "default"

/** `--ddp-mode` values (7.1 DDP). */
export const DDP_MODES = ["standard", "bluray"] as const
export type DdpMode = (typeof DDP_MODES)[number]

/** `--ddp-atmos-mode` values (DDP Atmos). */
export const DDP_ATMOS_MODES = ["streaming", "standard", "bluray"] as const
export type DdpAtmosMode = (typeof DDP_ATMOS_MODES)[number]

/**
 * A mode select value: `DEFAULT_MODE` is the mode of the config's
 * `[default_modes]` (nothing is passed to the CLI), or an explicit mode.
 */
export const DEFAULT_MODE = "default"
export type DdpModeChoice = typeof DEFAULT_MODE | DdpMode
export type DdpAtmosModeChoice = typeof DEFAULT_MODE | DdpAtmosMode

/** The config's `[default_modes]` (program defaults when unknown). */
export interface DefaultModes {
  ddp: DdpMode
  ddpAtmos: DdpAtmosMode
  ac4Ims: Ac4ImsMode
}

export const PROGRAM_DEFAULT_MODES: Readonly<DefaultModes> = {
  ddp: "standard",
  ddpAtmos: "streaming",
  ac4Ims: "general",
}

/** Modes from the settings answer; anything unknown is the program default. */
export function parseDefaultModes(
  ddp: unknown,
  ddpAtmos: unknown,
  ac4Ims: unknown
): DefaultModes {
  return {
    ddp: DDP_MODES.find((m) => m === ddp) ?? PROGRAM_DEFAULT_MODES.ddp,
    ddpAtmos:
      DDP_ATMOS_MODES.find((m) => m === ddpAtmos) ??
      PROGRAM_DEFAULT_MODES.ddpAtmos,
    ac4Ims:
      AC4_IMS_MODES.find((m) => m === ac4Ims) ?? PROGRAM_DEFAULT_MODES.ac4Ims,
  }
}

/** The mode a choice stands for. */
export function effectiveDdpMode(
  choice: DdpModeChoice,
  defaults: DefaultModes
): DdpMode {
  return choice === DEFAULT_MODE ? defaults.ddp : choice
}

export function effectiveDdpAtmosMode(
  choice: DdpAtmosModeChoice,
  defaults: DefaultModes
): DdpAtmosMode {
  return choice === DEFAULT_MODE ? defaults.ddpAtmos : choice
}

/** `--ac4-ims-mode` values (AC-4 IMS). */
export const AC4_IMS_MODES = ["general", "music"] as const
export type Ac4ImsMode = (typeof AC4_IMS_MODES)[number]

/** The mode select: the default is the config's (nothing is passed). */
export type Ac4ImsModeChoice = typeof DEFAULT_MODE | Ac4ImsMode

/** The mode a choice stands for. */
export function effectiveAc4ImsMode(
  choice: Ac4ImsModeChoice,
  defaults: DefaultModes
): Ac4ImsMode {
  return choice === DEFAULT_MODE ? defaults.ac4Ims : choice
}

/** Whether the "AC-4 IMS mode" select is enabled. */
export function ac4ImsModeApplies(format: Format): boolean {
  return format === "ac4-ims"
}

/** `-wm` values (truehdd's warp mode). */
export const WARP_MODES = ["normal", "warping", "prologiciix", "loro"] as const
export type WarpMode = (typeof WARP_MODES)[number]

/** The warp mode select: the default is the config's (nothing is passed). */
export type WarpModeChoice = typeof DEFAULT_MODE | WarpMode

/** The config's `truehdd_warp_mode`: empty leaves it to truehdd (the source's metadata). */
export type WarpModeConfig = WarpMode | ""

/**
 * Whether `-wm` and `-bc` are enabled: the formats that decode a TrueHD
 * Atmos source with truehdd, and measure only (any format). The CLI
 * rejects them for the other formats.
 */
export function truehddOptionsApply(
  format: Format,
  measureOnly: boolean
): boolean {
  return (
    measureOnly ||
    format === "ddp-atmos" ||
    format === "thd-atmos" ||
    format === "ac4-atmos" ||
    format === "ac4-ims"
  )
}

export interface DeewOptions {
  inputs: string[]
  /** `N`, `a:N` (the Nth audio track) or `s:N` (container stream N). */
  trackIndex: string
  output: string
  format: Format
  /** `DEFAULT_BITRATE` or a bitrate in kbps as text. */
  bitrate: string
  downmix: Downmix
  delay: string
  drc: Drc
  /** Integer text between -31 and 0. */
  dialnorm: string
  instances: string
  /** `-oi`: write each output next to its input; `output` is then unused. */
  outputToInput: boolean
  /** `-on`: the name of the output file, only for a single input file; empty is none. */
  outputName: string
  /** `-ot`: the name template; `null` (or empty) follows the config's `[output] template`. */
  outputTemplate: string | null
  /** `--ddp-mode`: profile of a 7.1 DDP output. */
  ddpMode: DdpModeChoice
  /** `--ddp-atmos-mode`: profile of DDP Atmos. */
  ddpAtmosMode: DdpAtmosModeChoice
  /** `--ac4-ims-mode`: profile of AC-4 IMS (general is the default). */
  ac4ImsMode: Ac4ImsModeChoice
  /** `-dl`: remove the LFE channel. */
  disableLfe: boolean
  /**
   * `-sps`: DEE's 90° surround phase shift (DD, DDP and DDP Atmos). `null`
   * follows the config's `surround_phase_shift`; the flag is only passed when
   * the value differs from the config (see `effectiveDdFilter`).
   */
  surroundPhaseShift: boolean | null
  /** `-lpf`: 120 Hz LFE low-pass filter (DD and DDP); `null` follows the config. */
  lowPassFilter: boolean | null
  /** `-s3a`: 3 dB attenuation of the surrounds (DD, DDP and TrueHD); `null` follows the config. */
  surround3dbAttenuation: boolean | null
  /** `-wm`: truehdd's warp mode (TrueHD Atmos sources); `DEFAULT_MODE` follows the config. */
  warpMode: WarpModeChoice
  /** `-bc`: truehdd's bed conformance (TrueHD Atmos sources); `null` follows the config. */
  truehddBedConform: boolean | null
  keepTemp: boolean
  keepLog: boolean
  measureOnly: boolean
  longArgument: boolean
  noPrompt: boolean
}

export const DEFAULT_OPTIONS: Readonly<DeewOptions> = {
  inputs: [],
  trackIndex: "0",
  output: "",
  format: "ddp",
  bitrate: DEFAULT_BITRATE,
  downmix: "none",
  delay: "",
  drc: "none",
  dialnorm: "0",
  instances: "",
  outputToInput: false,
  outputName: "",
  outputTemplate: null,
  ddpMode: DEFAULT_MODE,
  ddpAtmosMode: DEFAULT_MODE,
  ac4ImsMode: DEFAULT_MODE,
  disableLfe: false,
  surroundPhaseShift: null,
  lowPassFilter: null,
  surround3dbAttenuation: null,
  warpMode: DEFAULT_MODE,
  truehddBedConform: null,
  keepTemp: false,
  keepLog: false,
  measureOnly: false,
  longArgument: false,
  noPrompt: false,
}

/** The on/off options that are tied to the config's value. */
export type DdFilterOption =
  | "surroundPhaseShift"
  | "lowPassFilter"
  | "surround3dbAttenuation"
  | "truehddBedConform"

/**
 * The config's `surround_phase_shift`, `low_pass_filter`,
 * `surround_3db_attenuation`, `truehdd_bed_conform` and `truehdd_warp_mode`.
 */
export type DdFilterDefaults = Record<DdFilterOption, boolean> & {
  truehddWarpMode: WarpModeConfig
}

export const PROGRAM_DD_FILTER_DEFAULTS: Readonly<DdFilterDefaults> = {
  surroundPhaseShift: true,
  lowPassFilter: true,
  surround3dbAttenuation: false,
  truehddBedConform: false,
  truehddWarpMode: "",
}

/** The config's values from the settings answer; anything but a boolean gives the program default. */
export function ddFilterDefaultsOf(
  settings: Partial<Record<DdFilterOption, unknown>> & {
    truehddWarpMode?: unknown
  }
): DdFilterDefaults {
  const pick = (key: DdFilterOption) =>
    typeof settings[key] === "boolean"
      ? settings[key]
      : PROGRAM_DD_FILTER_DEFAULTS[key]
  return {
    surroundPhaseShift: pick("surroundPhaseShift"),
    lowPassFilter: pick("lowPassFilter"),
    surround3dbAttenuation: pick("surround3dbAttenuation"),
    truehddBedConform: pick("truehddBedConform"),
    truehddWarpMode:
      WARP_MODES.find((m) => m === settings.truehddWarpMode) ?? "",
  }
}

/** The state of a toggle: its own value, else the config's. */
export function effectiveDdFilter(
  o: DeewOptions,
  key: DdFilterOption,
  defaults: DdFilterDefaults = PROGRAM_DD_FILTER_DEFAULTS
): boolean {
  return o[key] ?? defaults[key]
}

export type BooleanOption =
  | "disableLfe"
  | "keepTemp"
  | "keepLog"
  | "measureOnly"
  | "longArgument"
  | "noPrompt"

/**
 * Flags of the one-off commands that need no input, in menu order. Commands
 * with the same `group` are shown together (separated from other groups).
 */
export const INFO_COMMANDS = [
  { flag: "-h", label: "Help", group: 0 },
  { flag: "-fh", label: "Full help", group: 0 },
  { flag: "-v", label: "Version", group: 0 },
  { flag: "-cl", label: "Changelog", group: 0 },
  { flag: "-lb", label: "List bitrates", group: 1 },
  { flag: "-pl", label: "Print logos", group: 1 },
  { flag: "-c", label: "Show config", group: 2 },
  { flag: "-gc", label: "Generate config", group: 2 },
  { flag: "-gt", label: "Download tools", group: 3 },
  { flag: "-u", label: "Update", group: 3 },
  { flag: "-u all", label: "Update and install all", group: 3 },
] as const
export type InfoFlag = (typeof INFO_COMMANDS)[number]["flag"]

export function infoArgs(flag: InfoFlag): string[] {
  // `flag` is also the id of the entry: "-u all" is two argv tokens.
  const args = flag.split(" ")
  // The dialog has asked already, so the CLI must not ask again.
  return flag === "-gc" || flag === "-u" || flag === "-u all"
    ? [...args, "-np"]
    : args
}

// Bitrate tables, copied from src/data.rs (a test keeps them in sync).
export const BITRATE_TABLE: Readonly<Record<string, readonly number[]>> = {
  dd_10: [96, 112, 128, 160, 192, 224, 256, 320, 384, 448, 512, 576, 640],
  dd_20: [96, 112, 128, 160, 192, 224, 256, 320, 384, 448, 512, 576, 640],
  dd_51: [224, 256, 320, 384, 448, 512, 576, 640],
  ddp_10: [
    32, 40, 48, 56, 64, 72, 80, 88, 96, 104, 112, 120, 128, 144, 160, 176, 192,
    200, 208, 216, 224, 232, 240, 248, 256, 272, 288, 304, 320, 336, 352, 368,
    384, 400, 448, 512, 576, 640, 704, 768, 832, 896, 960, 1008, 1024,
  ],
  ddp_20: [
    96, 104, 112, 120, 128, 144, 160, 176, 192, 200, 208, 216, 224, 232, 240,
    248, 256, 272, 288, 304, 320, 336, 352, 368, 384, 400, 448, 512, 576, 640,
    704, 768, 832, 896, 960, 1008, 1024,
  ],
  ddp_51: [
    192, 200, 208, 216, 224, 232, 240, 248, 256, 272, 288, 304, 320, 336, 352,
    368, 384, 400, 448, 512, 576, 640, 704, 768, 832, 896, 960, 1008, 1024,
  ],
  // 7.1 DDP: the standard profile (uses dee_shim) and the Blu-ray profile.
  ddp_71: [
    384, 448, 504, 576, 640, 704, 768, 832, 896, 960, 1008, 1024, 1152, 1280,
    1408, 1512, 1536, 1664, 1792,
  ],
  ddp_71_bluray: [768, 1024, 1280, 1536, 1664],
  // DDP Atmos: streaming (5.1), standard (7.1, E-AC-3 only) and Blu-ray (7.1).
  ddp_atmos_streaming: [384, 448, 576, 640, 768, 1024],
  ddp_atmos_standard: [768, 1024, 1152, 1280, 1408, 1536, 1664, 1792, 2048],
  ddp_atmos_bluray: [1152, 1280, 1408, 1512, 1536, 1664],
  ac4_20: [48, 64, 96, 128, 144, 192, 256, 288, 320, 384, 448, 512, 768],
  ac4_51: [96, 128, 144, 192, 256, 288, 320, 384, 448, 512, 768],
  ac4_atmos: [256, 320, 448, 768],
  ac4_ims: [64, 96, 128, 144, 256, 320],
  aac_20: [
    56, 64, 80, 96, 112, 128, 144, 160, 192, 224, 256, 288, 320, 384, 448, 512,
    576,
  ],
}

const FORMAT_BITRATE_KEYS: Readonly<Record<Format, readonly string[]>> = {
  dd: ["dd_10", "dd_20", "dd_51"],
  ddp: ["ddp_10", "ddp_20", "ddp_51", "ddp_71"],
  "ddp-atmos": [
    "ddp_atmos_streaming",
    "ddp_atmos_standard",
    "ddp_atmos_bluray",
  ],
  ac4: ["ac4_20", "ac4_51"],
  "ac4-atmos": ["ac4_atmos"],
  "ac4-ims": ["ac4_ims"],
  aac: ["aac_20"],
  thd: [],
  "thd-atmos": [],
}

/** Union of the allowed bitrates of every layout of `format`, ascending. */
export function bitrateOptions(format: Format): number[] {
  const all = new Set<number>()
  for (const key of FORMAT_BITRATE_KEYS[format]) {
    for (const b of BITRATE_TABLE[key]) all.add(b)
  }
  return [...all].sort((a, b) => a - b)
}

export interface BitrateContext {
  format: Format
  /** Channel count common to all probed inputs, `null` if unknown. */
  channels: number | null
  downmix: Downmix
  /** `--ddp-mode`; only has an effect on a 7.1 DDP output (standard if unset). */
  ddpMode?: DdpMode
  /** `--ddp-atmos-mode` (streaming if unset). */
  atmosMode?: DdpAtmosMode
  /**
   * The distinct ffprobe codec names of the probed inputs (empty if unknown),
   * for the Atmos formats that need TrueHD.
   */
  codecs?: readonly string[]
  /**
   * The distinct (codec, Atmos) kinds of the probed inputs; more precise than
   * `codecs` when given.
   */
  inputs?: readonly InputKind[]
  /** `-mo`: the format is ignored, so the Atmos master checks are too. */
  measureOnly?: boolean
}

export interface InputKind {
  codec: string
  atmos: boolean
  /** An Atmos master (ADM BWF / DAMF), `codec` is its label. */
  master?: boolean
}

export interface BitrateInfo {
  /**
   * `ok`: exact list for the probed layout; `fallback`: layout unknown, all
   * bitrates of the format; `na`: the format has no bitrate (TrueHD);
   * `invalid`: no encode possible, `bitrates` is empty.
   */
  status: "ok" | "fallback" | "na" | "invalid"
  bitrates: number[]
  /** Short text for the (disabled) select when there is no list. */
  placeholder?: string
  /** Problems the CLI would report for these inputs and options. */
  warnings: string[]
  /**
   * Key of the output layout in the config's `[default_bitrates]` (e.g.
   * `dd_5_1`); unset when the layout is unknown, TrueHD or invalid.
   */
  defaultKey?: string
  /** The output is 7.1 DDP, so `--ddp-mode` applies. */
  ddp71?: boolean
  /** Bitrates of `bitrates` that only work with dee_shim. */
  shimBitrates?: number[]
}

export const ATMOS_INPUT_WARNING =
  "The Atmos formats need a TrueHD Atmos input."
export const DDP_ATMOS_INPUT_WARNING =
  "DDP Atmos input: the Atmos formats need a TrueHD Atmos input."
export const THD_NO_ATMOS_INPUT_WARNING =
  "TrueHD input without Atmos: the Atmos formats need a TrueHD Atmos input."

/** One warning per kind of unsuitable input for the Atmos formats. */
export function atmosInputWarnings(
  allInputs: readonly InputKind[] | undefined,
  codecs: readonly string[] | undefined
): string[] {
  // Atmos masters are Atmos sources of their own, they are never a problem.
  const inputs = allInputs?.filter((i) => i.master !== true)
  if (inputs === undefined) {
    return codecs?.some((codec) => codec !== "truehd")
      ? [ATMOS_INPUT_WARNING]
      : []
  }
  const warnings: string[] = []
  if (inputs.some((i) => i.codec === "eac3" && i.atmos)) {
    warnings.push(DDP_ATMOS_INPUT_WARNING)
  }
  if (inputs.some((i) => i.codec === "truehd" && !i.atmos)) {
    warnings.push(THD_NO_ATMOS_INPUT_WARNING)
  }
  if (
    inputs.some((i) => i.codec !== "truehd" && !(i.codec === "eac3" && i.atmos))
  ) {
    warnings.push(ATMOS_INPUT_WARNING)
  }
  return warnings
}

export const IMS_INPUT_WARNING =
  "AC-4 IMS needs a 5.1 (or 5.0, 7.0, 7.1) or an Atmos input, stereo and mono can't be encoded to immersive stereo."
export const IMS_MIXED_WARNING =
  "AC-4 IMS can't encode Atmos inputs (TrueHD Atmos, ADM BWF, DAMF) and channel-based inputs in the same run."
export const MASTER_MIXED_WARNING =
  "Atmos masters (ADM BWF, DAMF) can't be encoded to DD/DDP together with other inputs."

/** Whether the source of AC-4 IMS is Atmos: a master or TrueHD Atmos. */
export function imsAtmosSource(i: InputKind): boolean {
  return i.master === true || (i.codec === "truehd" && i.atmos)
}

/** The formats whose encoder reads an Atmos master (DD/DDP render a 5.1). */
export function takesMaster(format: Format): boolean {
  return format !== "thd" && format !== "ac4" && format !== "aac"
}

/** Problems of Atmos masters (ADM BWF, DAMF) with the format. */
export function masterWarnings(c: BitrateContext): string[] {
  const masters = c.inputs?.filter((i) => i.master === true).length ?? 0
  if (masters === 0 || c.measureOnly === true) return []
  if (!takesMaster(c.format)) {
    return [
      `Atmos masters (ADM BWF, DAMF) can't be encoded to ${FORMAT_LABELS[c.format]}, use DDP Atmos, TrueHD Atmos, AC4 Atmos or AC-4 IMS (or DD/DDP for a 5.1 render).`,
    ]
  }
  if (
    (c.format === "dd" || c.format === "ddp") &&
    masters !== c.inputs?.length
  ) {
    return [MASTER_MIXED_WARNING]
  }
  return []
}

/** Problems of the inputs with AC-4 IMS (`Ac4ImsInput`, `Ac4ImsMixed`). */
function imsInputWarnings(c: BitrateContext): string[] {
  const atmos = c.inputs?.filter(imsAtmosSource).length ?? 0
  const total = c.inputs?.length ?? 0
  if (atmos > 0 && atmos < total) return [IMS_MIXED_WARNING]
  if (atmos === 0 && c.channels !== null) {
    if (c.channels <= 2) return [IMS_INPUT_WARNING]
    if (![5, 6, 7, 8].includes(c.channels)) return [CHANNELS_WARNING]
  }
  return []
}

export const CHANNELS_WARNING =
  "Input channels can only be 1, 2, 5, 6, 7 or 8 (1.0, 2.0, 5.0, 5.1, 7.0 or 7.1)."

/**
 * The bitrates deew accepts for the probed channel layout, mirroring
 * `resolve_layout` and `select_bitrate` of src/plan.rs.
 */
export function relevantBitrates(c: BitrateContext): BitrateInfo {
  const info = layoutBitrates(c)
  const extra = masterWarnings(c)
  return extra.length === 0
    ? info
    : { ...info, warnings: [...extra, ...info.warnings] }
}

function layoutBitrates(c: BitrateContext): BitrateInfo {
  const warnings: string[] = []
  if (c.format === "ac4-ims") {
    // Always immersive stereo, from the 5.1 of the source or an Atmos input.
    warnings.push(...imsInputWarnings(c))
    return {
      status: "ok",
      bitrates: [...BITRATE_TABLE.ac4_ims],
      warnings,
      defaultKey: "ac4_ims",
      shimBitrates: [],
    }
  }
  if (isAtmos(c.format)) {
    // Atmos inputs have no usable channel layout; the encode needs TrueHD.
    warnings.push(...atmosInputWarnings(c.inputs, c.codecs))
    if (c.downmix !== "none") {
      warnings.push("Downmix is not supported for Atmos formats.")
    }
    if (c.format === "thd-atmos") {
      // Lossless, like TrueHD: no bitrate.
      return {
        status: "na",
        bitrates: [],
        placeholder: "N/A for TrueHD",
        warnings,
      }
    }
    const key =
      c.format === "ddp-atmos"
        ? `ddp_atmos_${c.atmosMode ?? PROGRAM_DEFAULT_MODES.ddpAtmos}`
        : tomlKey(c.format)
    const bitrates = [...BITRATE_TABLE[key]]
    return {
      status: "ok",
      bitrates,
      warnings,
      defaultKey: key,
      // Every DDP Atmos bitrate goes through dee_shim; AC4 Atmos does not.
      shimBitrates: c.format === "ddp-atmos" ? [...bitrates] : [],
    }
  }
  if (c.format === "thd") {
    if (c.channels === 1) {
      warnings.push("Input channels for TrueHD can only be 2, 6 or 8.")
    }
    // TrueHD is downmixed by ffmpeg, to 2.0 or 5.1 (it has no mono).
    if (c.downmix !== "none") {
      if (c.channels !== null && Number(c.downmix) >= c.channels) {
        warnings.push("Downmix has to be lower than the input channels.")
      } else if (!downmixValues("thd").includes(c.downmix)) {
        warnings.push(`Downmix ${c.downmix} is not supported for this format.`)
      }
    }
    return {
      status: "na",
      bitrates: [],
      placeholder: "N/A for TrueHD",
      warnings,
    }
  }
  if (c.channels === null) {
    return {
      status: "fallback",
      bitrates: bitrateOptions(c.format),
      warnings,
    }
  }
  if (![1, 2, 5, 6, 7, 8].includes(c.channels)) {
    return {
      status: "invalid",
      bitrates: [],
      placeholder: "unsupported input",
      warnings: [CHANNELS_WARNING],
    }
  }
  if (c.format === "ac4" && c.channels === 1) {
    return {
      status: "invalid",
      bitrates: [],
      placeholder: "AC4 needs 2.0+ input",
      warnings: ["Input channels for AC4 can only be 2, 5, 6, 7 or 8."],
    }
  }
  if (c.format === "aac" && c.channels === 1) {
    return {
      status: "invalid",
      bitrates: [],
      placeholder: "AAC needs 2.0+ input",
      warnings: ["Input channels for AAC can only be 2, 5, 6, 7 or 8."],
    }
  }
  const downmix = c.downmix === "none" ? null : Number(c.downmix)
  // 5.0 / 7.0 are treated like 5.1 / 7.1 for the output layout and rates.
  const layoutChannels =
    ({ 5: 6, 7: 8 } as Record<number, number>)[c.channels] ?? c.channels
  let outchannels = layoutChannels
  if (downmix !== null) {
    if (downmix >= c.channels) {
      warnings.push("Downmix has to be lower than the input channels.")
    } else if (!downmixValues(c.format).includes(c.downmix)) {
      warnings.push(`Downmix ${c.downmix} is not supported for this format.`)
    } else {
      outchannels = downmix
    }
  }
  if (downmix === null || outchannels === layoutChannels) {
    // No (usable) downmix: DD and AC4 reduce 7.1 to 5.1 themselves, and AAC
    // is always stereo.
    if ((c.format === "dd" || c.format === "ac4") && layoutChannels === 8) {
      outchannels = 6
    } else if (c.format === "aac") {
      outchannels = 2
    }
  }
  const ddp71 = c.format === "ddp" && outchannels === 8
  const bluray = c.ddpMode === "bluray"
  let key: string
  let defaultKey: string
  if (ddp71) {
    key = bluray ? "ddp_71_bluray" : "ddp_71"
    defaultKey = bluray ? "ddp_7_1_bluray" : "ddp_7_1_standard"
  } else {
    const layout = { 1: "10", 2: "20", 6: "51" }[outchannels as 1 | 2 | 6]
    key = `${tomlKey(c.format)}_${layout}`
    const defaultLayout = { 1: "1_0", 2: "2_0", 6: "5_1" }[
      outchannels as 1 | 2 | 6
    ]
    defaultKey = `${tomlKey(c.format)}_${defaultLayout}`
  }
  return {
    status: "ok",
    bitrates: [...BITRATE_TABLE[key]],
    warnings,
    defaultKey,
    ddp71,
    shimBitrates: ddp71 ? shimBitratesDdp71(bluray) : [],
  }
}

/** Built-in `[default_bitrates]` of deew's config template. */
export const TEMPLATE_DEFAULT_BITRATES: Readonly<Record<string, number>> = {
  dd_1_0: 128,
  dd_2_0: 256,
  dd_5_1: 640,
  ddp_1_0: 128,
  ddp_2_0: 256,
  ddp_5_1: 1024,
  ddp_7_1_standard: 1536,
  ddp_7_1_bluray: 1536,
  ddp_atmos_streaming: 1024,
  ddp_atmos_standard: 1536,
  ddp_atmos_bluray: 1536,
  ac4_2_0: 256,
  ac4_5_1: 448,
  ac4_atmos: 448,
  ac4_ims: 256,
  aac_2_0: 256,
}

/** `find_closest_allowed` of src/util.rs: the first closest value wins. */
export function closestAllowed(value: number, allowed: readonly number[]) {
  let best: number | undefined
  for (const a of allowed) {
    if (best === undefined || Math.abs(a - value) < Math.abs(best - value)) {
      best = a
    }
  }
  return best
}

/**
 * The bitrate the CLI picks for the default entry (the config's, else the
 * program's, moved to the closest allowed one); `null` when there is none.
 */
export function defaultBitrate(
  info: BitrateInfo,
  configDefaults: Readonly<Record<string, number>>
): number | null {
  if (info.status !== "ok" && info.status !== "fallback") return null
  if (!info.defaultKey) return null
  const wanted =
    configDefaults[info.defaultKey] ??
    TEMPLATE_DEFAULT_BITRATES[info.defaultKey]
  return wanted === undefined
    ? null
    : (closestAllowed(wanted, info.bitrates) ?? null)
}

export interface BitrateChoice {
  value: string
  label: string
  /** Only possible with dee_shim. */
  shim?: boolean
}

/**
 * The entries of the bitrate select. The default entry (value
 * `DEFAULT_BITRATE`) sits at the position of the bitrate the CLI would pick.
 */
export function bitrateChoices(
  info: BitrateInfo,
  configDefaults: Readonly<Record<string, number>>
): BitrateChoice[] {
  const hasList = info.status === "ok" || info.status === "fallback"
  const fromConfig = info.defaultKey
    ? configDefaults[info.defaultKey]
    : undefined
  const fallbackLabel =
    fromConfig === undefined
      ? "default (from config)"
      : `default (${fromConfig} from config)`
  if (!hasList) {
    return [
      { value: DEFAULT_BITRATE, label: info.placeholder ?? fallbackLabel },
    ]
  }
  const wanted = info.defaultKey
    ? (fromConfig ?? TEMPLATE_DEFAULT_BITRATES[info.defaultKey])
    : undefined
  const picked =
    wanted === undefined ? undefined : closestAllowed(wanted, info.bitrates)
  if (picked === undefined) {
    return [
      { value: DEFAULT_BITRATE, label: fallbackLabel },
      ...info.bitrates.map((b) => ({ value: String(b), label: `${b} kbps` })),
    ]
  }
  const suffix = fromConfig === undefined ? "default" : "default from config"
  const shim = (b: number) => info.shimBitrates?.includes(b) === true
  return info.bitrates.map((b) =>
    b === picked
      ? {
          value: DEFAULT_BITRATE,
          label: `${b} kbps (${suffix})`,
          shim: shim(b),
        }
      : { value: String(b), label: `${b} kbps`, shim: shim(b) }
  )
}

/** The `-dm` values a format accepts (besides keeping the layout). */
function downmixValues(format: Format): readonly Downmix[] {
  switch (format) {
    case "dd":
    case "ddp":
      return ["1", "2", "6"]
    case "ac4":
      return ["2", "6"]
    case "aac":
      return ["2"]
    case "thd":
      return ["2", "6"]
    default:
      return []
  }
}

/**
 * The downmix choices that make sense for the format and the probed input
 * channel count (a downmix has to be lower than the input); all the format
 * accepts when the channels are unknown.
 */
export function relevantDownmixes(
  format: Format,
  channels: number | null
): Downmix[] {
  return [
    "none" as const,
    ...downmixValues(format).filter(
      (d) => channels === null || Number(d) < channels
    ),
  ]
}

/**
 * Whether the "DDP mode" select is shown: DDP, and the output is 7.1 or not
 * known yet (no probed input). A known 5.1 or smaller output hides it.
 */
export function ddpModeApplies(format: Format, info: BitrateInfo): boolean {
  return format === "ddp" && (info.ddp71 === true || info.status === "fallback")
}

/** Whether the "DDP Atmos mode" select is shown. */
export function ddpAtmosModeApplies(format: Format): boolean {
  return format === "ddp-atmos"
}

/**
 * Whether `-dl` is enabled: a probed 5.1 or 7.1 input, not an Atmos format,
 * not AAC (always 2.0), not AC-4 IMS (needs the LFE), and no downmix to mono or stereo.
 */
export function disableLfeApplies(
  format: Format,
  channels: number | null | undefined,
  downmix: Downmix
): boolean {
  return (
    !isAtmos(format) &&
    format !== "aac" &&
    format !== "ac4-ims" &&
    (channels === 6 || channels === 8) &&
    downmix !== "1" &&
    downmix !== "2"
  )
}

/** Whether `-sps` is enabled: DD, DDP and DDP Atmos (Atmos via dee_shim `--phase90`). */
export function surroundPhaseShiftApplies(format: Format): boolean {
  return format === "dd" || format === "ddp" || format === "ddp-atmos"
}

/** Whether `-lpf` is enabled: DD and DDP only. */
export function lowPassFilterApplies(format: Format): boolean {
  return format === "dd" || format === "ddp"
}

/**
 * Whether `-s3a` is enabled: DD, DDP and TrueHD (the CLI asks first for
 * TrueHD, the output isn't identical to the source).
 */
export function surround3dbAttenuationApplies(format: Format): boolean {
  return format === "dd" || format === "ddp" || format === "thd"
}

/** Whether the toggle can be used with the format (`-bc` also with measure only). */
export function ddFilterApplies(
  key: DdFilterOption,
  format: Format,
  measureOnly = false
): boolean {
  switch (key) {
    case "truehddBedConform":
      return truehddOptionsApply(format, measureOnly)
    case "surroundPhaseShift":
      return surroundPhaseShiftApplies(format)
    case "lowPassFilter":
      return lowPassFilterApplies(format)
    case "surround3dbAttenuation":
      return surround3dbAttenuationApplies(format)
  }
}

const DD_FILTER_FLAGS: [DdFilterOption, string][] = [
  ["surroundPhaseShift", "-sps"],
  ["lowPassFilter", "-lpf"],
  ["surround3dbAttenuation", "-s3a"],
  ["truehddBedConform", "-bc"],
]

/** Changes the format and resets what the new format cannot do. */
export function setFormat(
  o: DeewOptions,
  format: Format,
  defaults: DefaultModes = PROGRAM_DEFAULT_MODES
): DeewOptions {
  const bitrate =
    o.bitrate !== DEFAULT_BITRATE &&
    !bitrateOptions(format).includes(Number(o.bitrate))
      ? DEFAULT_BITRATE
      : o.bitrate
  return {
    ...o,
    format,
    bitrate,
    downmix: downmixValues(format).includes(o.downmix) ? o.downmix : "none",
    drc:
      (o.drc === "none" && !supportsDrcNone(format)) ||
      drcChoiceDisabled(
        format,
        o.drc,
        effectiveAc4ImsMode(o.ac4ImsMode, defaults)
      )
        ? "music_light"
        : o.drc,
    dialnorm: supportsDialnorm(format) ? o.dialnorm : "0",
  }
}

/** Sets `--ac4-ims-mode`; music mode needs the DRC music_light. */
export function setAc4ImsMode(
  o: DeewOptions,
  choice: Ac4ImsModeChoice,
  defaults: DefaultModes = PROGRAM_DEFAULT_MODES
): DeewOptions {
  return {
    ...o,
    ac4ImsMode: choice,
    drc:
      effectiveAc4ImsMode(choice, defaults) === "music" ? "music_light" : o.drc,
  }
}

/** Sets a checkbox. */
export function setFlag(
  o: DeewOptions,
  key: BooleanOption,
  value: boolean
): DeewOptions {
  return { ...o, [key]: value }
}

const INT_RE = /^[+-]?\d+$/

function parseInteger(text: string): number | null {
  const t = text.trim()
  return INT_RE.test(t) ? Number.parseInt(t, 10) : null
}

const TRACK_RE = /^(?:([as]):)?(\d+)$/

/**
 * The `-ti` text as the CLI takes it: `N` (the same as `a:N`, the Nth audio
 * track) or `a:N` / `s:N` (container stream N). `null` if it is none of them.
 */
export function normalizeTrackIndex(text: string): string | null {
  const m = TRACK_RE.exec(text.trim().toLowerCase())
  if (m === null) return null
  const n = Number.parseInt(m[2], 10)
  if (!Number.isSafeInteger(n)) return null
  return m[1] === undefined ? String(n) : `${m[1]}:${n}`
}

export const TRACK_INDEX_HINT = "0, a:1 or s:3"

/** What decides the output name that is passed: the config and the file count. */
export interface OutputNaming {
  /** The config's `[output] template`. */
  configTemplate: string
  /** Exactly one input file (after expanding folders): `-on` can be used. */
  singleFile: boolean
}

export const PROGRAM_OUTPUT_NAMING: Readonly<OutputNaming> = {
  configTemplate: DEFAULT_TEMPLATE,
  singleFile: false,
}

/** The name or the template that is passed, `null` when neither is. */
export function outputNameArg(
  o: DeewOptions,
  naming: OutputNaming
): { flag: "-on" | "-ot"; value: string } | null {
  if (o.measureOnly) return null
  const name = o.outputName.trim()
  // the name wins, and a template is never passed with it
  if (naming.singleFile && name !== "") return { flag: "-on", value: name }
  const template = o.outputTemplate
  if (
    template !== null &&
    template.trim() !== "" &&
    template !== naming.configTemplate
  ) {
    return { flag: "-ot", value: template }
  }
  return null
}

/** Human readable problems; Run is disabled while there are any. */
export function validate(
  o: DeewOptions,
  defaults: DefaultModes = PROGRAM_DEFAULT_MODES,
  naming: OutputNaming = PROGRAM_OUTPUT_NAMING
): string[] {
  const problems: string[] = []
  if (normalizeTrackIndex(o.trackIndex) === null) {
    problems.push(`Track index must be like ${TRACK_INDEX_HINT}.`)
  }
  // -mo measures the source: the format and the encoding options are ignored.
  if (o.measureOnly) return problems
  const named = outputNameArg(o, naming)
  if (named?.flag === "-ot") {
    const parsed = parseTemplate(named.value)
    if ("error" in parsed) problems.push(`Output template: ${parsed.error}.`)
  }
  const dn = parseInteger(o.dialnorm)
  if (dn === null || dn < -31 || dn > 0) {
    problems.push("Dialnorm must be between -31 and 0.")
  } else if (dn !== 0 && !supportsDialnorm(o.format)) {
    problems.push("Dialnorm doesn't work for AAC, AC4 Atmos and AC-4 IMS.")
  }
  if (o.drc === "none" && !supportsDrcNone(o.format)) {
    problems.push("DRC none is not supported for AC4.")
  }
  if (
    o.format === "ac4-ims" &&
    effectiveAc4ImsMode(o.ac4ImsMode, defaults) === "music" &&
    o.drc !== "music_light"
  ) {
    problems.push("AC-4 IMS music mode needs the DRC music_light.")
  }
  return problems
}

/**
 * Builds the deew arguments (without the program name). Only options that
 * differ from the defaults are included. The delay always uses the `-d=`
 * form, since the CLI cannot take a negative value as a separate argument.
 */
export function buildArgs(
  o: DeewOptions,
  layout: {
    ddp71?: boolean
    channels?: number | null
    /** The config's `[default_modes]`; a mode equal to it is not passed. */
    defaultModes?: DefaultModes
    /** The config's on/off values; a toggle equal to it is not passed. */
    ddFilterDefaults?: DdFilterDefaults
    /** The config's output template and the file count, see `outputNameArg`. */
    naming?: OutputNaming
  } = {}
): string[] {
  const defaultModes = layout.defaultModes ?? PROGRAM_DEFAULT_MODES
  const args: string[] = []
  if (o.inputs.length > 0) args.push("-i", ...o.inputs)

  const ti = normalizeTrackIndex(o.trackIndex)
  if (ti !== null && ti !== "0" && ti !== "a:0") args.push("-ti", ti)
  // -oi overrides -o, so the (greyed out) directory is not passed with it.
  if (o.outputToInput) args.push("-oi")
  else if (o.output.trim() !== "") args.push("-o", o.output.trim())
  const named = outputNameArg(o, layout.naming ?? PROGRAM_OUTPUT_NAMING)
  if (named !== null) args.push(named.flag, named.value)
  if (o.format !== DEFAULT_OPTIONS.format) args.push("-f", o.format)
  if (
    o.format === "ddp" &&
    layout.ddp71 === true &&
    o.ddpMode !== DEFAULT_MODE &&
    o.ddpMode !== defaultModes.ddp
  ) {
    args.push("--ddp-mode", o.ddpMode)
  }
  if (
    ddpAtmosModeApplies(o.format) &&
    o.ddpAtmosMode !== DEFAULT_MODE &&
    o.ddpAtmosMode !== defaultModes.ddpAtmos
  ) {
    args.push("--ddp-atmos-mode", o.ddpAtmosMode)
  }
  if (
    ac4ImsModeApplies(o.format) &&
    o.ac4ImsMode !== DEFAULT_MODE &&
    o.ac4ImsMode !== defaultModes.ac4Ims
  ) {
    args.push("--ac4-ims-mode", o.ac4ImsMode)
  }
  if (o.bitrate !== DEFAULT_BITRATE && o.bitrate.trim() !== "") {
    args.push("-b", o.bitrate.trim())
  }
  if (o.downmix !== "none" && supportsDownmix(o.format)) {
    args.push("-dm", o.downmix)
  }
  if (o.delay.trim() !== "") args.push(`-d=${o.delay.trim()}`)
  const imsMode = effectiveAc4ImsMode(o.ac4ImsMode, defaultModes)
  if (supportsDrc(o.format) && o.drc !== defaultDrc(o.format, imsMode)) {
    args.push("-r", o.drc)
  }
  const dn = parseInteger(o.dialnorm)
  if (dn !== null && dn !== 0 && supportsDialnorm(o.format)) {
    args.push("-dn", String(dn))
  }
  if (o.instances.trim() !== "") args.push("-in", o.instances.trim())

  if (o.disableLfe && disableLfeApplies(o.format, layout.channels, o.downmix)) {
    args.push("-dl")
  }
  const filterDefaults = layout.ddFilterDefaults ?? PROGRAM_DD_FILTER_DEFAULTS
  for (const [key, flag] of DD_FILTER_FLAGS) {
    const value = o[key]
    if (
      value !== null &&
      value !== filterDefaults[key] &&
      ddFilterApplies(key, o.format, o.measureOnly)
    ) {
      args.push(flag, value ? "on" : "off")
    }
  }
  if (
    truehddOptionsApply(o.format, o.measureOnly) &&
    o.warpMode !== DEFAULT_MODE &&
    o.warpMode !== filterDefaults.truehddWarpMode
  ) {
    args.push("-wm", o.warpMode)
  }
  if (o.keepTemp) args.push("-k")
  if (o.keepLog) args.push("-kl")
  if (o.measureOnly) args.push("-mo")
  if (o.longArgument) args.push("-la")
  if (o.noPrompt) args.push("-np")
  return args
}

export type Platform = "posix" | "windows"

export function detectPlatform(userAgent: string): Platform {
  return /windows/i.test(userAgent) ? "windows" : "posix"
}

const POSIX_SAFE = /^[A-Za-z0-9_@%+=:,./-]+$/
const WINDOWS_SAFE = /^[A-Za-z0-9_@+=:,./\\-]+$/

/** Quotes one argument for display in a POSIX shell or on Windows. */
export function quoteArg(arg: string, platform: Platform): string {
  if (platform === "posix") {
    if (POSIX_SAFE.test(arg)) return arg
    return `'${arg.replaceAll("'", `'\\''`)}'`
  }
  if (WINDOWS_SAFE.test(arg)) return arg
  // CommandLineToArgvW rules: double the backslashes in front of a quote
  // (or of the closing quote) and escape the quote itself.
  let out = '"'
  let backslashes = 0
  for (const ch of arg) {
    if (ch === "\\") {
      backslashes++
    } else if (ch === '"') {
      out += "\\".repeat(backslashes * 2 + 1) + '"'
      backslashes = 0
    } else {
      out += "\\".repeat(backslashes) + ch
      backslashes = 0
    }
  }
  return out + "\\".repeat(backslashes * 2) + '"'
}

export function formatCommand(
  args: readonly string[],
  platform: Platform,
  program = "deew"
): string {
  return [program, ...args.map((a) => quoteArg(a, platform))].join(" ")
}
