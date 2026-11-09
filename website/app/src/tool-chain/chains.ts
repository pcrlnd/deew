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
const arrow = (text = "→"): FlowItem => ({ type: "arrow", text })
const alt = (rows: FlowItem[][]): FlowItem => ({ type: "alt", rows })

export const chains: Chain[] = [
  {
    title: "dd · ddp ≤ 5.1 · ac4 · aac",
    note: "DEE measures while encoding",
    flow: [
      file("IN"),
      arrow(),
      tool("ffmpeg"),
      arrow(),
      file("NAME.wav"),
      arrow(),
      tool("dee_ddp / ac4 / aac_encoder"),
      arrow(),
      file("OUT"),
    ],
  },
  {
    title: "any channel format with -dn",
    note: "separate measurement, deew writes the dialnorm",
    flow: [
      file("IN"),
      arrow(),
      tool("ffmpeg"),
      arrow(),
      file("NAME.wav"),
      arrow(),
      tool("dee_measure_loudness"),
      arrow(),
      file("loudness.json"),
      arrow(),
      step("deew sets dialnorm"),
      arrow("→ --input-manifest →"),
      tool("DEE encoder"),
      arrow(),
      file("OUT"),
    ],
  },
  {
    title: "-mo",
    note: "no encoder runs",
    flow: [
      file("IN"),
      arrow(),
      tool("ffmpeg"),
      arrow(),
      file("NAME.wav"),
      arrow(),
      tool("dee_measure_loudness"),
      arrow(),
      file("loudness.json"),
      arrow(),
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
      arrow(),
      file("NAME.wav"),
      arrow(),
      tool("dee_ddp_encoder -e bluray"),
      arrow(),
      file("OUT.eb3"),
    ],
  },
  {
    title: "ddp 7.1 standard · ddp 7.0",
    note: "--ddp-mode standard (the default) · 7.0 in both modes",
    flow: [
      file("IN"),
      arrow(),
      tool("ffmpeg"),
      arrow(),
      file("NAME.wav"),
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
    note: "dee_shim only with DRC none (the default)",
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
    note: "--ddp-atmos-mode streaming (5.1 JOC), standard or bluray (7.1) · raw .thd or a container track",
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
]
