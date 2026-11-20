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
  CHANNELS_WARNING,
  IMS_INPUT_WARNING,
  IMS_MIXED_WARNING,
  MASTER_MIXED_WARNING,
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
  master: false,
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
      masters: [],
      onlyMasters: false,
      atmos: false,
      inputs: [{ codec: "truehd", atmos: false, master: false }],
      entries: [
        {
          path: "a.mkv",
          channels: 6,
          language: null,
          title: null,
          master: false,
        },
        {
          path: "b.mkv",
          channels: 6,
          language: null,
          title: null,
          master: false,
        },
      ],
      mismatches: [],
      errors: [],
      skipped: [],
    })
    expect(formatSummary(s)).toBe("5.1 - 48 kHz - 24-bit - 2 files")
  })

  it("keeps the tags of the tracks, and works without them", () => {
    const s = summarizeProbe([
      file({ language: "eng", title: "Main" }),
      file({ path: "b.mkv", language: null }),
      file({ path: "c.mkv", error: "bad" }),
    ])
    expect(s.entries.map((e) => [e.path, e.language, e.title])).toEqual([
      ["a.mkv", "eng", "Main"],
      ["b.mkv", null, null],
    ])
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
      { codec: "truehd", atmos: false, master: false },
      { codec: "truehd", atmos: true, master: false },
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
    expect(w([{ codec: "eac3", atmos: false }])).toEqual([ATMOS_INPUT_WARNING])
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
    expect(relevantBitrates({ ...base, format: "thd-atmos" }).warnings).toEqual(
      [DDP_ATMOS_INPUT_WARNING]
    )
    expect(relevantBitrates({ ...base, format: "ddp" }).warnings).toEqual([])
  })
})

describe("Atmos masters", () => {
  const master = (patch: Partial<FileProbe> = {}) =>
    file({
      path: "m.wav",
      channels: 6,
      codec: "ADM BWF Atmos master",
      atmos: true,
      master: true,
      ...patch,
    })

  it("count as 5.1 only when every input is a master", () => {
    const s = summarizeProbe([
      master(),
      master({ path: "n.atmos", codec: "DAMF Atmos master" }),
    ])
    expect(s.onlyMasters).toBe(true)
    expect(s.channels).toBe(6)
    expect(s.masters).toEqual(["ADM BWF Atmos master", "DAMF Atmos master"])
    expect(s.codecs).toEqual([])
    expect(formatSummary(s)).toBe(
      "ADM BWF Atmos master + DAMF Atmos master - 2 files"
    )
  })

  it("have no say in the channels of the other inputs", () => {
    const s = summarizeProbe([master(), file({ channels: 8, codec: "ac3" })])
    expect(s.onlyMasters).toBe(false)
    expect(s.channels).toBe(8)
    expect(s.mismatches).toEqual([])
    expect(s.codecs).toEqual(["ac3"])
    expect(s.inputs).toContainEqual({
      codec: "ADM BWF Atmos master",
      atmos: true,
      master: true,
    })
    expect(formatSummary(s)).toBe(
      "7.1 - 48 kHz - 24-bit - 2 files - with ADM BWF Atmos master"
    )
  })

  it("are no problem for the Atmos formats", () => {
    const inputs = [
      { codec: "ADM BWF Atmos master", atmos: true, master: true },
    ]
    expect(atmosInputWarnings(inputs, undefined)).toEqual([])
    const base = { channels: 6, downmix: "none" as const, inputs }
    for (const format of [
      "ddp-atmos",
      "thd-atmos",
      "ac4-atmos",
      "ac4-ims",
    ] as const) {
      expect(relevantBitrates({ ...base, format }).warnings).toEqual([])
    }
  })

  it("can't go to TrueHD, AC4 and AAC (unless only measured)", () => {
    const inputs = [{ codec: "DAMF Atmos master", atmos: true, master: true }]
    const base = { channels: 6, downmix: "none" as const, inputs }
    for (const format of ["thd", "ac4", "aac"] as const) {
      const w = relevantBitrates({ ...base, format }).warnings
      expect(w).toHaveLength(1)
      expect(w[0]).toContain(
        "Atmos masters (ADM BWF, DAMF) can't be encoded to"
      )
      expect(
        relevantBitrates({ ...base, format, measureOnly: true }).warnings
      ).toEqual([])
    }
  })

  it("can't be mixed with other inputs for DD and DDP", () => {
    const inputs = [
      { codec: "ADM BWF Atmos master", atmos: true, master: true },
      { codec: "ac3", atmos: false, master: false },
    ]
    const base = { channels: 6, downmix: "none" as const, inputs }
    expect(relevantBitrates({ ...base, format: "ddp" }).warnings).toEqual([
      MASTER_MIXED_WARNING,
    ])
    expect(relevantBitrates({ ...base, format: "dd" }).warnings).toEqual([
      MASTER_MIXED_WARNING,
    ])
    expect(
      relevantBitrates({ ...base, inputs: [inputs[0]], format: "ddp" }).warnings
    ).toEqual([])
  })
})

describe("AC-4 IMS inputs", () => {
  const base = { format: "ac4-ims" as const, downmix: "none" as const }
  const pcm = { codec: "pcm_s24le", atmos: false, master: false }
  const thdAtmos = { codec: "truehd", atmos: true, master: false }
  const thd = { codec: "truehd", atmos: false, master: false }

  it("takes 5.0, 5.1, 7.0, 7.1 and Atmos", () => {
    for (const channels of [5, 6, 7, 8]) {
      expect(
        relevantBitrates({ ...base, channels, inputs: [pcm] }).warnings
      ).toEqual([])
    }
    expect(
      relevantBitrates({ ...base, channels: 8, inputs: [thdAtmos] }).warnings
    ).toEqual([])
  })

  it("rejects stereo and mono unless Atmos", () => {
    for (const channels of [1, 2]) {
      expect(
        relevantBitrates({ ...base, channels, inputs: [pcm] }).warnings
      ).toEqual([IMS_INPUT_WARNING])
    }
    expect(
      relevantBitrates({ ...base, channels: 3, inputs: [pcm] }).warnings
    ).toEqual([CHANNELS_WARNING])
    // a plain TrueHD 7.1 is channel-based
    expect(
      relevantBitrates({ ...base, channels: 8, inputs: [thd] }).warnings
    ).toEqual([])
  })

  it("can't mix Atmos and channel-based inputs", () => {
    expect(
      relevantBitrates({ ...base, channels: null, inputs: [thdAtmos, pcm] })
        .warnings
    ).toEqual([IMS_MIXED_WARNING])
    const master = { codec: "ADM BWF Atmos master", atmos: true, master: true }
    expect(
      relevantBitrates({ ...base, channels: 6, inputs: [master, thd] }).warnings
    ).toEqual([IMS_MIXED_WARNING])
    expect(
      relevantBitrates({ ...base, channels: 8, inputs: [master, thdAtmos] })
        .warnings
    ).toEqual([])
  })
})
