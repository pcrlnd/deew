import {
  Children,
  isValidElement,
  useEffect,
  useRef,
  useState,
  type ReactNode,
} from "react"
import {
  AudioWaveformIcon,
  BlocksIcon,
  BoxIcon,
  CopyIcon,
  CpuIcon,
  FileAudioIcon,
  FileCheckIcon,
  FileInputIcon,
  FileJsonIcon,
  GaugeIcon,
  PackageOpenIcon,
  PuzzleIcon,
  Rows3Icon,
  ScanSearchIcon,
  SlidersHorizontalIcon,
  TerminalIcon,
  WorkflowIcon,
  type LucideIcon,
} from "lucide-react"

import { SiteFooter, SiteHeader } from "@/components/site-shell"
import { chains, loudnessTrees } from "./tool-chain/chains"
import { Chip } from "./tool-chain/chip"
import "./tool-chain/tool-chain.css"

const ICON_SIZE = 14
const ICON_GAP = 5

/**
 * A node of the overview diagram: the block's own shapes, its name and a
 * lucide icon placed in front of the name. Once fonts are loaded the name is
 * measured and icon + name are centred together on the original x.
 */
function DiagramNode({
  kind,
  icon: Icon,
  textClass,
  x,
  y,
  name,
  children,
}: {
  kind: string
  icon: LucideIcon
  textClass: string
  x: number
  y: number
  name: string
  children?: ReactNode
}) {
  const nameRef = useRef<SVGTextElement>(null)
  const [start, setStart] = useState<number | null>(null)

  useEffect(() => {
    let cancelled = false
    const place = () => {
      const el = nameRef.current
      if (cancelled || !el) return
      const width = el.getComputedTextLength()
      setStart(x - (ICON_SIZE + ICON_GAP + width) / 2)
    }
    if (document.fonts) void document.fonts.ready.then(place)
    else place()
    return () => {
      cancelled = true
    }
  }, [x, name])

  // Keep the original DOM order: shape, name, sub texts.
  const kids = Children.toArray(children)
  const hasShape = isValidElement(kids[0]) && kids[0].type === "rect"
  const shape = hasShape ? kids[0] : null
  const subs = hasShape ? kids.slice(1) : kids

  return (
    <g className={"node " + kind}>
      {shape}
      <text
        ref={nameRef}
        className={textClass + " t-name"}
        x={start === null ? x : start + ICON_SIZE + ICON_GAP}
        y={y}
        textAnchor={start === null ? "middle" : "start"}
      >
        {name}
      </text>
      {subs}
      {start !== null && (
        <Icon
          className="ico"
          x={start}
          y={y - ICON_SIZE + 2}
          width={ICON_SIZE}
          height={ICON_SIZE}
        />
      )}
    </g>
  )
}

function ChainList() {
  return (
    <div className="chains">
      {chains.map((c) => (
        <div className="chain" key={c.title}>
          <div className="when">
            <b>{c.title}</b>
            <small>{c.note}</small>
          </div>
          <div className="flow">
            {c.flow.map((item, i) => (
              <Chip key={i} item={item} />
            ))}
          </div>
        </div>
      ))}
    </div>
  )
}

function LoudnessTree() {
  return (
    <div className="ltrees">
      {loudnessTrees.map((t) => (
        <div className="ltree" key={t.option}>
          <div className="lt-root">
            <Chip item={{ type: "chip", kind: "step", text: t.option }} />
          </div>
          <div className="lt-rows">
            {t.rows.map((r) => (
              <div className="lt-row" key={r.cond}>
                <div className="lt-cond">{r.cond}</div>
                <div className="flow">
                  {r.flow.map((item, i) => (
                    <Chip key={i} item={item} />
                  ))}
                </div>
                {r.note && <div className="lt-note">{r.note}</div>}
              </div>
            ))}
          </div>
        </div>
      ))}
    </div>
  )
}

export default function ToolChain() {
  return (
    <div className="min-h-screen">
      <SiteHeader current="tool-chain" />
      <div className="tc">
        <div className="wrap">
          <header>
            <div className="eyebrow">
              deew {__DEEW_VERSION__} · how the tools are wired
            </div>
            <h1>deew tool chain</h1>
            <p className="lede">
              deew itself never touches audio. It probes the input, then chains
              ffmpeg, truehdd, dee_shim and the DEE command line tools through
              pipes and temp files, with the options shown below. Placeholders
              are in capitals: <code>IN</code>, <code>TI</code> (track index),{" "}
              <code>TEMP/NAME</code>, <code>OUT</code>, <code>B</code>{" "}
              (bitrate), <code>P</code> (DRC profile).
            </p>
            <div className="legend" aria-label="Legend">
              <span>
                <i className="sw tool" />
                external tool
              </span>
              <span>
                <i className="sw file" />
                file / data
              </span>
              <span>
                <i className="sw step" />
                deew
              </span>
              <span>
                <i className="sw opt" />
                only in some cases
              </span>
            </div>
          </header>

          <section>
            <div className="sec-head">
              <h2>
                <WorkflowIcon />
                The whole chain
              </h2>
              <p>
                Three lanes: probing (always), the channel path (DD, DDP,
                TrueHD, AC4, AAC) and the Atmos path (ddp-atmos, thd-atmos,
                ac4-atmos). Both paths end in one DEE encoder, wrapped in
                dee_shim when the format needs it.
              </p>
            </div>
            <div className="diagram">
              <svg
                className="dg"
                viewBox="0 0 1120 450"
                role="img"
                aria-label="Block diagram: input file to ffprobe; input to ffmpeg, whose raw PCM goes through a pipe (NAME.wav for TrueHD, AC4 with -dn and pipe = false) to the DEE encoder, and to an optional dee_measure_loudness and loudness.json; input through an optional ffmpeg copy pipe to truehdd decode to NAME.atmos, then to the DEE encoder; dee_shim optionally wraps the encoder; the encoder writes the output."
              >
                <defs>
                  <marker
                    id="ah"
                    viewBox="0 0 10 10"
                    refX="9"
                    refY="5"
                    markerWidth="7"
                    markerHeight="7"
                    orient="auto-start-reverse"
                  >
                    <path className="arrowhead" d="M0,0 L10,5 L0,10 z" />
                  </marker>
                </defs>
                <path
                  className="wire"
                  markerEnd="url(#ah)"
                  d="M130,185 C152,185 144,45 166,45 L178,45"
                />
                <path
                  className="wire"
                  markerEnd="url(#ah)"
                  d="M130,205 L178,205"
                />
                <path
                  className="wire"
                  markerEnd="url(#ah)"
                  d="M130,230 C152,230 150,290 178,290 L405,290 L405,318"
                />
                <path
                  className="wire"
                  markerEnd="url(#ah)"
                  d="M75,240 L75,335 Q75,355 95,355 L178,355"
                />
                <path
                  className="wire"
                  markerEnd="url(#ah)"
                  d="M320,45 L368,45"
                />
                <path
                  className="wire"
                  markerEnd="url(#ah)"
                  d="M500,45 L548,45"
                />
                <path
                  className="wire"
                  markerEnd="url(#ah)"
                  d="M320,205 L368,205"
                />
                <path
                  className="wire opt"
                  markerEnd="url(#ah)"
                  d="M500,192 C516,192 510,125 526,125 L538,125"
                />
                <path
                  className="wire opt"
                  markerEnd="url(#ah)"
                  d="M730,125 L758,125"
                />
                <path
                  className="wire opt"
                  markerEnd="url(#ah)"
                  d="M890,125 C902,125 896,168 906,168 L918,168"
                />
                <path
                  className="wire"
                  markerEnd="url(#ah)"
                  d="M500,212 L918,212"
                />
                <path
                  className="wire"
                  markerEnd="url(#ah)"
                  d="M320,355 L366,355"
                />
                <path
                  className="wire"
                  markerEnd="url(#ah)"
                  d="M500,355 L548,355"
                />
                <path
                  className="wire"
                  markerEnd="url(#ah)"
                  d="M720,355 L770,355 C800,355 790,250 820,250 L918,250"
                />
                <path
                  className="wire"
                  markerEnd="url(#ah)"
                  d="M1050,270 L1050,378"
                />
                <text className="lbl" x="344" y="36" textAnchor="middle">
                  JSON
                </text>
                <text className="lbl" x="344" y="196" textAnchor="middle">
                  PCM
                </text>
                <text className="lbl" x="700" y="204" textAnchor="middle">
                  -i /dev/stdin (Windows: named_pipe://\\.\pipe\deew-PID-JOB)
                </text>
                <text className="lbl" x="700" y="228" textAnchor="middle">
                  --input-format pcm_s24le:…
                </text>
                <text className="lbl" x="510" y="246" textAnchor="start">
                  else -i NAME.wav (--input-format wav)
                </text>
                <text className="lbl" x="343" y="346" textAnchor="middle">
                  pipe
                </text>
                <text className="lbl" x="524" y="346" textAnchor="middle">
                  writes
                </text>
                <text className="lbl" x="860" y="242" textAnchor="middle">
                  -i NAME.atmos
                </text>
                <text className="lbl" x="290" y="282" textAnchor="middle">
                  raw .thd: truehdd reads IN directly
                </text>
                <text className="lbl" x="128" y="347" textAnchor="middle">
                  container
                </text>
                <text className="lbl" x="1062" y="330" textAnchor="start">
                  -o
                </text>

                <DiagramNode
                  kind="file"
                  icon={FileInputIcon}
                  textClass="t-file"
                  x={75}
                  y={198}
                  name="IN"
                >
                  <rect
                    className="b-file"
                    x="20"
                    y="170"
                    width="110"
                    height="70"
                  />
                  <text
                    className="t-file t-sub"
                    x="75"
                    y="216"
                    textAnchor="middle"
                  >
                    wav, flac, ac3,
                  </text>
                  <text
                    className="t-file t-sub"
                    x="75"
                    y="230"
                    textAnchor="middle"
                  >
                    thd, mkv, m2ts…
                  </text>
                </DiagramNode>

                <DiagramNode
                  kind="tool"
                  icon={ScanSearchIcon}
                  textClass="t-tool"
                  x={250}
                  y={42}
                  name="ffprobe"
                >
                  <rect
                    className="b-tool"
                    x="180"
                    y="20"
                    width="140"
                    height="50"
                  />
                  <text
                    className="t-tool t-sub"
                    x="250"
                    y="59"
                    textAnchor="middle"
                  >
                    every input
                  </text>
                </DiagramNode>
                <DiagramNode
                  kind="file"
                  icon={FileJsonIcon}
                  textClass="t-file"
                  x={435}
                  y={42}
                  name="stream info"
                >
                  <rect
                    className="b-file"
                    x="370"
                    y="20"
                    width="130"
                    height="50"
                  />
                  <text
                    className="t-file t-sub"
                    x="435"
                    y="59"
                    textAnchor="middle"
                  >
                    codec, channels, rate
                  </text>
                </DiagramNode>
                <DiagramNode
                  kind="step"
                  icon={TerminalIcon}
                  textClass="t-step"
                  x={635}
                  y={42}
                  name="deew"
                >
                  <rect
                    className="b-step"
                    x="550"
                    y="20"
                    width="170"
                    height="50"
                  />
                  <text
                    className="t-step t-sub"
                    x="635"
                    y="59"
                    textAnchor="middle"
                  >
                    checks, builds every command
                  </text>
                </DiagramNode>

                <DiagramNode
                  kind="tool"
                  icon={AudioWaveformIcon}
                  textClass="t-tool"
                  x={250}
                  y={198}
                  name="ffmpeg"
                >
                  <rect
                    className="b-tool"
                    x="180"
                    y="170"
                    width="140"
                    height="70"
                  />
                  <text
                    className="t-tool t-sub"
                    x="250"
                    y="216"
                    textAnchor="middle"
                  >
                    pan / downmix /
                  </text>
                  <text
                    className="t-tool t-sub"
                    x="250"
                    y="230"
                    textAnchor="middle"
                  >
                    resample → PCM
                  </text>
                </DiagramNode>
                <DiagramNode
                  kind="file"
                  icon={FileAudioIcon}
                  textClass="t-file"
                  x={435}
                  y={202}
                  name="PCM pipe"
                >
                  <rect
                    className="b-file"
                    x="370"
                    y="180"
                    width="130"
                    height="50"
                  />
                  <text
                    className="t-file t-sub"
                    x="435"
                    y="219"
                    textAnchor="middle"
                  >
                    or NAME.wav
                  </text>
                </DiagramNode>
                <a href="#loudness">
                  <DiagramNode
                    kind="tool"
                    icon={GaugeIcon}
                    textClass="t-tool"
                    x={635}
                    y={122}
                    name="dee_measure_loudness"
                  >
                    <rect
                      className="b-tool dash"
                      x="540"
                      y="100"
                      width="190"
                      height="50"
                    />
                    <text
                      className="t-tool t-sub"
                      x="635"
                      y="139"
                      textAnchor="middle"
                    >
                      -mo / AC4 -dn · see Loudness
                    </text>
                  </DiagramNode>
                </a>
                <DiagramNode
                  kind="file"
                  icon={FileJsonIcon}
                  textClass="t-file"
                  x={825}
                  y={122}
                  name="loudness.json"
                >
                  <rect
                    className="b-file dash"
                    x="760"
                    y="100"
                    width="130"
                    height="50"
                  />
                  <text
                    className="t-file t-sub"
                    x="825"
                    y="139"
                    textAnchor="middle"
                  >
                    → --input-manifest
                  </text>
                </DiagramNode>

                <DiagramNode
                  kind="tool"
                  icon={CopyIcon}
                  textClass="t-tool"
                  x={250}
                  y={348}
                  name="ffmpeg"
                >
                  <rect
                    className="b-tool"
                    x="180"
                    y="320"
                    width="140"
                    height="70"
                  />
                  <text
                    className="t-tool t-sub"
                    x="250"
                    y="366"
                    textAnchor="middle"
                  >
                    copy track TI
                  </text>
                  <text
                    className="t-tool t-sub"
                    x="250"
                    y="380"
                    textAnchor="middle"
                  >
                    mkv / mka / m2ts
                  </text>
                </DiagramNode>
                <DiagramNode
                  kind="tool"
                  icon={PackageOpenIcon}
                  textClass="t-tool"
                  x={435}
                  y={348}
                  name="truehdd"
                >
                  <rect
                    className="b-tool"
                    x="370"
                    y="320"
                    width="130"
                    height="70"
                  />
                  <text
                    className="t-tool t-sub"
                    x="435"
                    y="366"
                    textAnchor="middle"
                  >
                    decode
                  </text>
                  <text
                    className="t-tool t-sub"
                    x="435"
                    y="380"
                    textAnchor="middle"
                  >
                    presentation 3
                  </text>
                </DiagramNode>
                <DiagramNode
                  kind="file"
                  icon={BoxIcon}
                  textClass="t-file"
                  x={635}
                  y={352}
                  name="NAME.atmos"
                >
                  <rect
                    className="b-file"
                    x="550"
                    y="330"
                    width="170"
                    height="50"
                  />
                  <text
                    className="t-file t-sub"
                    x="635"
                    y="369"
                    textAnchor="middle"
                  >
                    + .metadata + .audio (DAMF)
                  </text>
                </DiagramNode>

                <rect
                  className="b-frame"
                  x="900"
                  y="88"
                  width="200"
                  height="252"
                />
                <DiagramNode
                  kind="tool"
                  icon={CpuIcon}
                  textClass="t-tool"
                  x={1000}
                  y={178}
                  name="DEE encoder"
                >
                  <rect
                    className="b-tool"
                    x="920"
                    y="150"
                    width="160"
                    height="120"
                  />
                  <text
                    className="t-tool t-sub"
                    x="1000"
                    y="198"
                    textAnchor="middle"
                  >
                    dee_ddp_encoder
                  </text>
                  <text
                    className="t-tool t-sub"
                    x="1000"
                    y="213"
                    textAnchor="middle"
                  >
                    dee_ddpjoc_encoder
                  </text>
                  <text
                    className="t-tool t-sub"
                    x="1000"
                    y="228"
                    textAnchor="middle"
                  >
                    dee_dthd_encoder
                  </text>
                  <text
                    className="t-tool t-sub"
                    x="1000"
                    y="243"
                    textAnchor="middle"
                  >
                    dee_ac4(ajoc)_encoder
                  </text>
                  <text
                    className="t-tool t-sub"
                    x="1000"
                    y="258"
                    textAnchor="middle"
                  >
                    dee_aac_encoder
                  </text>
                </DiagramNode>
                <DiagramNode
                  kind="shim"
                  icon={PuzzleIcon}
                  textClass="t-shim"
                  x={975}
                  y={300}
                  name="dee_shim"
                >
                  <text
                    className="t-shim t-sub"
                    x="975"
                    y="318"
                    textAnchor="middle"
                  >
                    wraps it when needed
                  </text>
                </DiagramNode>

                <DiagramNode
                  kind="file"
                  icon={FileCheckIcon}
                  textClass="t-file"
                  x={1000}
                  y={402}
                  name="OUT"
                >
                  <rect
                    className="b-file"
                    x="900"
                    y="380"
                    width="200"
                    height="50"
                  />
                  <text
                    className="t-file t-sub"
                    x="1000"
                    y="419"
                    textAnchor="middle"
                  >
                    .ac3 .ec3 .eb3 .thd .ac4 .aac
                  </text>
                </DiagramNode>
              </svg>
            </div>
          </section>
          <section>
            <div className="sec-head">
              <h2>
                <Rows3Icon />
                Which blocks each format uses
              </h2>
              <p>
                Left to right, as data flows. Dashed blocks only appear in the
                case noted.
              </p>
            </div>
            <ChainList />
          </section>
          <section id="loudness">
            <div className="sec-head">
              <h2>
                <GaugeIcon />
                Loudness: -mo and -dn
              </h2>
              <p>
                What deew runs for <code>-mo</code> (measure only) and{" "}
                <code>-dn</code> (custom dialnorm), by channel count and format.
                <code>-mo</code> measures the source and ignores <code>-f</code>
                , <code>-dm</code>, <code>-r</code> and <code>-dn</code>.
              </p>
            </div>
            <LoudnessTree />
            <p className="lt-foot">
              <code>-dn</code> has no effect for AAC and AC4 Atmos.{" "}
              <code>-dn 0</code> keeps the default: DEE measures while it
              encodes.
            </p>
          </section>
          <section>
            <div className="sec-head">
              <h2>
                <SlidersHorizontalIcon />
                DDP profiles
              </h2>
              <p>
                <code>--ddp-mode</code> picks the profile of 7.1 DDP (5.1 and
                below are not affected), <code>--ddp-atmos-mode</code> the
                profile of ddp-atmos. Without the option deew uses the config's{" "}
                <code>[default_modes]</code>.
              </p>
            </div>
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Option</th>
                    <th>Mode</th>
                    <th>Encoder</th>
                    <th>
                      <code>--layout</code>
                    </th>
                    <th>
                      <code>-e</code> mode
                    </th>
                    <th>Coded as</th>
                    <th>Bitrates</th>
                    <th>Ext</th>
                  </tr>
                </thead>
                <tbody>
                  <tr>
                    <td rowSpan={4}>
                      <code>--ddp-mode</code>
                    </td>
                    <td>
                      <code>standard</code> (default)
                    </td>
                    <td>dee_shim ddp_encoder</td>
                    <td>
                      <code>7.1/eac3</code>
                    </td>
                    <td>
                      <code>ddp</code>
                    </td>
                    <td>E-AC-3 independent + dependent</td>
                    <td>384–1792</td>
                    <td>.ec3</td>
                  </tr>
                  <tr>
                    <td>
                      <code>bluray</code>
                    </td>
                    <td>dee_ddp_encoder</td>
                    <td>—</td>
                    <td>
                      <code>bluray</code>
                    </td>
                    <td>AC-3 core + E-AC-3 dependent</td>
                    <td>768–1664</td>
                    <td>.eb3</td>
                  </tr>
                  <tr>
                    <td>
                      <code>standard</code>, 7.0
                    </td>
                    <td>dee_shim ddp_encoder</td>
                    <td>
                      <code>7.0/eac3</code>
                    </td>
                    <td>
                      <code>ddp</code>
                    </td>
                    <td>E-AC-3 independent + dependent</td>
                    <td>384–1792</td>
                    <td>.ec3</td>
                  </tr>
                  <tr>
                    <td>
                      <code>bluray</code>, 7.0
                    </td>
                    <td>dee_shim ddp_encoder</td>
                    <td>
                      <code>7.0</code>
                    </td>
                    <td>
                      <code>bluray</code>
                    </td>
                    <td>AC-3 core + E-AC-3 dependent</td>
                    <td>768–1664</td>
                    <td>.eb3</td>
                  </tr>
                  <tr>
                    <td rowSpan={3}>
                      <code>--ddp-atmos-mode</code>
                    </td>
                    <td>
                      <code>streaming</code> (default)
                    </td>
                    <td>dee_shim ddpjoc_encoder</td>
                    <td>
                      <code>5.1</code>
                    </td>
                    <td>
                      <code>ddpjoc</code>
                    </td>
                    <td>5.1 E-AC-3 + JOC</td>
                    <td>384–1024</td>
                    <td>.ec3</td>
                  </tr>
                  <tr>
                    <td>
                      <code>standard</code>
                    </td>
                    <td>dee_shim ddpjoc_encoder</td>
                    <td>
                      <code>7.1/eac3</code>
                    </td>
                    <td>
                      <code>bluray</code>
                    </td>
                    <td>7.1 E-AC-3 independent + dependent, Atmos</td>
                    <td>768–2048</td>
                    <td>.ec3</td>
                  </tr>
                  <tr>
                    <td>
                      <code>bluray</code>
                    </td>
                    <td>dee_shim ddpjoc_encoder</td>
                    <td>
                      <code>7.1</code>
                    </td>
                    <td>
                      <code>bluray</code>
                    </td>
                    <td>7.1 AC-3 core + E-AC-3 dependent, Atmos</td>
                    <td>1152–1664</td>
                    <td>.eb3</td>
                  </tr>
                </tbody>
              </table>
            </div>
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th>Column / term</th>
                    <th>Meaning</th>
                  </tr>
                </thead>
                <tbody>
                  <tr>
                    <td>Encoder</td>
                    <td>
                      The program deew runs. <code>dee_shim …_encoder</code>{" "}
                      loads the DEE encoder through dee_shim,{" "}
                      <code>dee_ddp_encoder</code> runs DEE directly (no
                      dee_shim needed).
                    </td>
                  </tr>
                  <tr>
                    <td>
                      <code>--layout</code>
                    </td>
                    <td>
                      dee_shim's option, before the <code>--</code>: the output
                      channel layout and how it is coded. <code>/eac3</code>{" "}
                      means an E-AC-3 independent substream instead of an AC-3
                      core; <code>7.0</code> drops the LFE. <code>—</code>: not
                      run through dee_shim, so there is no <code>--layout</code>
                      .
                    </td>
                  </tr>
                  <tr>
                    <td>
                      <code>-e</code> mode
                    </td>
                    <td>
                      DEE's own encoder mode, after the <code>--</code>: the
                      part before <code>:drc_profile=P</code>. <code>ddp</code>{" "}
                      is plain E-AC-3, <code>bluray</code> is DEE's Blu-ray
                      mode, <code>ddpjoc</code> is E-AC-3 with Atmos objects.
                    </td>
                  </tr>
                  <tr>
                    <td>AC-3 core + E-AC-3 dependent</td>
                    <td>
                      The Blu-ray structure: a 5.1 AC-3 (Dolby Digital) stream
                      that every Dolby decoder can play, plus an E-AC-3 stream
                      with the extra channels for DDP decoders.
                    </td>
                  </tr>
                  <tr>
                    <td>E-AC-3 independent + dependent</td>
                    <td>
                      No AC-3 core: the 5.1 base is E-AC-3 too, plus a dependent
                      E-AC-3 stream with the extra channels. Needs a DDP
                      decoder, but allows lower bitrates.
                    </td>
                  </tr>
                  <tr>
                    <td>JOC</td>
                    <td>
                      Joint Object Coding: Atmos objects carried inside a 5.1
                      E-AC-3 stream, the format of streaming services.
                    </td>
                  </tr>
                  <tr>
                    <td>.ec3 / .eb3</td>
                    <td>
                      <code>.ec3</code> is an E-AC-3 stream; <code>.eb3</code>{" "}
                      marks the Blu-ray structure with an AC-3 core.
                    </td>
                  </tr>
                  <tr>
                    <td>7.0</td>
                    <td>
                      A 7.1 input with <code>-dl</code>, or a 7.0 input. It goes
                      through dee_shim in both modes.
                    </td>
                  </tr>
                </tbody>
              </table>
            </div>
            <p className="lede" style={{ fontSize: "13px" }}>
              The default bitrate of each profile is set in the config (
              <code>ddp_7_1_standard</code>, <code>ddp_7_1_bluray</code>,{" "}
              <code>ddp_atmos_streaming</code>, <code>ddp_atmos_standard</code>,{" "}
              <code>ddp_atmos_bluray</code>), <code>deew -lb</code> lists every
              allowed bitrate.
            </p>
          </section>
          <section>
            <div className="sec-head">
              <h2>
                <BlocksIcon />
                Every block and its options
              </h2>
              <p>
                The exact argument lists deew builds, in order. <code>-la</code>{" "}
                prints the same commands with your real paths.
              </p>
            </div>
            <div className="modules">
              <article className="module">
                <div className="module-head">
                  <h3>
                    <ScanSearchIcon />
                    ffprobe
                  </h3>
                  <span className="tag">external · every input</span>
                  <p>
                    Runs once per input before anything else. Its JSON decides
                    the layout, the bitrate table, the Atmos checks and the
                    progress total.
                  </p>
                </div>
                <div className="ports">
                  <div className="port">
                    <h4>In</h4>
                    <ul>
                      <li>the input file</li>
                    </ul>
                  </div>
                  <div className="port">
                    <h4>Out → deew</h4>
                    <ul>
                      <li>
                        audio stream <code>TI</code>: codec, profile (Atmos?),
                        channels, channel layout, sample rate, bit depth
                      </li>
                      <li>duration (progress total, decode length check)</li>
                    </ul>
                  </div>
                </div>
                <div className="module-body">
                  <pre className="argv">
                    <span className="p">{"ffprobe"}</span>
                    {" -v error -print_format json -show_format -show_streams "}
                    <span className="v">{"IN"}</span>
                  </pre>
                </div>
              </article>
              <article className="module">
                <div className="module-head">
                  <h3>
                    <AudioWaveformIcon />
                    ffmpeg · convert
                  </h3>
                  <span className="tag">external · channel formats</span>
                  <p>
                    Turns track <code>TI</code> into the PCM DEE reads, in DEE's
                    channel order. By default (<code>pipe = true</code>) the raw
                    samples go straight into DEE; TrueHD, AC4 <code>-dn</code>{" "}
                    and <code>pipe = false</code> write an RF64 wav instead,
                    which is skipped when it is already in TEMP from a run with{" "}
                    <code>-k</code>.
                  </p>
                </div>
                <div className="ports">
                  <div className="port">
                    <h4>In</h4>
                    <ul>
                      <li>
                        <code>IN</code>, any format ffmpeg reads
                      </li>
                    </ul>
                  </div>
                  <div className="port">
                    <h4>Out</h4>
                    <ul>
                      <li>
                        raw PCM on <b>stdout → DEE</b> (the pipe); on
                        Windows it is written to DEE's named pipe{" "}
                        <code>\\.\pipe\deew-PID-JOB</code>
                      </li>
                      <li>
                        or <code>TEMP/NAME[.variant].wav</code>, RF64
                      </li>
                      <li>
                        stderr <code>time=</code> → progress bar
                      </li>
                    </ul>
                  </div>
                </div>
                <div className="module-body">
                  <pre className="argv">
                    <span className="p">{"ffmpeg"}</span>
                    {" -y -drc_scale 0 -i "}
                    <span className="v">{"IN"}</span>
                    {"\n       "}
                    <span className="c">{"-map 0:a:TI"}</span>
                    {"                                   "}
                    <span className="m">{"# when no filter is needed"}</span>
                    {"\n       -c pcm_s"}
                    <span className="v">{"16|24|32"}</span>
                    {"le                           "}
                    <span className="m">
                      {"# input bit depth, 32 when resampling"}
                    </span>
                    {"\n       "}
                    <span className="c">{"-filter_complex '[a:TI]PAN'"}</span>
                    {"                   "}
                    <span className="m">
                      {"# reorder / silent LFE / TrueHD downmix"}
                    </span>
                    {"\n       "}
                    <span className="c">
                      {"-filter_complex '[a:TI]PAN,aresample=resampler=soxr'"}
                    </span>
                    {"\n       "}
                    <span className="c">
                      {
                        "  -ar 48000|96000 -precision 28 -cutoff 1 -dither_scale 0"
                      }
                    </span>
                    {"   "}
                    <span className="m">{"# rate change"}</span>
                    {"\n       -rf64 always "}
                    <span className="v">{"TEMP/NAME.wav"}</span>
                    {"\n       "}
                    <span className="m">
                      {
                        "# piped: -c pcm_s24le … -f s24le - replaces the -c and -rf64 parts (pcm_f32le / f32le for a 32 bit plan); Windows: \\\\.\\pipe\\deew-PID-JOB instead of -"
                      }
                    </span>
                  </pre>
                  <div className="table-wrap">
                    <table>
                      <thead>
                        <tr>
                          <th>Case</th>
                          <th>Filter (PAN)</th>
                          <th>wav name</th>
                        </tr>
                      </thead>
                      <tbody>
                        <tr>
                          <td>mono, stereo, plain 5.1</td>
                          <td>
                            none, <code>-map 0:a:TI</code>
                          </td>
                          <td>
                            <code>NAME.wav</code>
                          </td>
                        </tr>
                        <tr>
                          <td>7.1 (ffmpeg has back before side)</td>
                          <td className="wrapcode">
                            <code>
                              pan=7.1|c0=c0|c1=c1|c2=c2|c3=c3|c4=c6|c5=c7|c6=c4|c7=c5
                            </code>
                          </td>
                          <td>
                            <code>NAME.wav</code>
                          </td>
                        </tr>
                        <tr>
                          <td>5.0 input</td>
                          <td className="wrapcode">
                            <code>
                              pan=5.1|c0=c0|c1=c1|c2=c2|c3=0*c0|c4=c3|c5=c4
                            </code>
                          </td>
                          <td>
                            <code>NAME.wav</code>
                          </td>
                        </tr>
                        <tr>
                          <td>7.0 input</td>
                          <td className="wrapcode">
                            <code>
                              pan=7.1|c0=c0|c1=c1|c2=c2|c3=0*c0|c4=c5|c5=c6|c6=c3|c7=c4
                            </code>
                          </td>
                          <td>
                            <code>NAME.wav</code>
                          </td>
                        </tr>
                        <tr>
                          <td>
                            <code>-dl</code> on TrueHD / AC4 (LFE kept silent)
                          </td>
                          <td className="wrapcode">
                            LFE slot becomes <code>0*c0</code>
                          </td>
                          <td>
                            <code>NAME.dl.wav</code>
                          </td>
                        </tr>
                        <tr>
                          <td>
                            TrueHD <code>-dm 6</code> from 7.1
                          </td>
                          <td className="wrapcode">
                            <code>
                              aformat=sample_fmts=dbl,pan=5.1|…|c4=0.5*c4+0.5*c6|c5=0.5*c5+0.5*c7
                            </code>
                          </td>
                          <td>
                            <code>NAME.dm6.wav</code>
                          </td>
                        </tr>
                        <tr>
                          <td>
                            TrueHD <code>-dm 2</code>
                          </td>
                          <td className="wrapcode">
                            <code>
                              aformat=sample_fmts=dbl,pan=stereo|c0=c0+0.7071*c2+…
                            </code>{" "}
                            → per-channel{" "}
                            <code>
                              alimiter=limit=0.98855:level=0:latency=1
                            </code>{" "}
                            → <code>join</code>
                          </td>
                          <td>
                            <code>NAME.dm2.wav</code>
                          </td>
                        </tr>
                        <tr>
                          <td>not 48 kHz (DD, DDP, AC4, AAC)</td>
                          <td>soxr to 48000</td>
                          <td>
                            <code>NAME.wav</code>
                          </td>
                        </tr>
                        <tr>
                          <td>TrueHD, not 48 / 96 kHz</td>
                          <td>soxr to 48000 (below 72 kHz) or 96000</td>
                          <td>
                            <code>NAME.thd.wav</code>
                          </td>
                        </tr>
                      </tbody>
                    </table>
                  </div>
                </div>
              </article>
              <article className="module">
                <div className="module-head">
                  <h3>
                    <CopyIcon />
                    ffmpeg · copy to pipe
                  </h3>
                  <span className="tag">external · Atmos from a container</span>
                  <p>
                    truehdd only reads raw TrueHD, so for mkv / mka / m2ts
                    ffmpeg copies the track unchanged and streams it into
                    truehdd's stdin. Not used for a raw <code>.thd</code>.
                  </p>
                </div>
                <div className="ports">
                  <div className="port">
                    <h4>In</h4>
                    <ul>
                      <li>
                        <code>IN</code> container, track <code>TI</code>
                      </li>
                    </ul>
                  </div>
                  <div className="port">
                    <h4>Out</h4>
                    <ul>
                      <li>
                        raw TrueHD on <b>stdout → truehdd stdin</b>
                      </li>
                      <li>
                        only the exit code counts (timestamp warnings are
                        ignored)
                      </li>
                    </ul>
                  </div>
                </div>
                <div className="module-body">
                  <pre className="argv">
                    <span className="p">{"ffmpeg"}</span>
                    {" -v error -nostdin -i "}
                    <span className="v">{"IN"}</span>
                    {" -map 0:a:"}
                    <span className="v">{"TI"}</span>
                    {" -c copy -f truehd "}
                    <span className="v">{"-"}</span>
                  </pre>
                </div>
              </article>
              <article className="module">
                <div className="module-head">
                  <h3>
                    <PackageOpenIcon />
                    truehdd
                  </h3>
                  <span className="tag">external · Atmos formats</span>
                  <p>
                    Decodes the Atmos presentation into a DAMF master: the
                    object audio plus its metadata. Afterwards deew checks that
                    an Atmos presentation was found and that the decoded length
                    matches ffprobe's duration.
                  </p>
                </div>
                <div className="ports">
                  <div className="port">
                    <h4>In</h4>
                    <ul>
                      <li>
                        <code>IN</code> (raw <code>.thd</code>) or{" "}
                        <code>-</code> (stdin from ffmpeg)
                      </li>
                    </ul>
                  </div>
                  <div className="port">
                    <h4>Out</h4>
                    <ul>
                      <li>
                        <code>TEMP/NAME.atmos</code>,{" "}
                        <code>.atmos.metadata</code>, <code>.atmos.audio</code>
                      </li>
                      <li>
                        <code>NAME.caf</code> instead when there is no Atmos
                        (the job fails)
                      </li>
                      <li>JSON lines on stdout → frame count, progress</li>
                    </ul>
                  </div>
                </div>
                <div className="module-body">
                  <pre className="argv">
                    <span className="p">{"truehdd"}</span>
                    {" decode --presentation 3 --json --output-path "}
                    <span className="v">{"TEMP/NAME"}</span>{" "}
                    <span className="v">{"IN|-"}</span>
                  </pre>
                </div>
              </article>
              <article className="module">
                <div className="module-head">
                  <h3>
                    <GaugeIcon />
                    dee_measure_loudness
                  </h3>
                  <span className="tag">DEE · -mo / AC4 -dn</span>
                  <p>
                    A separate loudness pass, used by <code>-mo</code> and AC4{" "}
                    <code>-dn</code> (see <a href="#loudness">Loudness</a>).
                    Doesn't take 7.1, and has no <code>--temp-dir</code>.
                  </p>
                </div>
                <div className="ports">
                  <div className="port">
                    <h4>In</h4>
                    <ul>
                      <li>
                        the PCM pipe (<code>PIPE</code>), or{" "}
                        <code>TEMP/NAME.wav</code> for AC4 <code>-dn</code>
                      </li>
                    </ul>
                  </div>
                  <div className="port">
                    <h4>Out</h4>
                    <ul>
                      <li>
                        <code>TEMP/NAME.loudness.json</code>
                      </li>
                      <li>its own 0–100% progress step, measured dB</li>
                    </ul>
                  </div>
                </div>
                <div className="module-body">
                  <pre className="argv">
                    <span className="p">{"dee_measure_loudness"}</span>
                    {" --progress 1 --overwrite 1 --input-format "}
                    <span className="v">{"PCMFMT"}</span>
                    {" -i "}
                    <span className="v">{"PIPE"}</span>
                    {"\n                     --output-manifest "}
                    <span className="v">{"TEMP/NAME.loudness.json"}</span>
                  </pre>
                </div>
              </article>
              <article className="module">
                <div className="module-head">
                  <h3>
                    <CpuIcon />
                    DEE encoders
                  </h3>
                  <span className="tag">DEE 6.7 · from the dee folder</span>
                  <p>
                    Every encode starts with{" "}
                    <code>--progress 1 --overwrite 1</code> and ends with the
                    delay and temp options. DEE's <code>[===&gt;] N %</code>{" "}
                    lines drive the bar; <code>[Source loudness]</code> shows
                    the measured dB. DD, DDP, AC4, AAC and <code>-mo</code> read
                    ffmpeg's PCM from the pipe: <code>PIPE</code> is{" "}
                    <code>/dev/stdin</code> (Windows:{" "}
                    <code>named_pipe://\\.\pipe\deew-PID-JOB</code>, which DEE
                    creates before ffmpeg is started) and <code>PCMFMT</code>{" "}
                    is{" "}
                    <code>
                      pcm_s24le:sample_rate=R:channels=N:streaming=1:sample_count=C
                    </code>{" "}
                    (<code>pcm_f32le</code> for a 32 bit plan). With{" "}
                    <code>pipe = false</code> they read{" "}
                    <code>--input-format wav -i TEMP/NAME.wav</code> instead, as
                    TrueHD and AC4 <code>-dn</code> always do. With{" "}
                    <code>-dn</code> (not AC4) the loudness option is{" "}
                    <code>--loudness-management skip:dialnorm=N</code> instead
                    of <code>measure_only</code>.
                  </p>
                </div>
                <div className="ports">
                  <div className="port">
                    <h4>In</h4>
                    <ul>
                      <li>
                        the PCM pipe, <code>NAME.wav</code> or{" "}
                        <code>NAME.atmos</code>
                      </li>
                      <li>
                        <code>loudness.json</code> with AC4 <code>-dn</code>
                      </li>
                    </ul>
                  </div>
                  <div className="port">
                    <h4>Out</h4>
                    <ul>
                      <li>
                        <code>OUT</code> in the output folder
                      </li>
                      <li>
                        AC4: <code>TEMP/NAME.ac4.json</code> manifest
                      </li>
                      <li>
                        TrueHD: <code>.mll</code> / <code>.thd.log.html</code>{" "}
                        (removed, log kept with <code>-kl</code>)
                      </li>
                    </ul>
                  </div>
                </div>
                <div className="module-body">
                  <h4>DD · DDP (≤ 5.1, and 7.1 with --ddp-mode bluray)</h4>
                  <pre className="argv">
                    <span className="p">{"dee_ddp_encoder"}</span>
                    {" --progress 1 --overwrite 1 --input-format "}
                    <span className="v">{"PCMFMT"}</span>
                    {" -i "}
                    <span className="v">{"PIPE"}</span>
                    {" -o "}
                    <span className="v">{"OUT"}</span>
                    {"\n                -e "}
                    <span className="v">{"dd|ddp|bluray"}</span>
                    {":drc_profile="}
                    <span className="v">{"P"}</span>
                    <span className="c">{"[:lfe_on=0]"}</span>
                    <span className="c">
                      {"[:surround_90deg_phase_shift=0]"}
                    </span>
                    {" --data-rate "}
                    <span className="v">{"B"}</span>
                    {"\n                "}
                    <span className="c">{"-c mono|stereo|5.1"}</span>
                    {"                      "}
                    <span className="m">
                      {"# DEE downmixes (-dm, or 7.1 → 5.1 for DD)"}
                    </span>
                    {"\n                --loudness-management measure_only "}
                    <span className="c">
                      {"[--input-manifest TEMP/NAME.loudness.json]"}
                    </span>
                    {"\n                "}
                    <span className="c">
                      {"[--start S | --add-silence S:0]"}
                    </span>
                    {" --temp-dir "}
                    <span className="v">{"TEMP/dee-PID-INDEX"}</span>
                    {"\n                "}
                    <span className="m">
                      {
                        "# -nps (--no-phase-shift) adds :surround_90deg_phase_shift=0 to -e, DD / DDP surround only"
                      }
                    </span>
                  </pre>
                  <h4>AC4</h4>
                  <pre className="argv">
                    <span className="p">{"dee_ac4_encoder"}</span>
                    {" --progress 1 --overwrite 1 --input-format "}
                    <span className="v">{"PCMFMT"}</span>
                    {" -i "}
                    <span className="v">{"PIPE"}</span>
                    {" -o "}
                    <span className="v">{"OUT"}</span>
                    {"\n                --output-manifest "}
                    <span className="v">{"TEMP/NAME.ac4.json"}</span>
                    {" --data-rate "}
                    <span className="v">{"B"}</span>
                    {" -e drc_profile="}
                    <span className="v">{"P"}</span>
                    {"\n                "}
                    <span className="c">{"-c stereo|5.1"}</span>
                    {" --loudness-management measure_only "}
                    <span className="c">{"[--input-manifest …]"}</span>
                    {"\n                "}
                    <span className="c">
                      {"[--start S | --add-silence S:0]"}
                    </span>
                    {" --temp-dir "}
                    <span className="v">{"TEMP/dee-PID-INDEX"}</span>
                  </pre>
                  <h4>AAC</h4>
                  <pre className="argv">
                    <span className="p">{"dee_aac_encoder"}</span>
                    {" --progress 1 --overwrite 1 --input-format "}
                    <span className="v">{"PCMFMT"}</span>
                    {" -i "}
                    <span className="v">{"PIPE"}</span>
                    {" -o "}
                    <span className="v">{"OUT"}</span>
                    {"\n                -e aaclc --data-rate "}
                    <span className="v">{"B"}</span>
                    {" --loudness-management skip     "}
                    <span className="m">{"# DEE downmixes to 2.0 itself"}</span>
                    {"\n                "}
                    <span className="c">
                      {"[--start S | --add-silence S:0]"}
                    </span>
                    {" --temp-dir "}
                    <span className="v">{"TEMP/dee-PID-INDEX"}</span>
                  </pre>
                  <h4>TrueHD (DRC other than none)</h4>
                  <pre className="argv">
                    <span className="p">{"dee_dthd_encoder"}</span>
                    {" --progress 1 --overwrite 1 --presentation "}
                    <span className="v">{"2ch|6ch|8ch"}</span>
                    {":drc_profile="}
                    <span className="v">{"P"}</span>
                    {"\n                 --input-format wav -i "}
                    <span className="v">{"TEMP/NAME.wav"}</span>
                    {" -o "}
                    <span className="v">{"OUT"}</span>
                    {"\n                 "}
                    <span className="c">
                      {"[--start S | --add-silence S:0]"}
                    </span>
                    {" --temp-dir "}
                    <span className="v">{"TEMP/dee-PID-INDEX"}</span>
                  </pre>
                  <h4>AC4 Atmos</h4>
                  <pre className="argv">
                    <span className="p">{"dee_ac4ajoc_encoder"}</span>
                    {" --progress 1 --overwrite 1 -i "}
                    <span className="v">{"TEMP/NAME.atmos"}</span>
                    {" -o "}
                    <span className="v">{"OUT"}</span>
                    {"\n                    --output-manifest "}
                    <span className="v">{"TEMP/NAME.ac4.json"}</span>
                    {" --level 3 --data-rate "}
                    <span className="v">{"B"}</span>
                    {" -e drc_profile="}
                    <span className="v">{"P"}</span>
                    {"\n                    "}
                    <span className="c">
                      {"[--start S | --add-silence S:0]"}
                    </span>
                    {"          "}
                    <span className="m">{"# no --temp-dir"}</span>
                  </pre>
                  <h4>-mo, TrueHD Atmos</h4>
                  <pre className="argv">
                    <span className="p">{"dee_ddpjoc_encoder"}</span>
                    {
                      " --progress 1 --overwrite 1 --input-format atmos_mezz -i "
                    }
                    <span className="v">{"TEMP/NAME.atmos"}</span>
                    {"\n                    -o "}
                    <span className="v">
                      {"TEMP/dee-PID-INDEX/measure.ec3"}
                    </span>
                    {
                      " -e drc_profile=none --data-rate 1024\n                    --loudness-management measure_only    "
                    }
                    <span className="m">
                      {"# killed once the loudness is known"}
                    </span>
                  </pre>
                  <h4>-mo, 7.1</h4>
                  <pre className="argv">
                    <span className="p">{"dee_ddp_encoder"}</span>
                    {" --progress 1 --overwrite 1 --input-format "}
                    <span className="v">{"PCMFMT"}</span>
                    {" -i "}
                    <span className="v">{"PIPE"}</span>
                    {"\n                -o "}
                    <span className="v">
                      {"TEMP/dee-PID-INDEX/measure.ec3"}
                    </span>
                    {
                      " -e bluray:drc_profile=none --data-rate 1024\n                --loudness-management measure_only --temp-dir "
                    }
                    <span className="v">{"TEMP/dee-PID-INDEX"}</span>
                    {"   "}
                    <span className="m">{"# killed once measured"}</span>
                  </pre>
                  <div className="table-wrap">
                    <table>
                      <thead>
                        <tr>
                          <th>
                            Delay trim added to <code>-d</code>
                          </th>
                          <th>Samples</th>
                          <th>at 48 kHz</th>
                        </tr>
                      </thead>
                      <tbody>
                        <tr>
                          <td>DD, DDP, DDP Atmos</td>
                          <td>256</td>
                          <td>
                            <code>--start 0.005333</code>
                          </td>
                        </tr>
                        <tr>
                          <td>AC4, AC4 Atmos</td>
                          <td>2048</td>
                          <td>
                            <code>--start 0.042667</code>
                          </td>
                        </tr>
                        <tr>
                          <td>AAC</td>
                          <td>1600</td>
                          <td>
                            <code>--start 0.033333</code>
                          </td>
                        </tr>
                        <tr>
                          <td>TrueHD, TrueHD Atmos</td>
                          <td>0</td>
                          <td>
                            only your <code>-d</code>
                          </td>
                        </tr>
                      </tbody>
                    </table>
                  </div>
                </div>
              </article>
              <article className="module">
                <div className="module-head">
                  <h3>
                    <PuzzleIcon />
                    dee_shim
                  </h3>
                  <span className="tag">
                    external · in front of a DEE encoder
                  </span>
                  <p>
                    Loads the untouched DEE encoder (<code>--binary</code>) into
                    its own process and patches it in memory. Shim options go
                    before <code>--</code>, DEE's own options after it. deew
                    removes <code>DSHIM_LOG</code>, <code>DSHIM_LOG_LEVEL</code>
                    , <code>DSHIM_LOG_FORMAT</code>, <code>RUST_LOG</code> and{" "}
                    <code>DEE_SHIM_*</code> from its environment.
                  </p>
                </div>
                <div className="ports">
                  <div className="port">
                    <h4>Needed for</h4>
                    <ul>
                      <li>ddp-atmos, thd-atmos</li>
                      <li>TrueHD with DRC none</li>
                      <li>7.1 DDP standard profile, 7.0 DDP</li>
                    </ul>
                  </div>
                  <div className="port">
                    <h4>Errors</h4>
                    <ul>
                      <li>
                        exit 2: usage error, 3: lever not applicable (nothing
                        written)
                      </li>
                      <li>
                        deew shows its first <code>error:</code> line
                      </li>
                    </ul>
                  </div>
                </div>
                <div className="module-body">
                  <h4>DDP Atmos</h4>
                  <pre className="argv">
                    <span className="p">{"dee_shim"}</span>
                    {" ddpjoc_encoder --binary "}
                    <span className="v">{"dee/dee_ddpjoc_encoder"}</span>
                    {" --layout "}
                    <span className="v">{"5.1|7.1/eac3|7.1"}</span>
                    {" --oamd-as-is on --bitrate "}
                    <span className="v">{"B"}</span>{" "}
                    <span className="s">{"--"}</span>
                    {"\n         --progress 1 --overwrite 1 -e "}
                    <span className="v">{"ddpjoc|bluray"}</span>
                    {":drc_profile="}
                    <span className="v">{"P"}</span>
                    {"\n         --input-format atmos_mezz -i "}
                    <span className="v">{"TEMP/NAME.atmos"}</span>
                    {" -o "}
                    <span className="v">{"OUT"}</span>{" "}
                    <span className="c">
                      {"[--start S | --add-silence S:0]"}
                    </span>
                  </pre>
                  <h4>TrueHD Atmos</h4>
                  <pre className="argv">
                    <span className="p">{"dee_shim"}</span>
                    {" dthd_encoder --binary "}
                    <span className="v">{"dee/dee_dthd_encoder"}</span>
                    {" --oamd-as-is on --as-is on --drc "}
                    <span className="v">{"P"}</span>{" "}
                    <span className="s">{"--"}</span>
                    {
                      "\n         --progress 1 --overwrite 1 --presentation atmos:number_of_elements=16:legacy_authoring=0\n         --input-format atmos_mezz -i "
                    }
                    <span className="v">{"TEMP/NAME.atmos"}</span>
                    {" -o "}
                    <span className="v">{"OUT"}</span>
                    {"\n         "}
                    <span className="c">
                      {"[--start S | --add-silence S:0]"}
                    </span>
                    {" --temp-dir "}
                    <span className="v">{"TEMP/dee-PID-INDEX"}</span>
                  </pre>
                  <h4>TrueHD, DRC none</h4>
                  <pre className="argv">
                    <span className="p">{"dee_shim"}</span>
                    {" dthd_encoder --binary "}
                    <span className="v">{"dee/dee_dthd_encoder"}</span>
                    {" --drc none "}
                    <span className="s">{"--"}</span>
                    {"\n         --progress 1 --overwrite 1 --presentation "}
                    <span className="v">{"2ch|6ch|8ch"}</span>
                    {":drc_profile=film_light   "}
                    <span className="m">
                      {"# placeholder, the shim's --drc wins"}
                    </span>
                    {"\n         --input-format wav -i "}
                    <span className="v">{"TEMP/NAME.wav"}</span>
                    {" -o "}
                    <span className="v">{"OUT"}</span>{" "}
                    <span className="c">
                      {"[--start S | --add-silence S:0]"}
                    </span>
                    {" --temp-dir "}
                    <span className="v">{"TEMP/dee-PID-INDEX"}</span>
                  </pre>
                  <h4>7.1 DDP standard · 7.0 DDP</h4>
                  <pre className="argv">
                    <span className="p">{"dee_shim"}</span>
                    {" ddp_encoder --binary "}
                    <span className="v">{"dee/dee_ddp_encoder"}</span>
                    {" --layout "}
                    <span className="v">{"7.1/eac3|7.0/eac3|7.0"}</span>
                    {" --bitrate "}
                    <span className="v">{"B"}</span>{" "}
                    <span className="s">{"--"}</span>
                    {"\n         --progress 1 --overwrite 1 -e "}
                    <span className="v">{"ddp|bluray"}</span>
                    {":drc_profile="}
                    <span className="v">{"P"}</span>
                    {"\n         --input-format "}
                    <span className="v">{"PCMFMT"}</span>
                    {" -i "}
                    <span className="v">{"PIPE"}</span>
                    {" -o "}
                    <span className="v">{"OUT"}</span>
                    {" --loudness-management measure_only\n         "}
                    <span className="c">
                      {
                        "[--input-manifest TEMP/NAME.loudness.json] [--start S | --add-silence S:0]"
                      }
                    </span>
                    {" --temp-dir "}
                    <span className="v">{"TEMP/dee-PID-INDEX"}</span>
                    {"\n         "}
                    <span className="m">
                      {"# -nps adds :surround_90deg_phase_shift=0 to -e"}
                    </span>
                  </pre>
                  <div className="table-wrap">
                    <table>
                      <thead>
                        <tr>
                          <th>Case</th>
                          <th>
                            <code>--layout</code>
                          </th>
                          <th>
                            <code>-e</code> mode
                          </th>
                          <th>Coded as</th>
                          <th>Ext</th>
                        </tr>
                      </thead>
                      <tbody>
                        <tr>
                          <td>7.1 standard</td>
                          <td>
                            <code>7.1/eac3</code>
                          </td>
                          <td>
                            <code>ddp</code>
                          </td>
                          <td>E-AC-3 independent + dependent</td>
                          <td>.ec3</td>
                        </tr>
                        <tr>
                          <td>
                            7.0 standard (<code>-dl</code> or 7.0 input)
                          </td>
                          <td>
                            <code>7.0/eac3</code>
                          </td>
                          <td>
                            <code>ddp</code>
                          </td>
                          <td>E-AC-3 independent + dependent</td>
                          <td>.ec3</td>
                        </tr>
                        <tr>
                          <td>
                            7.0 with <code>--ddp-mode bluray</code>
                          </td>
                          <td>
                            <code>7.0</code>
                          </td>
                          <td>
                            <code>bluray</code>
                          </td>
                          <td>AC-3 core + E-AC-3 dependent</td>
                          <td>.eb3</td>
                        </tr>
                        <tr>
                          <td>DDP Atmos streaming</td>
                          <td>
                            <code>5.1</code>
                          </td>
                          <td>
                            <code>ddpjoc</code>
                          </td>
                          <td>E-AC-3 + JOC</td>
                          <td>.ec3</td>
                        </tr>
                        <tr>
                          <td>DDP Atmos standard</td>
                          <td>
                            <code>7.1/eac3</code>
                          </td>
                          <td>
                            <code>bluray</code>
                          </td>
                          <td>E-AC-3 independent + dependent, Atmos</td>
                          <td>.ec3</td>
                        </tr>
                        <tr>
                          <td>DDP Atmos bluray</td>
                          <td>
                            <code>7.1</code>
                          </td>
                          <td>
                            <code>bluray</code>
                          </td>
                          <td>AC-3 core + E-AC-3 dependent, Atmos</td>
                          <td>.eb3</td>
                        </tr>
                      </tbody>
                    </table>
                  </div>
                </div>
              </article>
              <article className="module">
                <div className="module-head">
                  <h3>
                    <TerminalIcon />
                    deew between the blocks
                  </h3>
                  <span className="tag step">deew</span>
                  <p>
                    What deew does with the files and output streams that pass
                    through it.
                  </p>
                </div>
                <div className="ports">
                  <div className="port">
                    <h4>Reads</h4>
                    <ul>
                      <li>ffprobe JSON</li>
                      <li>
                        ffmpeg stderr (<code>time=</code>), truehdd JSON lines,
                        DEE stdout (<code>N %</code>,{" "}
                        <code>measured_loudness</code>)
                      </li>
                      <li>each tool's exit code and last stderr line</li>
                    </ul>
                  </div>
                  <div className="port">
                    <h4>Writes</h4>
                    <ul>
                      <li>
                        the dialnorm into <code>loudness.json</code> (AC4{" "}
                        <code>-dn</code>)
                      </li>
                      <li>
                        progress events to <code>DEEW_EVENTS_FILE</code> (GUI,
                        web)
                      </li>
                      <li>
                        <code>NAME.deew-log.html</code> with <code>-kl</code>
                      </li>
                    </ul>
                  </div>
                  <div className="port">
                    <h4>Removes</h4>
                    <ul>
                      <li>
                        <code>TEMP/dee-PID-INDEX</code> after every job
                      </li>
                      <li>
                        wav / atmos / json temp files unless <code>-k</code>
                      </li>
                      <li>partial output on Ctrl-C</li>
                    </ul>
                  </div>
                </div>
              </article>
            </div>
          </section>
          <footer>
            Built from the deew source on 2026-11-11 ({__DEEW_VERSION__}):
            src/commands.rs, src/probe.rs, src/pipeline.rs, src/encode.rs. Tool
            lookup: next to deew (DEE in the <code>dee</code> folder), then
            PATH.
          </footer>
        </div>
      </div>
      <SiteFooter />
    </div>
  )
}
