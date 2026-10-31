import { describe, expect, it } from "vitest"

import {
  formatChannels,
  formatSampleRate,
  formatSummary,
  normalizeBitDepth,
  summarizeProbe,
  type FileProbe,
} from "./probe"

const file = (patch: Partial<FileProbe> = {}): FileProbe => ({
  path: "a.mkv",
  channels: 6,
  sampleRate: 48000,
  bitDepth: 24,
  error: null,
  ...patch,
})

describe("summarizeProbe", () => {
  it("summarises consistent files", () => {
    const s = summarizeProbe([file(), file({ path: "b.mkv" })])
    expect(s).toEqual({
      files: 2,
      channels: 6,
      sampleRate: 48000,
      bitDepth: 24,
      mixedBitDepth: false,
      mismatches: [],
      errors: [],
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
    expect([s.channels, s.sampleRate, s.bitDepth]).toEqual([null, null, null])
    expect(formatSummary(s)).toBe("mixed bit depth - 2 files")
  })

  it("shows mixed bit depths without a warning", () => {
    const s = summarizeProbe([file(), file({ path: "b", bitDepth: 16 })])
    expect(s.mismatches).toEqual([])
    expect(s.mixedBitDepth).toBe(true)
    expect(formatSummary(s)).toBe("5.1 - 48 kHz - mixed bit depth - 2 files")
  })

  it("compares bit depths after normalisation", () => {
    const s = summarizeProbe([file({ bitDepth: 20 }), file({ path: "b" })])
    expect(s.mixedBitDepth).toBe(false)
    expect(s.bitDepth).toBe(24)
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
        error: "nope",
      }),
    ])
    expect(s.channels).toBeNull()
    expect(s.errors).toEqual([{ path: "bad", error: "nope" }])
    expect(s.mismatches).toEqual([])
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
