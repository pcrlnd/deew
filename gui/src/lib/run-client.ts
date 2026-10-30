// WebSocket side of the web mode: `GET /api/run` of deew-web. One persistent
// socket carries start / input / resize / stop and the job's output.

export type ServerMessage =
  | { type: "state"; running: boolean; args: string[] | null }
  | { type: "started"; args: string[] }
  | { type: "exit"; code: number | null }
  | { type: "error"; message: string }
  | { type: "progress"; data: string }
  | { type: "ping" }

export type ClientMessage =
  | {
      type: "start"
      args: string[]
      cols: number
      rows: number
      native: boolean
    }
  | { type: "input"; data: string }
  | { type: "resize"; cols: number; rows: number }
  | { type: "stop" }

const isStringArray = (value: unknown): value is string[] =>
  Array.isArray(value) && value.every((v) => typeof v === "string")

/** Parses a text frame of the server; `null` for anything unknown. */
export function parseServerMessage(text: string): ServerMessage | null {
  let value: unknown
  try {
    value = JSON.parse(text)
  } catch {
    return null
  }
  if (!value || typeof value !== "object") return null
  const m = value as Record<string, unknown>
  switch (m.type) {
    case "state":
      if (typeof m.running !== "boolean") return null
      return {
        type: "state",
        running: m.running,
        args: isStringArray(m.args) ? m.args : null,
      }
    case "started":
      return isStringArray(m.args) ? { type: "started", args: m.args } : null
    case "exit":
      return {
        type: "exit",
        code: typeof m.code === "number" ? m.code : null,
      }
    case "error":
      return typeof m.message === "string"
        ? { type: "error", message: m.message }
        : null
    case "progress":
      return typeof m.data === "string"
        ? { type: "progress", data: m.data }
        : null
    case "ping":
      return { type: "ping" }
    default:
      return null
  }
}

/** `ws://` or `wss://` URL of the run socket for the page's location. */
export function runSocketUrl(location: {
  protocol: string
  host: string
}): string {
  const scheme = location.protocol === "https:" ? "wss:" : "ws:"
  return `${scheme}//${location.host}/api/run`
}

/** What the UI should do with a `state` message (sent on every connect). */
export type StateAction =
  /** A job is running that this tab did not start (reload, other tab). */
  | "attach"
  /** Reconnected while a job runs: the output is replayed from the start. */
  | "reattach"
  /** The job ended while the connection was down. */
  | "lost"
  | "none"

export function stateAction(
  uiRunning: boolean,
  serverRunning: boolean
): StateAction {
  if (serverRunning) return uiRunning ? "reattach" : "attach"
  return uiRunning ? "lost" : "none"
}

/** The subset of `WebSocket` that the client uses. */
export interface RunSocket {
  readonly readyState: number
  binaryType: BinaryType
  onopen: ((event: Event) => void) | null
  onmessage: ((event: MessageEvent) => void) | null
  onclose: ((event: CloseEvent) => void) | null
  onerror: ((event: Event) => void) | null
  send: (data: string) => void
  close: () => void
}

const OPEN = 1

export interface RunClientHandlers {
  onState: (running: boolean, args: string[] | null) => void
  onStarted: (args: string[]) => void
  onOutput: (bytes: Uint8Array) => void
  onExit: (code: number | null) => void
  onError: (message: string) => void
  /** One raw progress event line (see lib/progress.ts). */
  onProgress: (line: string) => void
  /** The socket closed; `opened` is false if it never got connected. */
  onClose?: (opened: boolean) => void
}

export interface RunClientOptions {
  url: string
  createSocket: (url: string) => RunSocket
  handlers: RunClientHandlers
  /** Reconnect delay: starts at `initial`, doubles up to `max` (ms). */
  backoff?: { initial: number; max: number }
}

export const DEFAULT_BACKOFF = { initial: 500, max: 10_000 }

/** The server pings every 5 s; this long without any frame means it is dead. */
export const STALE_AFTER_MS = 15_000
const WATCHDOG_INTERVAL_MS = 2_000

/** Keeps the socket connected (with backoff) and dispatches its messages. */
export class RunClient {
  private socket: RunSocket | null = null
  private timer: ReturnType<typeof setTimeout> | undefined
  private delay: number
  private stopped = true
  private readonly options: RunClientOptions
  private lastMessageAt = 0
  private watchdog: ReturnType<typeof setInterval> | undefined
  private waiters: ((connected: boolean) => void)[] = []

  constructor(options: RunClientOptions) {
    this.options = options
    this.delay = (options.backoff ?? DEFAULT_BACKOFF).initial
  }

  get connected(): boolean {
    return this.socket?.readyState === OPEN
  }

  connect(): void {
    this.stopped = false
    this.open()
  }

  /** Closes for good (no reconnect). */
  close(): void {
    this.stopped = true
    clearTimeout(this.timer)
    clearInterval(this.watchdog)
    this.watchdog = undefined
    this.dropSocket()
    this.settleWaiters(false)
  }

  /**
   * Resolves true once connected (at once if already so), false after
   * `timeoutMs` or when the client is closed.
   */
  whenConnected(timeoutMs: number): Promise<boolean> {
    if (this.connected) return Promise.resolve(true)
    if (this.stopped) return Promise.resolve(false)
    return new Promise((resolve) => {
      const settle = (value: boolean) => {
        clearTimeout(timer)
        this.waiters = this.waiters.filter((w) => w !== settle)
        resolve(value)
      }
      const timer = setTimeout(() => settle(false), timeoutMs)
      this.waiters.push(settle)
    })
  }

  private settleWaiters(value: boolean): void {
    for (const waiter of [...this.waiters]) waiter(value)
  }

  /**
   * Replaces the socket by a fresh one now if it is not connected or nothing
   * arrived for `maxAgeMs` (a half-open connection looks open but is dead).
   */
  ensureFresh(maxAgeMs: number): void {
    if (this.stopped) return
    if (this.connected && Date.now() - this.lastMessageAt <= maxAgeMs) return
    this.reconnectNow()
  }

  private dropSocket(): boolean {
    const socket = this.socket
    this.socket = null
    if (!socket) return false
    socket.onopen = socket.onmessage = socket.onclose = socket.onerror = null
    try {
      socket.close()
    } catch {
      // already gone
    }
    return true
  }

  private reconnectNow(): void {
    clearTimeout(this.timer)
    const opened = this.openedOnce
    if (this.dropSocket()) this.options.handlers.onClose?.(opened)
    this.delay = (this.options.backoff ?? DEFAULT_BACKOFF).initial
    this.open()
  }

  private openedOnce = false

  /** Sends a message; false if the socket is not connected. */
  send(message: ClientMessage): boolean {
    if (!this.socket || !this.connected) return false
    this.socket.send(JSON.stringify(message))
    return true
  }

  private open(): void {
    const socket = this.options.createSocket(this.options.url)
    socket.binaryType = "arraybuffer"
    this.socket = socket
    let opened = false
    this.openedOnce = false
    this.lastMessageAt = Date.now()
    if (this.watchdog === undefined)
      this.watchdog = setInterval(() => this.check(), WATCHDOG_INTERVAL_MS)
    socket.onopen = () => {
      opened = true
      this.openedOnce = true
      this.lastMessageAt = Date.now()
      this.delay = (this.options.backoff ?? DEFAULT_BACKOFF).initial
      this.settleWaiters(true)
    }
    socket.onmessage = (event) => {
      this.lastMessageAt = Date.now()
      this.dispatch(event.data)
    }
    socket.onclose = () => {
      if (this.socket !== socket) return
      this.socket = null
      this.options.handlers.onClose?.(opened)
      if (!this.stopped) this.scheduleReconnect()
    }
  }

  private check(): void {
    if (this.stopped || !this.connected) return
    if (Date.now() - this.lastMessageAt >= STALE_AFTER_MS) this.reconnectNow()
  }

  private scheduleReconnect(): void {
    const { max } = this.options.backoff ?? DEFAULT_BACKOFF
    this.timer = setTimeout(() => {
      if (!this.stopped) this.open()
    }, this.delay)
    this.delay = Math.min(this.delay * 2, max)
  }

  private dispatch(data: unknown): void {
    const h = this.options.handlers
    if (data instanceof ArrayBuffer) {
      h.onOutput(new Uint8Array(data))
      return
    }
    if (typeof data !== "string") return
    const message = parseServerMessage(data)
    if (!message) return
    switch (message.type) {
      case "state":
        h.onState(message.running, message.args)
        break
      case "started":
        h.onStarted(message.args)
        break
      case "exit":
        h.onExit(message.code)
        break
      case "error":
        h.onError(message.message)
        break
      case "progress":
        h.onProgress(message.data)
        break
      case "ping":
        break
    }
  }
}
