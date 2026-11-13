import { readFileSync } from "node:fs"
import { resolve } from "node:path"
import { describe, expect, it } from "vitest"

import {
  BITRATE_TABLE,
  ATMOS_INPUT_WARNING,
  CHANNELS_WARNING,
  DEFAULT_OPTIONS,
  FORMATS,
  DRC_VALUES,
  ddpAtmosModeApplies,
  ddpModeApplies,
  parseDefaultModes,
  disableLfeApplies,
  noPhaseShiftApplies,
  drcChoices,
  isAtmos,
  supportsDialnorm,
  FORMAT_LABELS,
  INFO_COMMANDS,
  TEMPLATE_DEFAULT_BITRATES,
  tomlKey,
  bitrateChoices,
  bitrateOptions,
  buildArgs,
  detectPlatform,
  formatCommand,
  infoArgs,
  quoteArg,
  relevantBitrates,
  relevantDownmixes,
  setFlag,
  setFormat,
  validate,
  type BitrateContext,
  type DeewOptions,
  type Downmix,
} from "./deew-args"
import {
  SHIM_DDP71_BITRATES,
  isShimDrc,
  isShimFormat,
  shimBitratesDdp71,
} from "./shim"

const opts = (patch: Partial<DeewOptions>): DeewOptions => ({
  ...DEFAULT_OPTIONS,
  ...patch,
})

describe("buildArgs", () => {
  it("is empty for the defaults", () => {
    expect(buildArgs(DEFAULT_OPTIONS)).toEqual([])
  })

  it("omits options equal to their defaults", () => {
    expect(
      buildArgs(
        opts({
          inputs: ["a.mkv"],
          trackIndex: "0",
          format: "ddp",
          bitrate: "default",
          downmix: "none",
          drc: "none",
          dialnorm: "0",
          output: "  ",
          delay: " ",
          instances: "",
        })
      )
    ).toEqual(["-i", "a.mkv"])
  })

  it("emits every flag", () => {
    expect(
      buildArgs({
        inputs: ["a.mkv", "/b/c"],
        trackIndex: "2",
        output: "/out",
        format: "dd",
        bitrate: "640",
        downmix: "2",
        delay: "+5ms",
        drc: "speech",
        dialnorm: "-27",
        instances: "50%",
        outputToInput: false,
        ddpMode: "default",
        ddpAtmosMode: "default",
        disableLfe: false,
        noPhaseShift: false,
        keepTemp: true,
        keepLog: true,
        measureOnly: true,
        longArgument: true,
        noPrompt: true,
      })
    ).toEqual([
      "-i",
      "a.mkv",
      "/b/c",
      "-ti",
      "2",
      "-o",
      "/out",
      "-f",
      "dd",
      "-b",
      "640",
      "-dm",
      "2",
      "-d=+5ms",
      "-r",
      "speech",
      "-dn",
      "-27",
      "-in",
      "50%",
      "-k",
      "-kl",
      "-mo",
      "-la",
      "-np",
    ])
  })

  it("passes a negative delay in the -d=<value> form", () => {
    expect(buildArgs(opts({ delay: "-5ms" }))).toEqual(["-d=-5ms"])
    expect(buildArgs(opts({ delay: " -24@pal " }))).toEqual(["-d=-24@pal"])
    expect(buildArgs(opts({ delay: "+10@24000/1001" }))).toEqual([
      "-d=+10@24000/1001",
    ])
  })

  it("keeps paths with spaces and quotes as single arguments", () => {
    const inputs = ["/m/My Movie.mkv", '/m/it\'s "x".mkv']
    expect(buildArgs(opts({ inputs }))).toEqual(["-i", ...inputs])
  })

  it("ignores unparsable integers (validate reports them)", () => {
    const o = opts({ trackIndex: "x", dialnorm: "" })
    expect(buildArgs(o)).toEqual([])
    expect(validate(o)).toHaveLength(2)
  })

  it("only includes the selected bitrate and format", () => {
    expect(buildArgs(opts({ format: "thd" }))).toEqual(["-f", "thd"])
    expect(buildArgs(opts({ format: "ddp-atmos" }))).toEqual([
      "-f",
      "ddp-atmos",
    ])
    expect(buildArgs(opts({ bitrate: "448" }))).toEqual(["-b", "448"])
  })
})

describe("validate", () => {
  it("accepts the defaults and the dialnorm range", () => {
    expect(validate(DEFAULT_OPTIONS)).toEqual([])
    expect(validate(opts({ dialnorm: "-31" }))).toEqual([])
    expect(validate(opts({ trackIndex: "3" }))).toEqual([])
  })

  it("rejects out of range values", () => {
    expect(validate(opts({ dialnorm: "-32" }))).toHaveLength(1)
    expect(validate(opts({ dialnorm: "1" }))).toHaveLength(1)
    expect(validate(opts({ trackIndex: "-1" }))).toHaveLength(1)
    expect(validate(opts({ trackIndex: "1.5" }))).toHaveLength(1)
  })

  it("rejects dialnorm and measure only where the format cannot do them", () => {
    for (const format of [
      "dd",
      "ddp",
      "thd",
      "ddp-atmos",
      "thd-atmos",
      "ac4",
    ] as const) {
      expect(
        validate(opts({ format, drc: "music_light", dialnorm: "-27" }))
      ).toEqual([])
    }
    for (const format of ["aac", "ac4-atmos"] as const) {
      expect(
        validate(opts({ format, drc: "music_light", dialnorm: "-27" }))
      ).toHaveLength(1)
      expect(
        validate(opts({ format, drc: "music_light", dialnorm: "0" }))
      ).toEqual([])
    }
    expect(validate(opts({ format: "aac", measureOnly: true }))).toEqual([])
    expect(validate(opts({ format: "thd", measureOnly: true }))).toEqual([])
    // -mo ignores the format and the encoding options
    for (const format of ["ddp-atmos", "ac4-atmos", "ac4"] as const) {
      expect(
        validate(
          opts({ format, drc: "none", dialnorm: "-27", measureOnly: true })
        )
      ).toEqual([])
    }
    expect(validate(opts({ trackIndex: "x", measureOnly: true }))).toHaveLength(
      1
    )
  })
})

describe("DRC none", () => {
  it("is offered last, except for AC4", () => {
    for (const f of [
      "dd",
      "ddp",
      "ddp-atmos",
      "thd",
      "thd-atmos",
      "aac",
    ] as const) {
      expect(drcChoices(f)[0]).toBe("none")
    }
    for (const f of ["ac4", "ac4-atmos"] as const) {
      expect(drcChoices(f)).not.toContain("none")
      expect(drcChoices(f)).toHaveLength(5)
    }
  })

  it("is the default; other profiles are passed with -r", () => {
    expect(DEFAULT_OPTIONS.drc).toBe("none")
    expect(buildArgs(opts({ drc: "none" }))).toEqual([])
    expect(buildArgs(opts({ drc: "music_light" }))).toEqual([
      "-r",
      "music_light",
    ])
  })

  it("lists none first", () => {
    expect([...DRC_VALUES]).toEqual([
      "none",
      "film_light",
      "film_standard",
      "music_light",
      "music_standard",
      "speech",
    ])
  })

  it("makes AC4 get -r music_light from the default state", () => {
    const o = setFormat(opts({ inputs: ["a.mkv"] }), "ac4")
    expect(o.drc).toBe("music_light")
    expect(buildArgs(o)).toEqual([
      "-i",
      "a.mkv",
      "-f",
      "ac4",
      "-r",
      "music_light",
    ])
  })

  it("is reported for AC4 and reset by setFormat", () => {
    for (const format of ["ac4", "ac4-atmos"] as const) {
      expect(validate(opts({ format, drc: "none" }))).toEqual([
        "DRC none is not supported for AC4.",
      ])
      expect(setFormat(opts({ drc: "none" }), format).drc).toBe("music_light")
    }
    expect(validate(opts({ format: "thd", drc: "none" }))).toEqual([])
    expect(setFormat(opts({ drc: "none" }), "thd").drc).toBe("none")
    expect(setFormat(opts({ drc: "speech" }), "ac4").drc).toBe("speech")
  })
})

describe("formats", () => {
  it("are listed in dropdown order with their labels", () => {
    expect([...FORMATS]).toEqual([
      "dd",
      "ddp",
      "ddp-atmos",
      "thd",
      "thd-atmos",
      "ac4",
      "ac4-atmos",
      "aac",
    ])
    expect(FORMATS.map((f) => FORMAT_LABELS[f])).toEqual([
      "DD",
      "DDP",
      "DDP Atmos",
      "TrueHD",
      "TrueHD Atmos",
      "AC4",
      "AC4 Atmos",
      "AAC",
    ])
  })

  it("has the mode options but no -fb / -fs", () => {
    const keys = Object.keys(DEFAULT_OPTIONS)
    expect(keys).not.toContain("forceStandard")
    expect(keys).not.toContain("forceBluray")
    expect(keys).toContain("ddpMode")
    expect(keys).toContain("ddpAtmosMode")
    expect(keys).toContain("outputToInput")
  })

  it("setFormat resets what the new format cannot do", () => {
    const o = opts({
      format: "ddp",
      downmix: "1",
      dialnorm: "-27",
      measureOnly: true,
    })
    expect(setFormat(o, "ac4")).toMatchObject({
      downmix: "none",
      dialnorm: "-27",
    })
    expect(setFormat(o, "aac")).toMatchObject({
      downmix: "none",
      dialnorm: "0",
      measureOnly: true,
    })
    expect(setFormat(o, "ddp-atmos")).toMatchObject({
      downmix: "none",
      dialnorm: "-27",
      measureOnly: true,
    })
  })
})

describe("thd-atmos", () => {
  it("is passed as -f thd-atmos with no -r by default", () => {
    expect(buildArgs(opts({ inputs: ["a.thd"], format: "thd-atmos" }))).toEqual(
      ["-i", "a.thd", "-f", "thd-atmos"]
    )
  })

  it("has no bitrate, downmix or measure only", () => {
    expect(bitrateOptions("thd-atmos")).toEqual([])
    const info = relevantBitrates({
      format: "thd-atmos",
      channels: 8,
      downmix: "none",
      codecs: ["truehd"],
    })
    expect(info).toMatchObject({ status: "na", bitrates: [] })
    expect(info.defaultKey).toBeUndefined()
    expect(info.warnings).toEqual([])
    expect(relevantDownmixes("thd-atmos", 8)).toEqual(["none"])
    expect(supportsDialnorm("thd-atmos")).toBe(true)
    expect(isAtmos("thd-atmos")).toBe(true)
    expect(drcChoices("thd-atmos")).toEqual([...DRC_VALUES])
  })

  it("needs a TrueHD input", () => {
    const info = relevantBitrates({
      format: "thd-atmos",
      channels: 6,
      downmix: "none",
      codecs: ["eac3"],
    })
    expect(info.warnings).toEqual([ATMOS_INPUT_WARNING])
  })

  it("validates like the other Atmos formats", () => {
    expect(validate(opts({ format: "thd-atmos" }))).toEqual([])
    expect(validate(opts({ format: "thd-atmos", dialnorm: "-27" }))).toEqual([])
    expect(validate(opts({ format: "thd-atmos", measureOnly: true }))).toEqual(
      []
    )
    expect(validate(opts({ format: "thd-atmos", drc: "speech" }))).toEqual([])
  })

  it("setFormat resets invalid options when switching to it", () => {
    const o = opts({
      bitrate: "448",
      downmix: "2",
      dialnorm: "-27",
      measureOnly: true,
      drc: "speech",
    })
    expect(setFormat(o, "thd-atmos")).toMatchObject({
      bitrate: "default",
      downmix: "none",
      dialnorm: "-27",
      measureOnly: true,
      drc: "speech",
    })
    expect(setFormat(opts({ drc: "none" }), "thd-atmos").drc).toBe("none")
    // ...and from it to AC4.
    expect(
      setFormat(opts({ format: "thd-atmos", drc: "none" }), "ac4").drc
    ).toBe("music_light")
  })
})

describe("--ddp-mode and --ddp-atmos-mode", () => {
  const ddp71Of = (channels: number | null, downmix: Downmix = "none") =>
    relevantBitrates({ format: "ddp", channels, downmix, ddpMode: "bluray" })
      .ddp71
  const bluray = opts({ ddpMode: "bluray" })

  it("--ddp-mode is emitted only for a probed 7.x DDP output", () => {
    // No probe: nothing.
    expect(ddp71Of(null)).toBeUndefined()
    expect(buildArgs(bluray)).toEqual([])
    expect(buildArgs(bluray, { ddp71: ddp71Of(null) })).toEqual([])
    // 7.1 and 7.0.
    for (const channels of [8, 7]) {
      expect(ddp71Of(channels)).toBe(true)
      expect(buildArgs(bluray, { ddp71: ddp71Of(channels) })).toEqual([
        "--ddp-mode",
        "bluray",
      ])
    }
    expect(
      buildArgs(opts({ ddpMode: "bluray", keepTemp: true }), { ddp71: true })
    ).toEqual(["--ddp-mode", "bluray", "-k"])
    // 7.1 downmixed to 5.1, 5.1 / 5.0 / 2.0: nothing.
    expect(ddp71Of(8, "6")).toBe(false)
    expect(buildArgs(bluray, { ddp71: ddp71Of(8, "6") })).toEqual([])
    for (const channels of [6, 5, 2, 1]) {
      expect(buildArgs(bluray, { ddp71: ddp71Of(channels) })).toEqual([])
    }
    // Other formats: nothing.
    for (const format of ["ac4", "dd", "thd"] as const) {
      expect(
        buildArgs(opts({ ddpMode: "bluray", format }), { ddp71: true })
      ).not.toContain("--ddp-mode")
    }
    expect(buildArgs(opts({ ddpMode: "default" }), { ddp71: true })).toEqual([])
  })

  it("only passes a mode that differs from the config default", () => {
    const std = opts({ ddpMode: "standard" })
    expect(buildArgs(std, { ddp71: true })).toEqual([])
    expect(
      buildArgs(std, {
        ddp71: true,
        defaultModes: { ddp: "bluray", ddpAtmos: "streaming" },
      })
    ).toEqual(["--ddp-mode", "standard"])
    expect(
      buildArgs(bluray, {
        ddp71: true,
        defaultModes: { ddp: "bluray", ddpAtmos: "streaming" },
      })
    ).toEqual([])
    const atmos = (m: "streaming" | "standard" | "bluray" | "default") =>
      opts({ format: "ddp-atmos", ddpAtmosMode: m })
    expect(buildArgs(atmos("streaming"))).toEqual(["-f", "ddp-atmos"])
    expect(buildArgs(atmos("default"))).toEqual(["-f", "ddp-atmos"])
    expect(buildArgs(atmos("standard"))).toEqual([
      "-f",
      "ddp-atmos",
      "--ddp-atmos-mode",
      "standard",
    ])
    expect(
      buildArgs(atmos("bluray"), {
        defaultModes: { ddp: "standard", ddpAtmos: "bluray" },
      })
    ).toEqual(["-f", "ddp-atmos"])
    // not for other formats
    expect(buildArgs(opts({ format: "ddp", ddpAtmosMode: "bluray" }))).toEqual(
      []
    )
  })

  it("the selects are shown for DDP 7.1 / unknown and DDP Atmos", () => {
    const info = (channels: number | null) =>
      relevantBitrates({ format: "ddp", channels, downmix: "none" })
    expect(ddpModeApplies("ddp", info(null))).toBe(true)
    expect(ddpModeApplies("ddp", info(8))).toBe(true)
    expect(ddpModeApplies("ddp", info(7))).toBe(true)
    expect(ddpModeApplies("ddp", info(6))).toBe(false)
    expect(ddpModeApplies("ddp", info(2))).toBe(false)
    expect(ddpModeApplies("dd", info(8))).toBe(false)
    expect(ddpAtmosModeApplies("ddp-atmos")).toBe(true)
    expect(ddpAtmosModeApplies("ddp")).toBe(false)
    expect(ddpAtmosModeApplies("thd-atmos")).toBe(false)
  })

  it("setFormat keeps the choices", () => {
    expect(setFormat(bluray, "dd").ddpMode).toBe("bluray")
  })

  it("parseDefaultModes falls back to the program defaults", () => {
    expect(parseDefaultModes("bluray", "standard")).toEqual({
      ddp: "bluray",
      ddpAtmos: "standard",
    })
    expect(parseDefaultModes("", undefined)).toEqual({
      ddp: "standard",
      ddpAtmos: "streaming",
    })
    expect(parseDefaultModes("streaming", "x")).toEqual({
      ddp: "standard",
      ddpAtmos: "streaming",
    })
  })

  it("DDP Atmos uses the list and default key of its mode", () => {
    for (const [mode, key] of [
      ["streaming", "ddp_atmos_streaming"],
      ["standard", "ddp_atmos_standard"],
      ["bluray", "ddp_atmos_bluray"],
    ] as const) {
      const info = relevantBitrates({
        format: "ddp-atmos",
        channels: null,
        downmix: "none",
        atmosMode: mode,
      })
      expect(info.bitrates).toEqual(BITRATE_TABLE[key])
      expect(info.defaultKey).toBe(key)
    }
    expect(BITRATE_TABLE.ddp_atmos_streaming).toEqual([
      384, 448, 576, 640, 768, 1024,
    ])
    expect(BITRATE_TABLE.ddp_atmos_standard).toEqual([
      768, 1024, 1152, 1280, 1408, 1536, 1664, 1792, 2048,
    ])
    expect(BITRATE_TABLE.ddp_atmos_bluray).toEqual([
      1152, 1280, 1408, 1512, 1536, 1664,
    ])
  })
})

describe("-oi (save next to input)", () => {
  it("replaces -o", () => {
    expect(buildArgs(opts({ output: "/out", outputToInput: true }))).toEqual([
      "-oi",
    ])
    expect(buildArgs(opts({ output: "/out" }))).toEqual(["-o", "/out"])
    expect(buildArgs(opts({ outputToInput: true, keepTemp: true }))).toEqual([
      "-oi",
      "-k",
    ])
  })
})

describe("-dl (disable LFE)", () => {
  const dl = (
    format: Parameters<typeof disableLfeApplies>[0],
    channels: number | null,
    downmix: Parameters<typeof disableLfeApplies>[2] = "none"
  ) => buildArgs(opts({ disableLfe: true, format, downmix }), { channels })

  it("is enabled for a probed 5.1 or 7.1 input", () => {
    for (const channels of [6, 8]) {
      expect(dl("ddp", channels)).toEqual(["-dl"])
      expect(dl("dd", channels)).toEqual(["-f", "dd", "-dl"])
      expect(dl("thd", channels)).toEqual(["-f", "thd", "-dl"])
      expect(dl("ac4", channels)).toContain("-dl")
    }
    // A downmix that keeps more than stereo is fine.
    expect(dl("ddp", 8, "6")).toContain("-dl")
    expect(disableLfeApplies("ddp", 6, "none")).toBe(true)
  })

  it("is disabled otherwise", () => {
    for (const channels of [null, 1, 2, 5, 7]) {
      expect(dl("ddp", channels)).toEqual([])
    }
    for (const format of ["ddp-atmos", "thd-atmos", "ac4-atmos"] as const) {
      expect(dl(format, 8)).not.toContain("-dl")
    }
    expect(dl("aac", 6)).not.toContain("-dl")
    expect(dl("ddp", 8, "2")).toEqual(["-dm", "2"])
    expect(dl("ddp", 6, "1")).not.toContain("-dl")
    expect(buildArgs(opts({ disableLfe: true }))).toEqual([])
    expect(buildArgs(opts({ disableLfe: false }), { channels: 6 })).toEqual([])
  })

  it("comes right after --ddp-mode and before -k", () => {
    expect(
      buildArgs(opts({ ddpMode: "bluray", disableLfe: true, keepTemp: true }), {
        ddp71: true,
        channels: 8,
      })
    ).toEqual(["--ddp-mode", "bluray", "-dl", "-k"])
  })
})

describe("-nps (no surround phase shift)", () => {
  const nps = (format: Parameters<typeof noPhaseShiftApplies>[0]) =>
    buildArgs(opts({ noPhaseShift: true, format }))

  it("is passed for DD and DDP", () => {
    expect(nps("ddp")).toEqual(["-nps"])
    expect(nps("dd")).toEqual(["-f", "dd", "-nps"])
    expect(noPhaseShiftApplies("dd")).toBe(true)
    expect(noPhaseShiftApplies("ddp")).toBe(true)
  })

  it("is not passed for other formats, so the CLI never fails on it", () => {
    for (const format of [
      "ddp-atmos",
      "thd",
      "thd-atmos",
      "ac4",
      "ac4-atmos",
      "aac",
    ] as const) {
      expect(nps(format)).not.toContain("-nps")
      expect(noPhaseShiftApplies(format)).toBe(false)
    }
  })

  it("is off by default and comes after -dl and before -k", () => {
    expect(buildArgs(opts({}))).toEqual([])
    expect(
      buildArgs(
        opts({ noPhaseShift: true, disableLfe: true, keepTemp: true }),
        { channels: 6 }
      )
    ).toEqual(["-dl", "-nps", "-k"])
    expect(validate(opts({ noPhaseShift: true, format: "thd" }))).toEqual([])
  })
})

describe("5.0 and 7.0 inputs", () => {
  const table = BITRATE_TABLE
  it("use the 5.1 / 7.1 layouts", () => {
    expect(
      relevantBitrates({ format: "dd", channels: 5, downmix: "none" }).bitrates
    ).toEqual(table.dd_51)
    expect(
      relevantBitrates({ format: "ddp", channels: 5, downmix: "none" })
        .defaultKey
    ).toBe("ddp_5_1")
    const seven = relevantBitrates({
      format: "ddp",
      channels: 7,
      downmix: "none",
    })
    expect(seven.bitrates).toEqual(table.ddp_71)
    expect(seven.defaultKey).toBe("ddp_7_1_standard")
    // DD and AC4 reduce 7.0 to 5.1 like 7.1.
    expect(
      relevantBitrates({ format: "dd", channels: 7, downmix: "none" }).bitrates
    ).toEqual(table.dd_51)
    expect(
      relevantBitrates({ format: "ac4", channels: 7, downmix: "none" }).bitrates
    ).toEqual(table.ac4_51)
    expect(
      relevantBitrates({ format: "aac", channels: 5, downmix: "none" }).bitrates
    ).toEqual(table.aac_20)
  })

  it("offer the downmixes below the channel count", () => {
    expect(relevantDownmixes("ddp", 5)).toEqual(["none", "1", "2"])
    expect(relevantDownmixes("ddp", 7)).toEqual(["none", "1", "2", "6"])
    expect(relevantDownmixes("ac4", 7)).toEqual(["none", "2", "6"])
    expect(relevantDownmixes("ac4", 5)).toEqual(["none", "2"])
    // TrueHD has no mono
    expect(relevantDownmixes("thd", 8)).toEqual(["none", "2", "6"])
    expect(relevantDownmixes("thd", 7)).toEqual(["none", "2", "6"])
    expect(relevantDownmixes("thd", 6)).toEqual(["none", "2"])
    expect(relevantDownmixes("thd", 5)).toEqual(["none", "2"])
    expect(relevantDownmixes("thd", 2)).toEqual(["none"])
    expect(relevantDownmixes("thd", null)).toEqual(["none", "2", "6"])
    expect(relevantDownmixes("thd-atmos", null)).toEqual(["none"])
  })

  it("are not an unsupported input any more", () => {
    for (const channels of [5, 7]) {
      expect(
        relevantBitrates({ format: "ddp", channels, downmix: "none" }).status
      ).toBe("ok")
    }
    for (const channels of [0, 3, 4, 9]) {
      expect(
        relevantBitrates({ format: "ddp", channels, downmix: "none" }).status
      ).toBe("invalid")
    }
  })
})

describe("shim badges", () => {
  it("marks the formats that need dee_shim", () => {
    expect(FORMATS.filter(isShimFormat)).toEqual(["ddp-atmos", "thd-atmos"])
  })

  it("marks DRC none for TrueHD formats only", () => {
    for (const f of ["thd", "thd-atmos"] as const) {
      expect(isShimDrc(f, "none")).toBe(true)
      expect(isShimDrc(f, "speech")).toBe(false)
    }
    for (const f of ["dd", "ddp", "ddp-atmos", "ac4", "aac"] as const) {
      expect(isShimDrc(f, "none")).toBe(false)
    }
  })

  it("marks every 7.1 DDP bitrate of the standard profile", () => {
    // The standard profile is encoded with dee_shim at any rate.
    expect(SHIM_DDP71_BITRATES).toEqual(BITRATE_TABLE.ddp_71)
    expect(shimBitratesDdp71(true)).toEqual([])
    const info = relevantBitrates({
      format: "ddp",
      channels: 8,
      downmix: "none",
    })
    const choices = bitrateChoices(info, {})
    expect(choices.filter((c) => c.shim).length).toBe(19)
    expect(choices.find((c) => c.label.startsWith("1536"))?.shim).toBe(true)
    expect(choices.find((c) => c.label.startsWith("1792"))?.shim).toBe(true)
    const bd = bitrateChoices(
      relevantBitrates({
        format: "ddp",
        channels: 8,
        downmix: "none",
        ddpMode: "bluray",
      }),
      {}
    )
    expect(bd.some((c) => c.shim)).toBe(false)
    expect(bd).toHaveLength(5)
    // No badge for 5.1 DDP or without a known layout.
    const five = relevantBitrates({
      format: "ddp",
      channels: 6,
      downmix: "none",
    })
    expect(bitrateChoices(five, {}).some((c) => c.shim)).toBe(false)
    const unknown = relevantBitrates({
      format: "ddp",
      channels: null,
      downmix: "none",
    })
    expect(bitrateChoices(unknown, {}).some((c) => c.shim)).toBe(false)
  })

  it("marks every DDP Atmos bitrate in every mode, but not AC4 Atmos", () => {
    for (const atmosMode of ["streaming", "standard", "bluray"] as const) {
      const info = relevantBitrates({
        format: "ddp-atmos",
        channels: 8,
        downmix: "none",
        atmosMode,
      })
      const choices = bitrateChoices(info, {})
      expect(choices.length).toBeGreaterThan(0)
      expect(choices.every((c) => c.shim)).toBe(true)
    }
    const ac4 = bitrateChoices(
      relevantBitrates({ format: "ac4-atmos", channels: 8, downmix: "none" }),
      {}
    )
    expect(ac4.length).toBeGreaterThan(0)
    expect(ac4.some((c) => c.shim)).toBe(false)
    const ddp51 = bitrateChoices(
      relevantBitrates({ format: "ddp", channels: 6, downmix: "none" }),
      {}
    )
    expect(ddp51.some((c) => c.shim)).toBe(false)
  })
})

describe("keep log", () => {
  it("is -kl right after -k", () => {
    expect(buildArgs(opts({ keepLog: true }))).toEqual(["-kl"])
    expect(buildArgs(opts({ keepTemp: true, keepLog: true }))).toEqual([
      "-k",
      "-kl",
    ])
  })
})

describe("setFlag", () => {
  it("sets one checkbox", () => {
    const o = setFlag(
      setFlag(DEFAULT_OPTIONS, "keepTemp", true),
      "noPrompt",
      true
    )
    expect(buildArgs(o)).toEqual(["-k", "-np"])
    expect(buildArgs(setFlag(o, "keepTemp", false))).toEqual(["-np"])
  })
})

describe("info commands", () => {
  it("are single flags without input", () => {
    expect(INFO_COMMANDS.map((c) => c.flag)).toEqual([
      "-h",
      "-v",
      "-cl",
      "-lb",
      "-pl",
      "-c",
      "-gc",
      "-gt",
    ])
    const groups = INFO_COMMANDS.map((c) => c.group)
    expect(groups).toEqual([...groups].sort())
    for (const { flag } of INFO_COMMANDS) {
      expect(infoArgs(flag)).toEqual([flag])
    }
  })
})

describe("bitrates", () => {
  const table = BITRATE_TABLE

  it("returns the union per format", () => {
    const dd = bitrateOptions("dd")
    expect(dd[0]).toBe(96)
    expect(dd.at(-1)).toBe(640)
    expect(dd).toContain(224)
    const ddp = bitrateOptions("ddp")
    expect(ddp[0]).toBe(32)
    expect(ddp.at(-1)).toBe(1792)
    expect(ddp).toContain(1280)
    expect(new Set(ddp).size).toBe(ddp.length)
    expect(ddp).toEqual([...ddp].sort((a, b) => a - b))
    expect(bitrateOptions("ac4")).toEqual(table.ac4_20)
    expect(bitrateOptions("aac")[0]).toBe(56)
    expect(bitrateOptions("ddp-atmos")).toEqual([
      384, 448, 576, 640, 768, 1024, 1152, 1280, 1408, 1512, 1536, 1664, 1792,
      2048,
    ])
    expect(bitrateOptions("ac4-atmos")).toEqual([256, 320, 448, 768])
    expect(bitrateOptions("thd")).toEqual([])
  })

  it("resets an impossible bitrate when the format changes", () => {
    expect(setFormat(opts({ bitrate: "1024" }), "dd").bitrate).toBe("default")
    expect(setFormat(opts({ bitrate: "448" }), "dd").bitrate).toBe("448")
    expect(setFormat(opts({ bitrate: "448" }), "thd").bitrate).toBe("default")
    expect(setFormat(opts({ bitrate: "448" }), "ac4-atmos").bitrate).toBe("448")
    expect(setFormat(opts({ bitrate: "512" }), "ac4-atmos").bitrate).toBe(
      "default"
    )
    expect(setFormat(opts({ bitrate: "default" }), "ac4").bitrate).toBe(
      "default"
    )
  })

  it("matches the tables in src/data.rs", () => {
    const source = readFileSync(
      resolve(import.meta.dirname, "../../../src/data.rs"),
      "utf8"
    )
    const block = source.slice(
      source.indexOf("pub const BITRATES"),
      source.indexOf("pub fn allowed_bitrates")
    )
    const parsed: Record<string, number[]> = {}
    for (const m of block.matchAll(/"([a-z0-9_]+)",\s*&\[([^\]]*)\]/g)) {
      parsed[m[1]] = m[2]
        .split(",")
        .map((s) => s.trim())
        .filter(Boolean)
        .map(Number)
    }
    expect(Object.keys(parsed).length).toBeGreaterThan(5)
    expect(parsed).toEqual(
      Object.fromEntries(
        Object.entries(BITRATE_TABLE).map(([k, v]) => [k, [...v]])
      )
    )
  })
})

describe("quoteArg / formatCommand", () => {
  it("leaves simple arguments alone", () => {
    expect(quoteArg("-d=-5ms", "posix")).toBe("-d=-5ms")
    expect(quoteArg("/a/b-c_d.mkv", "posix")).toBe("/a/b-c_d.mkv")
    expect(quoteArg("50%", "posix")).toBe("50%")
    expect(quoteArg("C:\\a\\b.mkv", "windows")).toBe("C:\\a\\b.mkv")
  })

  it("single-quotes for POSIX", () => {
    expect(quoteArg("/m/My Movie.mkv", "posix")).toBe("'/m/My Movie.mkv'")
    expect(quoteArg("it's", "posix")).toBe(`'it'\\''s'`)
    expect(quoteArg("", "posix")).toBe("''")
    expect(quoteArg("a$b", "posix")).toBe("'a$b'")
    expect(quoteArg('say "hi"', "posix")).toBe(`'say "hi"'`)
    expect(quoteArg("-d=+1,52s", "posix")).toBe("-d=+1,52s")
    expect(quoteArg("-d=+10@24000/1001", "posix")).toBe("-d=+10@24000/1001")
  })

  it("double-quotes for Windows", () => {
    expect(quoteArg("C:\\My Movies\\a.mkv", "windows")).toBe(
      '"C:\\My Movies\\a.mkv"'
    )
    expect(quoteArg("", "windows")).toBe('""')
    expect(quoteArg('say "hi"', "windows")).toBe('"say \\"hi\\""')
    expect(quoteArg("C:\\dir with space\\", "windows")).toBe(
      '"C:\\dir with space\\\\"'
    )
    expect(quoteArg('a\\"b c', "windows")).toBe('"a\\\\\\"b c"')
    expect(quoteArg("a&b", "windows")).toBe('"a&b"')
  })

  it("formats a full command", () => {
    const args = buildArgs(
      opts({ inputs: ["/m/My Movie.mkv"], delay: "-5ms", noPrompt: true })
    )
    expect(formatCommand(args, "posix")).toBe(
      "deew -i '/m/My Movie.mkv' -d=-5ms -np"
    )
    expect(formatCommand(["-lb"], "windows")).toBe("deew -lb")
    expect(formatCommand(["-i", "C:\\My Movies\\a.mkv"], "windows")).toBe(
      'deew -i "C:\\My Movies\\a.mkv"'
    )
  })

  it("detects the platform from the user agent", () => {
    expect(detectPlatform("Mozilla/5.0 (Windows NT 10.0; Win64; x64)")).toBe(
      "windows"
    )
    expect(detectPlatform("Mozilla/5.0 (X11; Linux x86_64)")).toBe("posix")
    expect(detectPlatform("Mozilla/5.0 (Macintosh; Intel Mac OS X)")).toBe(
      "posix"
    )
  })
})

describe("relevantBitrates", () => {
  const ctx = (patch: Partial<BitrateContext>): BitrateContext => ({
    format: "ddp",
    channels: 6,
    downmix: "none",
    ...patch,
  })
  const table = BITRATE_TABLE

  it("picks the table of the input layout", () => {
    for (const [format, channels, key] of [
      ["dd", 1, "dd_10"],
      ["dd", 2, "dd_20"],
      ["dd", 6, "dd_51"],
      ["ddp", 1, "ddp_10"],
      ["ddp", 2, "ddp_20"],
      ["ddp", 6, "ddp_51"],
      ["ddp", 8, "ddp_71"],
      ["ac4", 2, "ac4_20"],
      ["ac4", 6, "ac4_51"],
      ["aac", 2, "aac_20"],
    ] as const) {
      const info = relevantBitrates(ctx({ format, channels }))
      expect(info.status).toBe("ok")
      expect(info.bitrates).toEqual(table[key])
      expect(info.warnings).toEqual([])
    }
  })

  it("DDP 7.1 uses the standard list, or the Blu-ray list with --ddp-mode bluray", () => {
    expect(table.ddp_71).toEqual([
      384, 448, 504, 576, 640, 704, 768, 832, 896, 960, 1008, 1024, 1152, 1280,
      1408, 1512, 1536, 1664, 1792,
    ])
    expect(table.ddp_71_bluray).toEqual([768, 1024, 1280, 1536, 1664])
    const std = relevantBitrates(ctx({ channels: 8 }))
    expect(std.bitrates).toEqual(table.ddp_71)
    expect(std.defaultKey).toBe("ddp_7_1_standard")
    expect(std.ddp71).toBe(true)
    const bd = relevantBitrates(ctx({ channels: 8, ddpMode: "bluray" }))
    expect(bd.bitrates).toEqual(table.ddp_71_bluray)
    expect(bd.defaultKey).toBe("ddp_7_1_bluray")
    expect(bd.ddp71).toBe(true)
  })

  it("--ddp-mode only applies to a 7.1 DDP output", () => {
    for (const patch of [
      { channels: 6 },
      { channels: 8, downmix: "6" as const },
      { format: "dd" as const, channels: 8 },
      { format: "ac4" as const, channels: 8 },
    ]) {
      const info = relevantBitrates(ctx({ ...patch, ddpMode: "bluray" }))
      expect(info.ddp71).toBe(false)
      expect(info.shimBitrates).toEqual([])
      expect(info.bitrates).not.toEqual(table.ddp_71_bluray)
    }
  })

  it("downmixes DD and AC4 from 7.1 to 5.1 by default", () => {
    expect(
      relevantBitrates(ctx({ format: "dd", channels: 8 })).bitrates
    ).toEqual(table.dd_51)
    expect(
      relevantBitrates(ctx({ format: "ac4", channels: 8 })).bitrates
    ).toEqual(table.ac4_51)
    // An explicit downmix wins.
    expect(
      relevantBitrates(ctx({ format: "dd", channels: 8, downmix: "2" }))
        .bitrates
    ).toEqual(table.dd_20)
    expect(
      relevantBitrates(ctx({ format: "ac4", channels: 8, downmix: "2" }))
        .bitrates
    ).toEqual(table.ac4_20)
  })

  it("uses the downmix as the output layout", () => {
    expect(
      relevantBitrates(ctx({ channels: 8, downmix: "6" })).bitrates
    ).toEqual(table.ddp_51)
    expect(
      relevantBitrates(ctx({ channels: 6, downmix: "1" })).bitrates
    ).toEqual(table.ddp_10)
    expect(
      relevantBitrates(ctx({ format: "ac4", channels: 6, downmix: "2" }))
        .bitrates
    ).toEqual(table.ac4_20)
  })

  it("ignores a downmix that is not lower than the input channels", () => {
    const info = relevantBitrates(ctx({ channels: 2, downmix: "6" }))
    expect(info.bitrates).toEqual(table.ddp_20)
    expect(info.warnings).toHaveLength(1)
    expect(
      relevantBitrates(ctx({ channels: 2, downmix: "2" })).warnings
    ).toHaveLength(1)
  })

  it("AAC encodes at most two channels", () => {
    for (const channels of [2, 6, 8]) {
      const info = relevantBitrates(ctx({ format: "aac", channels }))
      expect(info.bitrates).toEqual(table.aac_20)
      expect(info.defaultKey).toBe("aac_2_0")
    }
    // Only -dm 2 exists; 1 and 6 are not accepted.
    for (const downmix of ["1", "6"] as const) {
      const info = relevantBitrates(
        ctx({ format: "aac", channels: 8, downmix })
      )
      expect(info.warnings).toHaveLength(1)
      expect(info.bitrates).toEqual(table.aac_20)
    }
    expect(table).not.toHaveProperty("aac_10")
    expect(TEMPLATE_DEFAULT_BITRATES).not.toHaveProperty("aac_1_0")
  })

  it("AAC rejects mono input", () => {
    const info = relevantBitrates(ctx({ format: "aac", channels: 1 }))
    expect(info.status).toBe("invalid")
    expect(info.bitrates).toEqual([])
    expect(info.placeholder).toBe("AAC needs 2.0+ input")
    expect(info.warnings).toEqual([
      "Input channels for AAC can only be 2, 5, 6, 7 or 8.",
    ])
  })

  it("AC4 rejects mono input", () => {
    const info = relevantBitrates(ctx({ format: "ac4", channels: 1 }))
    expect(info.status).toBe("invalid")
    expect(info.bitrates).toEqual([])
    expect(info.warnings.length).toBeGreaterThan(0)
  })

  it("Atmos formats have their own lists and need TrueHD", () => {
    for (const format of ["ddp-atmos", "ac4-atmos"] as const) {
      const ok = relevantBitrates(
        ctx({ format, channels: 8, codecs: ["truehd"] })
      )
      expect(ok.status).toBe("ok")
      const key = format === "ddp-atmos" ? "ddp_atmos_streaming" : "ac4_atmos"
      expect(ok.bitrates).toEqual(table[key])
      expect(ok.defaultKey).toBe(key)
      expect(ok.warnings).toEqual([])
      // Unknown codecs (not probed yet) are not a problem.
      expect(
        relevantBitrates(ctx({ format, channels: null })).warnings
      ).toEqual([])
      const bad = relevantBitrates(
        ctx({ format, channels: 6, codecs: ["eac3"] })
      )
      expect(bad.bitrates).toEqual(table[key])
      expect(bad.warnings).toEqual([ATMOS_INPUT_WARNING])
      // The channel layout does not matter.
      expect(
        relevantBitrates(ctx({ format, channels: 3, codecs: ["truehd"] }))
          .warnings
      ).toEqual([])
    }
  })

  it("has no bitrate for TrueHD", () => {
    const info = relevantBitrates(ctx({ format: "thd" }))
    expect(info).toMatchObject({
      status: "na",
      bitrates: [],
      placeholder: "N/A for TrueHD",
    })
    expect(info.warnings).toEqual([])
    expect(
      relevantBitrates(ctx({ format: "thd", channels: 1 })).warnings
    ).toHaveLength(1)
  })

  it("accepts TrueHD downmixes 2 and 6, and warns about the rest", () => {
    const w = (downmix: Downmix, channels: number | null) =>
      relevantBitrates(ctx({ format: "thd", channels, downmix })).warnings
    expect(w("2", 8)).toEqual([])
    expect(w("6", 8)).toEqual([])
    expect(w("2", null)).toEqual([])
    expect(w("6", 6)).toEqual([
      "Downmix has to be lower than the input channels.",
    ])
    expect(w("2", 2)).toHaveLength(1)
    expect(w("1", 8)).toEqual(["Downmix 1 is not supported for this format."])
    expect(
      buildArgs(opts({ format: "thd", downmix: "6" }), { channels: 8 })
    ).toEqual(["-f", "thd", "-dm", "6"])
    // switching to TrueHD keeps 2 and 6 only
    expect(setFormat(opts({ downmix: "2" }), "thd").downmix).toBe("2")
    expect(setFormat(opts({ downmix: "1" }), "thd").downmix).toBe("none")
    expect(setFormat(opts({ downmix: "2" }), "thd-atmos").downmix).toBe("none")
  })

  it("rejects unsupported channel counts", () => {
    for (const channels of [0, 3, 4, 9]) {
      const info = relevantBitrates(ctx({ channels }))
      expect(info.status).toBe("invalid")
      expect(info.bitrates).toEqual([])
      expect(info.warnings).toEqual([CHANNELS_WARNING])
    }
  })

  it("falls back to all bitrates of the format without a layout", () => {
    for (const format of ["dd", "ddp", "ac4", "aac"] as const) {
      const info = relevantBitrates(ctx({ format, channels: null }))
      expect(info.status).toBe("fallback")
      expect(info.bitrates).toEqual(bitrateOptions(format))
    }
  })

  it("every result is a subset of the format's bitrates", () => {
    for (const format of FORMATS) {
      for (const channels of [1, 2, 6, 8]) {
        for (const downmix of ["none", "1", "2", "6"] as const) {
          const all = bitrateOptions(format)
          const info = relevantBitrates(ctx({ format, channels, downmix }))
          for (const b of info.bitrates) expect(all).toContain(b)
        }
      }
    }
  })
})

describe("relevantBitrates defaultKey", () => {
  const key = (patch: Partial<BitrateContext>) =>
    relevantBitrates({
      format: "ddp",
      channels: 6,
      downmix: "none",
      ...patch,
    }).defaultKey

  it("names the output layout", () => {
    expect(key({ format: "dd", channels: 1 })).toBe("dd_1_0")
    expect(key({ format: "dd", channels: 2 })).toBe("dd_2_0")
    expect(key({ format: "dd", channels: 6 })).toBe("dd_5_1")
    expect(key({ format: "ddp", channels: 8 })).toBe("ddp_7_1_standard")
    expect(key({ format: "ac4", channels: 2 })).toBe("ac4_2_0")
    expect(key({ format: "ac4", channels: 6 })).toBe("ac4_5_1")
    expect(key({ format: "ac4-atmos" })).toBe("ac4_atmos")
  })

  it("follows the downmix and the DD / AC4 7.1 reduction", () => {
    expect(key({ format: "dd", channels: 8 })).toBe("dd_5_1")
    expect(key({ format: "ac4", channels: 8 })).toBe("ac4_5_1")
    expect(key({ channels: 8, downmix: "2" })).toBe("ddp_2_0")
    expect(key({ channels: 6, downmix: "1" })).toBe("ddp_1_0")
  })

  it("is unset without a known valid layout", () => {
    expect(key({ channels: null })).toBeUndefined()
    expect(key({ channels: 3 })).toBeUndefined()
    expect(key({ format: "thd", channels: 6 })).toBeUndefined()
    expect(key({ format: "ac4", channels: 1 })).toBeUndefined()
  })
})

describe("relevantDownmixes", () => {
  it("offers only lower channel counts", () => {
    expect(relevantDownmixes("ddp", 1)).toEqual(["none"])
    expect(relevantDownmixes("ddp", 2)).toEqual(["none", "1"])
    expect(relevantDownmixes("dd", 6)).toEqual(["none", "1", "2"])
    expect(relevantDownmixes("ddp", 8)).toEqual(["none", "1", "2", "6"])
  })
  it("offers everything the format accepts when the channels are unknown", () => {
    expect(relevantDownmixes("dd", null)).toEqual(["none", "1", "2", "6"])
    expect(relevantDownmixes("ddp", null)).toEqual(["none", "1", "2", "6"])
    expect(relevantDownmixes("ac4", null)).toEqual(["none", "2", "6"])
    expect(relevantDownmixes("aac", null)).toEqual(["none", "2"])
  })
  it("follows the format", () => {
    expect(relevantDownmixes("ac4", 2)).toEqual(["none"])
    expect(relevantDownmixes("ac4", 8)).toEqual(["none", "2", "6"])
    expect(relevantDownmixes("aac", 6)).toEqual(["none", "2"])
    expect(relevantDownmixes("aac", 2)).toEqual(["none"])
    for (const format of ["ddp-atmos", "ac4-atmos"] as const) {
      expect(relevantDownmixes(format, 8)).toEqual(["none"])
      expect(relevantDownmixes(format, null)).toEqual(["none"])
    }
  })
})

describe("bitrateChoices", () => {
  const ctx = {
    format: "ddp" as const,
    channels: 6,
    downmix: "none" as const,
  }
  const info = relevantBitrates(ctx)

  it("puts a config value at its position", () => {
    const c = bitrateChoices(info, { ddp_5_1: 768 })
    const i = c.findIndex((x) => x.value === "default")
    expect(c[i].label).toBe("768 kbps (default from config)")
    expect(c[i - 1].label).toBe("704 kbps")
    expect(c[i + 1].label).toBe("832 kbps")
    expect(c).toHaveLength(info.bitrates.length)
    expect(c.filter((x) => x.label.startsWith("768 "))).toHaveLength(1)
    expect(c.some((x) => x.value === "768")).toBe(false)
  })

  it("rounds the config value to the closest, first wins ties", () => {
    // 260 is between 256 and 272 (closer to 256); 280 ties 272/288.
    const a = bitrateChoices(info, { ddp_5_1: 260 })
    expect(a.find((x) => x.value === "default")?.label).toContain("256 kbps")
    const b = bitrateChoices(info, { ddp_5_1: 280 })
    expect(b.find((x) => x.value === "default")?.label).toContain("272 kbps")
  })

  it("uses the template default without config", () => {
    const c = bitrateChoices(info, {})
    expect(c.find((x) => x.value === "default")?.label).toBe(
      "1024 kbps (default)"
    )
    const atmos = bitrateChoices(
      relevantBitrates({ ...ctx, format: "ddp-atmos" }),
      {}
    )
    expect(atmos.find((x) => x.value === "default")?.label).toBe(
      "1024 kbps (default)"
    )
  })

  it("has a template key for every default key", () => {
    for (const format of FORMATS)
      for (const channels of [1, 2, 6, 8])
        for (const downmix of ["none", "1", "2", "6"] as const) {
          const i = relevantBitrates({ format, channels, downmix })
          if (i.defaultKey)
            expect(TEMPLATE_DEFAULT_BITRATES).toHaveProperty(i.defaultKey)
        }
  })

  it("keeps a top default entry without a known default", () => {
    const fb = relevantBitrates({ ...ctx, channels: null })
    const c = bitrateChoices(fb, { ddp_5_1: 768 })
    expect(c[0]).toEqual({ value: "default", label: "default (from config)" })
    expect(c).toHaveLength(fb.bitrates.length + 1)
    const na = bitrateChoices(relevantBitrates({ ...ctx, format: "thd" }), {})
    expect(na).toEqual([{ value: "default", label: "N/A for TrueHD" }])
  })
})

describe("tomlKey", () => {
  it("keeps underscores for the config keys", () => {
    expect(tomlKey("ddp-atmos")).toBe("ddp_atmos")
    expect(tomlKey("ac4")).toBe("ac4")
  })
})
