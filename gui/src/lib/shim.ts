// What only works because of dee_shim (formats, DRC none, DDP modes and
// bitrates). The GUI marks these values with a puzzle icon. Pure data and helpers, no DOM dependencies.

import type { DdpAtmosMode, DdpMode, Format } from "@/lib/deew-args"

/** Formats that are only possible with dee_shim. */
const SHIM_FORMATS: readonly Format[] = ["ddp-atmos", "thd-atmos"]

/** Formats whose DRC `none` goes through dee_shim (the others do it natively). */
const SHIM_DRC_NONE_FORMATS: readonly Format[] = ["thd", "thd-atmos"]

/**
 * Every 7.1 DDP bitrate of the standard profile needs dee_shim (the Blu-ray
 * profile is native).
 */
export const SHIM_DDP71_BITRATES: readonly number[] = [
  384, 448, 504, 576, 640, 704, 768, 832, 896, 960, 1008, 1024, 1152, 1280,
  1408, 1512, 1536, 1664, 1792,
]

/** `--ddp-mode` values that use dee_shim (the Blu-ray profile is native). */
export const SHIM_DDP_MODES: readonly DdpMode[] = ["standard"]

/** `--ddp-atmos-mode` values that use dee_shim: all of them. */
export const SHIM_DDP_ATMOS_MODES: readonly DdpAtmosMode[] = [
  "streaming",
  "standard",
  "bluray",
]

export function isShimFormat(format: Format): boolean {
  return SHIM_FORMATS.includes(format)
}

/** `drc` is the value of the DRC select. */
export function isShimDrc(format: Format, drc: string): boolean {
  return drc === "none" && SHIM_DRC_NONE_FORMATS.includes(format)
}

/** The bitrates of a 7.1 DDP output that need dee_shim: all of the standard profile, none of the Blu-ray one. */
export function shimBitratesDdp71(bluray: boolean): number[] {
  return bluray ? [] : [...SHIM_DDP71_BITRATES]
}
