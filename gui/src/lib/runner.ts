// Backend access. Inside Tauri these are thin wrappers around the commands of
// src-tauri/src/lib.rs; in a browser they talk to deew-web (see web-api.ts and
// run-client.ts).

import { Channel, invoke, isTauri } from "@tauri-apps/api/core"

import type { ProbeResult } from "@/lib/probe"
import type { ThemeMode } from "@/lib/theme"
import { fsRequestPath, type FsListing } from "@/lib/fs-browser"
import {
  RunClient,
  infoSocketUrl,
  parseInfoMessage,
  runSocketUrl,
  type RunClientHandlers,
} from "@/lib/run-client"
import { checkSession, errorMessage, requestJson } from "@/lib/web-api"

type RunEvent =
  | { event: "output"; data: string } // base64 of raw pty bytes
  | { event: "exit"; data: { code: number | null } }
  | { event: "progress"; data: string } // raw event line of the CLI

export interface RunHandlers {
  onOutput: (bytes: Uint8Array) => void
  onExit: (code: number | null) => void
  onProgress: (line: string) => void
}

export const inTauri = (): boolean => isTauri()

/** Running in a browser against deew-web. */
export const inWeb = (): boolean => !isTauri()

/**
 * Events of the run socket (web only). The server pushes them for every job,
 * also for jobs started elsewhere (page reload, another tab), so the web
 * flow is driven by these rather than by the handlers of `runDeew`.
 */
export type WebRunEvents = Omit<RunClientHandlers, "onClose">

let runClient: RunClient | null = null

/** A socket silent for longer than this is replaced (the server pings at 5 s). */
const RUN_SOCKET_MAX_AGE_MS = 7000

/** Replaces the run socket by a fresh one now (web only). */
export function reconnectRunSocket(): void {
  runClient?.ensureFresh(-1)
}

/**
 * Opens the run socket (reconnecting with backoff) and delivers its events.
 * Call once the user is authenticated; returns the function that closes it.
 */
export function connectRunEvents(events: WebRunEvents): () => void {
  const client = new RunClient({
    url: runSocketUrl(location),
    createSocket: (url) => new WebSocket(url),
    handlers: {
      ...events,
      // A socket that never connected may mean the session is gone.
      onClose: (opened) => {
        if (!opened) void checkSession()
      },
    },
  })
  runClient = client
  client.connect()
  // A phone that slept or changed network leaves a dead socket that still
  // reports OPEN: replace it when the page comes back.
  const fresh = () => client.ensureFresh(RUN_SOCKET_MAX_AGE_MS)
  const onVisible = () => {
    if (document.visibilityState === "visible") fresh()
  }
  document.addEventListener("visibilitychange", onVisible)
  window.addEventListener("pageshow", fresh)
  window.addEventListener("online", fresh)
  return () => {
    document.removeEventListener("visibilitychange", onVisible)
    window.removeEventListener("pageshow", fresh)
    window.removeEventListener("online", fresh)
    client.close()
    if (runClient === client) runClient = null
  }
}

function sendWeb(message: Parameters<RunClient["send"]>[0]): Promise<void> {
  return runClient?.send(message)
    ? Promise.resolve()
    : Promise.reject(new Error("Not connected to the server"))
}

function decodeBase64(text: string): Uint8Array {
  const bin = atob(text)
  const bytes = new Uint8Array(bin.length)
  for (let i = 0; i < bin.length; i++) bytes[i] = bin.charCodeAt(i)
  return bytes
}

/**
 * Starts a run. `native` asks the CLI for structured progress events; the GUI
 * always passes `true`, so the GUI view works whichever view the run started in.
 */
export async function runDeew(
  args: string[],
  cols: number,
  rows: number,
  native: boolean,
  handlers: RunHandlers
): Promise<void> {
  // Web: output / exit arrive through `connectRunEvents`, `handlers` is unused.
  if (inWeb()) {
    runClient?.ensureFresh(RUN_SOCKET_MAX_AGE_MS)
    // A socket that was just replaced or is reconnecting is not open yet.
    if (runClient && !(await runClient.whenConnected(5000)))
      throw new Error("Not connected to the server")
    return sendWeb({ type: "start", args, cols, rows, native })
  }
  const channel = new Channel<RunEvent>()
  channel.onmessage = (message) => {
    if (message.event === "output")
      handlers.onOutput(decodeBase64(message.data))
    else if (message.event === "progress") handlers.onProgress(message.data)
    else handlers.onExit(message.data.code)
  }
  await invoke("run_deew", { args, cols, rows, native, onOutput: channel })
}

export const writeStdin = (data: string): Promise<void> =>
  inWeb() ? sendWeb({ type: "input", data }) : invoke("write_stdin", { data })

export const resizePty = (cols: number, rows: number): Promise<void> =>
  inWeb()
    ? sendWeb({ type: "resize", cols, rows })
    : invoke("resize", { cols, rows })

export const stopDeew = (): Promise<void> =>
  inWeb() ? sendWeb({ type: "stop" }) : invoke("stop")

export interface InfoHandlers {
  onOutput: (bytes: Uint8Array) => void
  /** The command ended; `code` is null when unknown (connection lost). */
  onExit: (code: number | null) => void
  /** The command was refused or could not start; no `onExit` follows. */
  onError: (message: string) => void
}

// Web: the socket of the running Utilities command (it ends with it).
let infoSocket: WebSocket | null = null

/**
 * Runs one Utilities command (an allowlisted flag, see `INFO_COMMANDS`) in
 * its own terminal, independent of the encode. Output, the end and refusals
 * (e.g. -gt during an encode) arrive through `handlers`.
 */
export async function runInfo(
  flag: string,
  cols: number,
  rows: number,
  handlers: InfoHandlers
): Promise<void> {
  if (inWeb()) {
    const socket = new WebSocket(infoSocketUrl(location))
    socket.binaryType = "arraybuffer"
    infoSocket = socket
    let finished = false
    const finish = (end: () => void) => {
      if (finished) return
      finished = true
      if (infoSocket === socket) infoSocket = null
      socket.onmessage = socket.onclose = socket.onerror = null
      socket.close()
      end()
    }
    socket.onopen = () =>
      socket.send(JSON.stringify({ type: "start", flag, cols, rows }))
    socket.onmessage = (event) => {
      if (event.data instanceof ArrayBuffer) {
        handlers.onOutput(new Uint8Array(event.data))
        return
      }
      const message =
        typeof event.data === "string" ? parseInfoMessage(event.data) : null
      if (message?.type === "exit") finish(() => handlers.onExit(message.code))
      else if (message?.type === "error")
        finish(() => handlers.onError(message.message))
    }
    const lost = () => {
      void checkSession()
      finish(() => handlers.onExit(null))
    }
    socket.onclose = lost
    socket.onerror = lost
    return
  }
  const channel = new Channel<RunEvent>()
  channel.onmessage = (message) => {
    if (message.event === "output")
      handlers.onOutput(decodeBase64(message.data))
    else if (message.event === "exit") handlers.onExit(message.data.code)
  }
  try {
    await invoke("run_info", { flag, cols, rows, onOutput: channel })
  } catch (error) {
    handlers.onError(errorMessage(error))
  }
}

export const resizeInfo = (cols: number, rows: number): Promise<void> => {
  if (!inWeb()) return invoke("resize_info", { cols, rows })
  if (infoSocket?.readyState === WebSocket.OPEN)
    infoSocket.send(JSON.stringify({ type: "resize", cols, rows }))
  return Promise.resolve()
}

export const writeInfo = (data: string): Promise<void> => {
  if (!inWeb()) return invoke("write_info", { data })
  if (infoSocket?.readyState === WebSocket.OPEN)
    infoSocket.send(JSON.stringify({ type: "input", data }))
  return Promise.resolve()
}

export const stopInfo = (): Promise<void> => {
  if (!inWeb()) return invoke("stop_info")
  if (infoSocket?.readyState === WebSocket.OPEN)
    infoSocket.send(JSON.stringify({ type: "stop" }))
  return Promise.resolve()
}

export const probeInputs = (
  inputs: string[],
  trackIndex: string
): Promise<ProbeResult> =>
  inWeb()
    ? requestJson("/api/probe", {
        method: "POST",
        body: { inputs, trackIndex },
      })
    : invoke("probe_inputs", { inputs, trackIndex })

export const cliVersion = async (): Promise<string> =>
  inWeb()
    ? (await requestJson<{ version: string }>("/api/version")).version
    : invoke("cli_version")

/** The config's `[default_bitrates]` (key -> kbps); empty if unavailable. */
export const defaultBitrates = (): Promise<Record<string, number>> =>
  inWeb() ? requestJson("/api/default-bitrates") : invoke("default_bitrates")

/** Lists a directory of the machine deew-web runs on (web only). */
export const listDirectory = (path?: string): Promise<FsListing> =>
  requestJson(fsRequestPath(path))

/** What the window shows of a run: the CLI's terminal or native progress. */
export type OutputView = "cli" | "native"

export interface GuiSettings {
  progress: OutputView
  /** `output.path`: the output directory (-o), empty uses the default. */
  output: string
  /** `output.to_input`: save each output next to its input (-oi). */
  toInput: boolean
}

export interface LoadedGuiSettings extends GuiSettings {
  /** `gui.scaling` (read-only here; deew-gui applies it natively at startup). */
  scaling: number
  /** `gui.use_system_font`: the OS font instead of IBM Plex Sans (read-only here). */
  useSystemFont: boolean
  /** `gui.theme`: 'system', 'light' or 'dark' (deew-gui only; deew-web ignores it). */
  theme: ThemeMode
  /** The folder deew runs in (the home folder), empty when unknown. */
  defaultOutput: string
  /** `default_modes.ddp` of the config. */
  defaultDdpMode: string
  /** `default_modes.ddp_atmos` of the config. */
  defaultDdpAtmosMode: string
  /** `default_modes.ac4_ims` of the config. */
  defaultAc4ImsMode: string
  /** `surround_phase_shift` of the config (true when missing). */
  surroundPhaseShift: boolean
  /** `low_pass_filter` of the config (true when missing). */
  lowPassFilter: boolean
  /** `surround_3db_attenuation` of the config (false when missing). */
  surround3dbAttenuation: boolean
  /** `truehdd_warp_mode` of the config (empty when missing). */
  truehddWarpMode: string
  /** `truehdd_bed_conform` of the config (false when missing). */
  truehddBedConform: boolean
  /** `output.template` of the config (`{stem}` when missing). */
  outputTemplate: string
}

/** The GUI settings of the config (`gui.progress`, `output.*`, `default_modes`). */
export const guiSettings = (): Promise<LoadedGuiSettings> =>
  inWeb() ? requestJson("/api/settings") : invoke("gui_settings")

/** Saves only `gui.theme` (deew-gui; deew-web keeps the mode in the browser). */
export const saveGuiTheme = (theme: ThemeMode): Promise<void> =>
  inWeb() ? Promise.resolve() : invoke("save_gui_theme", { theme })

export const saveGuiSettings = (settings: GuiSettings): Promise<void> =>
  inWeb()
    ? requestJson("/api/settings", { method: "POST", body: settings })
    : invoke("save_gui_settings", { settings })
