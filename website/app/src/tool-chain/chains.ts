export type Kind = "tool" | "file" | "step"

export type FlowItem =
  | { type: "chip"; kind: Kind; text: string; opt?: boolean }
  | { type: "arrow"; text: string }
  | { type: "alt"; rows: FlowItem[][] }

export type Chain = { title: string; note: string; flow: FlowItem[] }

const tool = (text: string, opt?: boolean): FlowItem => ({
  type: "chip",
  kind: "tool",
  text,
  opt,
})
const file = (text: string, opt?: boolean): FlowItem => ({
  type: "chip",
  kind: "file",
  text,
  opt,
})
const step = (text: string, opt?: boolean): FlowItem => ({
  type: "chip",
  kind: "step",
  text,
  opt,
})
const pcm = (): FlowItem => file("PCM pipe")
const arrow = (text = "→"): FlowItem => ({ type: "arrow", text })
const alt = (rows: FlowItem[][]): FlowItem => ({ type: "alt", rows })

export const chains: Chain[] = [
  {
    title: "dd · ddp ≤ 5.1 · ac4 · aac",
    note: "DEE measures while encoding · NAME.wav instead of the pipe with pipe = false, and on Windows with pipe = 'auto' when 2 or more encodes run at once",
    flow: [
      file("IN"),
      arrow(),
      tool("ffmpeg"),
      arrow("raw PCM →"),
      pcm(),
      arrow(),
      tool("dee_ddp / ac4 / aac_encoder"),
      arrow(),
      file("OUT"),
    ],
  },
  {
    title: "ac4 with -dn",
    note: "separate measurement, deew writes the dialnorm into the manifest · ffmpeg pipes the audio into the measurement and again into the encoder",
    flow: [
      file("IN"),
      arrow(),
      tool("ffmpeg"),
      arrow("raw PCM →"),
      tool("dee_measure_loudness"),
      arrow(),
      file("loudness.json"),
      arrow(),
      step("deew sets dialnorm"),
      arrow("→ --input-manifest →"),
      tool("DEE encoder (fed by a second ffmpeg)"),
      arrow(),
      file("OUT"),
    ],
  },
  {
    title: "dd · ddp · thd · ddp-atmos · thd-atmos with -dn",
    note: "no measurement: the encoder gets ‑‑loudness-management skip:dialnorm=N · DD, DDP piped, TrueHD a wav",
    flow: [
      file("IN"),
      arrow(),
      tool("ffmpeg / truehdd decode"),
      arrow(),
      file("PCM pipe / NAME.wav / .atmos"),
      arrow("→ skip:dialnorm=N →"),
      tool("DEE encoder"),
      arrow(),
      file("OUT"),
    ],
  },
  {
    title: "-mo",
    note: "2.0 and 5.1 · no encoder runs · ignores ‑f, ‑dm, ‑r, ‑dn",
    flow: [
      file("IN"),
      arrow(),
      tool("ffmpeg"),
      arrow("raw PCM →"),
      pcm(),
      arrow(),
      tool("dee_measure_loudness"),
      arrow(),
      file("loudness.json"),
      arrow(),
      step("loudness in the bar"),
    ],
  },
  {
    title: "-mo with 7.1",
    note: "dee_shim measure_loudness --layout 7.1 · DEE's own meter refuses 8 channels",
    flow: [
      file("IN"),
      arrow(),
      tool("ffmpeg"),
      arrow("raw PCM →"),
      pcm(),
      arrow(),
      tool("dee_shim measure_loudness"),
      arrow("--binary →"),
      tool("dee_measure_loudness"),
      arrow("measured loudness →"),
      step("loudness in the bar"),
    ],
  },
  {
    title: "-mo with a TrueHD Atmos source",
    note: "measured as Atmos · raw .thd or a container track",
    flow: [
      alt([
        [file("IN.thd"), arrow()],
        [file("IN.mkv/m2ts"), arrow(), tool("ffmpeg copy"), arrow("pipe →")],
      ]),
      tool("truehdd decode"),
      arrow(),
      file("NAME.atmos"),
      arrow(),
      tool("dee_measure_loudness"),
      arrow("measured loudness →"),
      step("loudness in the bar"),
    ],
  },
  {
    title: "-mo with an Atmos master",
    note: "ADM BWF or DAMF · read by DEE as it is, no ffprobe, ffmpeg or truehdd",
    flow: [
      file("IN.wav (ADM BWF) / IN.atmos (DAMF)"),
      arrow("atmos_mezz →"),
      tool("dee_measure_loudness"),
      arrow("measured loudness →"),
      step("loudness in the bar"),
    ],
  },
  {
    title: "ddp 7.1 --ddp-mode bluray",
    note: "Blu-ray profile, native",
    flow: [
      file("IN"),
      arrow(),
      tool("ffmpeg"),
      arrow("raw PCM →"),
      pcm(),
      arrow(),
      tool("dee_ddp_encoder -e bluray"),
      arrow(),
      file("OUT.eb3"),
    ],
  },
  {
    title: "ddp 7.1 standard · ddp 7.0 · dd/ddp 5.0",
    note: "‑‑ddp-mode standard (the default) · 7.0 in both modes · a 5.0 source without downmix (5.0 layout, --mode dd for DD)",
    flow: [
      file("IN"),
      arrow(),
      tool("ffmpeg"),
      arrow("raw PCM →"),
      pcm(),
      arrow(),
      tool("dee_shim ddp_encoder"),
      arrow("--binary →"),
      tool("dee_ddp_encoder"),
      arrow(),
      file("OUT"),
    ],
  },
  {
    title: "thd",
    note: "dee_shim with DRC none (the default) or at 96 kHz · always a wav, never the pipe",
    flow: [
      file("IN"),
      arrow(),
      tool("ffmpeg"),
      arrow(),
      file("NAME[.thd][.dm2].wav"),
      arrow(),
      tool("dee_shim dthd_encoder", true),
      arrow(),
      tool("dee_dthd_encoder"),
      arrow(),
      file("OUT.thd"),
    ],
  },
  {
    title: "ddp-atmos",
    note: "‑‑ddp-atmos-mode streaming (5.1 JOC), standard or bluray (7.1) · raw .thd or a container track",
    flow: [
      alt([
        [file("IN.thd"), arrow()],
        [file("IN.mkv/m2ts"), arrow(), tool("ffmpeg copy"), arrow("pipe →")],
      ]),
      tool("truehdd decode"),
      arrow(),
      file("NAME.atmos"),
      arrow(),
      tool("dee_shim ddpjoc_encoder"),
      arrow("--binary →"),
      tool("dee_ddpjoc_encoder"),
      arrow(),
      file("OUT.ec3 / .eb3"),
    ],
  },
  {
    title: "thd-atmos",
    note: "16 elements, sample-exact objects · raw .thd or a container track",
    flow: [
      alt([
        [file("IN.thd"), arrow()],
        [file("IN.mkv/m2ts"), arrow(), tool("ffmpeg copy"), arrow("pipe →")],
      ]),
      tool("truehdd decode"),
      arrow(),
      file("NAME.atmos"),
      arrow(),
      tool("dee_shim dthd_encoder"),
      arrow("--binary →"),
      tool("dee_dthd_encoder"),
      arrow(),
      file("OUT.thd"),
    ],
  },
  {
    title: "ac4-atmos",
    note: "native, no dee_shim · raw .thd or a container track",
    flow: [
      alt([
        [file("IN.thd"), arrow()],
        [file("IN.mkv/m2ts"), arrow(), tool("ffmpeg copy"), arrow("pipe →")],
      ]),
      tool("truehdd decode"),
      arrow(),
      file("NAME.atmos"),
      arrow(),
      tool("dee_ac4ajoc_encoder"),
      arrow(),
      file("OUT.ac4"),
    ],
  },
  {
    title: "dd · ddp from an Atmos master",
    note: "5.1 render by DEE (-dm 2 / 1: stereo / mono) · the master is read as it is, no ffmpeg, no truehdd, nothing in TEMP",
    flow: [
      file("IN.wav (ADM BWF) / IN.atmos (DAMF)"),
      arrow("--input-format atmos_mezz →"),
      tool("dee_ddp_encoder"),
      arrow(),
      file("OUT"),
    ],
  },
  {
    title: "ddp-atmos · thd-atmos · ac4-atmos from an Atmos master",
    note: "no truehdd, no ffmpeg · the same encoder commands as with a TrueHD Atmos source, with the master as -i",
    flow: [
      file("IN.wav (ADM BWF) / IN.atmos (DAMF)"),
      arrow("atmos_mezz →"),
      tool("dee_shim ddpjoc / dthd_encoder, or dee_ac4ajoc_encoder"),
      arrow(),
      file("OUT"),
    ],
  },
  {
    title: "ac4-ims from 5.1 / 7.1",
    note: "immersive stereo from a 5.1 wav · 7.x is downmixed to 5.1 by ffmpeg, 5.0 gets a silent LFE · always a wav, never the pipe · no manifest, no --temp-dir",
    flow: [
      file("IN"),
      arrow(),
      tool("ffmpeg"),
      arrow(),
      file("NAME[.dm6].wav"),
      arrow(),
      tool("dee_ac4ims_encoder"),
      arrow(),
      file("OUT.ac4"),
    ],
  },
  {
    title: "ac4-ims from TrueHD Atmos",
    note: "raw .thd or a container track · decoded by truehdd",
    flow: [
      alt([
        [file("IN.thd"), arrow()],
        [file("IN.mkv/m2ts"), arrow(), tool("ffmpeg copy"), arrow("pipe →")],
      ]),
      tool("truehdd decode"),
      arrow(),
      file("NAME.atmos"),
      arrow(),
      tool("dee_ac4ims_encoder"),
      arrow(),
      file("OUT.ac4"),
    ],
  },
  {
    title: "ac4-ims from an Atmos master",
    note: "ADM BWF or DAMF · read by DEE as it is",
    flow: [
      file("IN.wav (ADM BWF) / IN.atmos (DAMF)"),
      arrow("atmos_mezz →"),
      tool("dee_ac4ims_encoder"),
      arrow(),
      file("OUT.ac4"),
    ],
  },
]

export type LoudnessRow = { cond: string; flow: FlowItem[]; note?: string }
export type LoudnessTree = { option: string; rows: LoudnessRow[] }

export const loudnessTrees: LoudnessTree[] = [
  {
    option: "-mo",
    rows: [
      {
        cond: "≤ 5.1",
        flow: [
          pcm(),
          arrow(),
          tool("dee_measure_loudness"),
          arrow(),
          file("loudness.json"),
          arrow(),
          step("loudness in the bar"),
        ],
      },
      {
        cond: "7.1",
        flow: [
          pcm(),
          arrow(),
          tool("dee_shim measure_loudness --layout 7.1"),
          arrow("--binary →"),
          tool("dee_measure_loudness"),
          arrow("measured loudness →"),
          step("loudness in the bar"),
        ],
      },
      {
        cond: "TrueHD Atmos",
        flow: [
          file("NAME.atmos"),
          arrow(),
          tool("dee_measure_loudness"),
          arrow("measured loudness →"),
          step("loudness in the bar"),
        ],
        note: "decoded by truehdd",
      },
      {
        cond: "Atmos master",
        flow: [
          file("IN (ADM BWF / DAMF)"),
          arrow(),
          tool("dee_measure_loudness"),
          arrow("measured loudness →"),
          step("loudness in the bar"),
        ],
        note: "read as it is, no truehdd",
      },
    ],
  },
  {
    option: "-dn",
    rows: [
      {
        cond: "DD · DDP · DDP Atmos · TrueHD · TrueHD Atmos",
        flow: [
          step("--loudness-management"),
          step("skip:dialnorm=N"),
          arrow(),
          tool("DEE encoder"),
        ],
        note: "no measurement",
      },
      {
        cond: "AC4",
        flow: [
          tool("dee_measure_loudness"),
          arrow(),
          file("loudness.json"),
          arrow(),
          step("deew writes the dialnorm"),
          arrow(),
          tool("dee_ac4_encoder --input-manifest"),
        ],
        note: "7.1 input is rejected",
      },
    ],
  },
]
