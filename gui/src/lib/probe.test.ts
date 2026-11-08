import { describe, expect, it } from "vitest"

import {
  formatChannels,
  formatSampleRate,
  formatSummary,
  normalizeBitDepth,
  summarizeProbe,
  type FileProbe,
} from "./probe"
import {
  ATMOS_INPUT_WARNING,
  DDP_ATMOS_INPUT_WARNING,
  THD_NO_ATMOS_INPUT_WARNING,
  atmosInputWarnings,
  relevantBitrates,
} from "./deew-args"

const file = (patch: Partial<FileProbe> = {}): FileProbe => ({
  path: "a.mkv",
  channels: 6,
  sampleRate: 48000,
  bitDepth: 24,
  codec: "truehd",
  atmos: false,
  error: null,
  skipped: false,
  ...patch,
})

describe("summarizeProbe", () => {
  it("summarises consistent files", () => {
    const s = summarizeProbe([file(), file({ path: "b.mkv" })])
    expect(s).toEqual({
      files: 2,
      channels: 6,
      sampleRate: 48000,
      bitDepths: [24],
      codecs: ["truehd"],
      atmos: false,
      inputs: [{ codec: "truehd", atmos: false }],
      mismatches: [],
      errors: [],
      skipped: [],
    })
    expect(formatSummary(s)).toBe("5.1 - 48 kHz - 24-bit - 2 files")
  })

  it("reports mismatches like the CLI and has no common value", () => {
    const s = summarizeProbe([
      file(),
      file({ path: "b", channels: 2, sampleRate: 44100, bitDepth: 16 }),
    ])
    expect(s.mismatches).toEqual([
      "each input has to have the same sample rate: [48000, 44100]",
      "each input has to have the same channel count: [6, 2]",
    ])
    expect([s.channels, s.sampleRate]).toEqual([null, null])
    expect(formatSummary(s)).toBe("16/24-bit - 2 files")
  })

  it("shows mixed bit depths without a warning", () => {
    const s = summarizeProbe([
      file({ bitDepth: 32 }),
      file({ path: "b", bitDepth: 16 }),
      file({ path: "c" }),
      file({ path: "d", bitDepth: 16 }),
    ])
    expect(s.mismatches).toEqual([])
    expect(s.bitDepths).toEqual([16, 24, 32])
    expect(formatSummary(s)).toBe("5.1 - 48 kHz - 16/24/32-bit - 4 files")
  })

  it("compares bit depths after normalisation", () => {
    const s = summarizeProbe([file({ bitDepth: 20 }), file({ path: "b" })])
    expect(s.bitDepths).toEqual([24])
    expect(formatSummary(s)).toBe("5.1 - 48 kHz - 24-bit - 2 files")
  })

  it("does not know the layout when a file failed", () => {
    const s = summarizeProbe([
      file(),
      file({
        path: "bad",
        channels: null,
        sampleRate: null,
        bitDepth: null,
        codec: null,
        error: "nope",
      }),
    ])
    expect(s.channels).toBeNull()
    expect(s.errors).toEqual([{ path: "bad", error: "nope" }])
    expect(s.mismatches).toEqual([])
  })

  it("collects the distinct codecs", () => {
    const s = summarizeProbe([
      file({ codec: "eac3" }),
      file({ path: "b" }),
      file({ path: "c", codec: "eac3" }),
    ])
    expect(s.codecs).toEqual(["eac3", "truehd"])
  })

  it("leaves skipped files out of everything but `skipped`", () => {
    const s = summarizeProbe([
      file(),
      file({
        path: "dir/info.nfo",
        channels: null,
        sampleRate: null,
        bitDepth: null,
        codec: null,
        error: "no audio track",
        skipped: true,
      }),
    ])
    expect(s.files).toBe(1)
    expect(s.errors).toEqual([])
    expect(s.channels).toBe(6)
    expect(s.skipped).toEqual([
      { path: "dir/info.nfo", error: "no audio track" },
    ])
    expect(formatSummary(s)).toBe("5.1 - 48 kHz - 24-bit - 1 file")
  })

  it("handles no files", () => {
    const s = summarizeProbe([])
    expect(s.files).toBe(0)
    expect(s.channels).toBeNull()
  })
})

describe("formatting", () => {
  it("names layouts, rates and depths", () => {
    expect(formatChannels(1)).toBe("1.0")
    expect(formatChannels(2)).toBe("2.0")
    expect(formatChannels(6)).toBe("5.1")
    expect(formatChannels(8)).toBe("7.1")
    expect(formatChannels(4)).toBe("4 ch")
    expect(formatSampleRate(44100)).toBe("44.1 kHz")
    expect(formatSampleRate(48000)).toBe("48 kHz")
    expect(formatSampleRate(96000)).toBe("96 kHz")
  })

  it("normalises bit depths like src/plan.rs", () => {
    const cases: [number, number][] = [
      [8, 16],
      [15, 16],
      [16, 16],
      [20, 24],
      [24, 24],
      [28, 32],
      [32, 32],
      [64, 32],
    ]
    for (const [depth, want] of cases)
      expect(normalizeBitDepth(depth)).toBe(want)
  })

  it("uses the singular for one file", () => {
    expect(formatSummary(summarizeProbe([file()]))).toBe(
      "5.1 - 48 kHz - 24-bit - 1 file"
    )
  })
})

describe("Atmos", () => {
  const atmos = (patch: Partial<FileProbe> = {}) =>
    file({ channels: 8, atmos: true, ...patch })

  it("prefixes the channels when all files are Atmos", () => {
    const s = summarizeProbe([atmos()])
    expect(s.atmos).toBe(true)
    expect(formatSummary(s)).toBe("Atmos 7.1 - 48 kHz - 24-bit - 1 file")
  })

  it("does not prefix non-Atmos or mixed files", () => {
    expect(formatSummary(summarizeProbe([file({ channels: 8 })]))).toBe(
      "7.1 - 48 kHz - 24-bit - 1 file"
    )
    const mixed = summarizeProbe([atmos(), atmos({ path: "b", atmos: false })])
    expect(mixed.atmos).toBe(false)
    expect(formatSummary(mixed)).toBe("7.1 - 48 kHz - 24-bit - 2 files")
    expect(mixed.inputs).toEqual([
      { codec: "truehd", atmos: false },
      { codec: "truehd", atmos: true },
    ])
  })

  it("warns once per kind of unsuitable input", () => {
    const w = (inputs: { codec: string; atmos: boolean }[]) =>
      atmosInputWarnings(inputs, undefined)
    expect(w([{ codec: "truehd", atmos: true }])).toEqual([])
    expect(w([{ codec: "eac3", atmos: true }])).toEqual([
      DDP_ATMOS_INPUT_WARNING,
    ])
    expect(w([{ codec: "truehd", atmos: false }])).toEqual([
      THD_NO_ATMOS_INPUT_WARNING,
    ])
    expect(w([{ codec: "eac3", atmos: false }])).toEqual([
      ATMOS_INPUT_WARNING,
    ])
    expect(
      w([
        { codec: "eac3", atmos: true },
        { codec: "truehd", atmos: false },
        { codec: "flac", atmos: false },
      ])
    ).toEqual([
      DDP_ATMOS_INPUT_WARNING,
      THD_NO_ATMOS_INPUT_WARNING,
      ATMOS_INPUT_WARNING,
    ])
    expect(atmosInputWarnings(undefined, ["eac3"])).toEqual([
      ATMOS_INPUT_WARNING,
    ])
  })

  it("is used by relevantBitrates for the Atmos formats only", () => {
    const inputs = [{ codec: "eac3", atmos: true }]
    const base = { channels: 6, downmix: "none" as const, inputs }
    expect(
      relevantBitrates({ ...base, format: "thd-atmos" }).warnings
    ).toEqual([DDP_ATMOS_INPUT_WARNING])
    expect(relevantBitrates({ ...base, format: "ddp" }).warnings).toEqual([])
  })
})
