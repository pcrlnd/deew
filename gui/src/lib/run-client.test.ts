import {
  afterEach,
  beforeEach,
  describe,
  expect,
  it,
  vi,
  type Mock,
} from "vitest"

import {
  RunClient,
  parseServerMessage,
  runSocketUrl,
  stateAction,
  type RunClientHandlers,
  type RunSocket,
} from "./run-client"

class FakeSocket implements RunSocket {
  readyState = 0
  binaryType: BinaryType = "blob"
  onopen: ((event: Event) => void) | null = null
  onmessage: ((event: MessageEvent) => void) | null = null
  onclose: ((event: CloseEvent) => void) | null = null
  onerror: ((event: Event) => void) | null = null
  sent: string[] = []
  closed = false
  send(data: string) {
    this.sent.push(data)
  }
  close() {
    this.closed = true
  }
  open() {
    this.readyState = 1
    this.onopen?.(new Event("open"))
  }
  receive(data: unknown) {
    this.onmessage?.({ data } as MessageEvent)
  }
  drop() {
    this.readyState = 3
    this.onclose?.(new Event("close") as CloseEvent)
  }
}

describe("parseServerMessage", () => {
  it("parses the server messages", () => {
    expect(
      parseServerMessage('{"type":"state","running":true,"args":["-i","a"]}')
    ).toEqual({ type: "state", running: true, args: ["-i", "a"] })
    expect(
      parseServerMessage('{"type":"state","running":false,"args":null}')
    ).toEqual({
      type: "state",
      running: false,
      args: null,
    })
    expect(parseServerMessage('{"type":"started","args":["x"]}')).toEqual({
      type: "started",
      args: ["x"],
    })
    expect(parseServerMessage('{"type":"exit","code":0}')).toEqual({
      type: "exit",
      code: 0,
    })
    expect(parseServerMessage('{"type":"exit","code":null}')).toEqual({
      type: "exit",
      code: null,
    })
    expect(parseServerMessage('{"type":"error","message":"boom"}')).toEqual({
      type: "error",
      message: "boom",
    })
    expect(
      parseServerMessage('{"type":"progress","data":"{\\"type\\":\\"done\\"}"}')
    ).toEqual({ type: "progress", data: '{"type":"done"}' })
  })

  it("knows ping", () => {
    expect(parseServerMessage('{"type":"ping"}')).toEqual({ type: "ping" })
  })

  it("rejects malformed input", () => {
    expect(parseServerMessage("not json")).toBeNull()
    expect(parseServerMessage("42")).toBeNull()
    expect(parseServerMessage('{"type":"nope"}')).toBeNull()
    expect(parseServerMessage('{"type":"state"}')).toBeNull()
    expect(parseServerMessage('{"type":"started","args":[1]}')).toBeNull()
    expect(parseServerMessage('{"type":"error"}')).toBeNull()
    expect(parseServerMessage('{"type":"progress"}')).toBeNull()
    expect(parseServerMessage('{"type":"progress","data":1}')).toBeNull()
  })
})

describe("runSocketUrl", () => {
  it("derives ws / wss from the page", () => {
    expect(runSocketUrl({ protocol: "http:", host: "localhost:3000" })).toBe(
      "ws://localhost:3000/api/run"
    )
    expect(runSocketUrl({ protocol: "https:", host: "deew.example" })).toBe(
      "wss://deew.example/api/run"
    )
  })
})

describe("stateAction", () => {
  it("attaches, reattaches or reports a lost job", () => {
    expect(stateAction(false, true)).toBe("attach")
    expect(stateAction(true, true)).toBe("reattach")
    expect(stateAction(true, false)).toBe("lost")
    expect(stateAction(false, false)).toBe("none")
  })
})

describe("RunClient", () => {
  let sockets: FakeSocket[]
  let handlers: {
    [K in keyof RunClientHandlers]-?: Mock<NonNullable<RunClientHandlers[K]>>
  }
  let client: RunClient

  beforeEach(() => {
    vi.useFakeTimers()
    sockets = []
    handlers = {
      onState: vi.fn<RunClientHandlers["onState"]>(),
      onStarted: vi.fn<RunClientHandlers["onStarted"]>(),
      onOutput: vi.fn<RunClientHandlers["onOutput"]>(),
      onExit: vi.fn<RunClientHandlers["onExit"]>(),
      onError: vi.fn<RunClientHandlers["onError"]>(),
      onProgress: vi.fn<RunClientHandlers["onProgress"]>(),
      onClose: vi.fn<NonNullable<RunClientHandlers["onClose"]>>(),
    }
    client = new RunClient({
      url: "ws://x/api/run",
      createSocket: () => {
        const socket = new FakeSocket()
        sockets.push(socket)
        return socket
      },
      handlers,
      backoff: { initial: 100, max: 400 },
    })
  })
  afterEach(() => {
    client.close()
    vi.useRealTimers()
  })

  it("uses arraybuffer frames and dispatches messages", () => {
    client.connect()
    const [socket] = sockets
    expect(socket.binaryType).toBe("arraybuffer")
    socket.open()
    socket.receive('{"type":"state","running":true,"args":["a"]}')
    socket.receive(new Uint8Array([104, 105]).buffer)
    socket.receive('{"type":"started","args":["b"]}')
    socket.receive('{"type":"exit","code":3}')
    socket.receive('{"type":"error","message":"nope"}')
    socket.receive('{"type":"progress","data":"line"}')
    socket.receive("garbage")
    expect(handlers.onState).toHaveBeenCalledWith(true, ["a"])
    expect(handlers.onOutput).toHaveBeenCalledWith(new Uint8Array([104, 105]))
    expect(handlers.onStarted).toHaveBeenCalledWith(["b"])
    expect(handlers.onExit).toHaveBeenCalledWith(3)
    expect(handlers.onError).toHaveBeenCalledWith("nope")
    expect(handlers.onProgress).toHaveBeenCalledWith("line")
  })

  it("sends JSON only while connected", () => {
    client.connect()
    const [socket] = sockets
    expect(client.send({ type: "stop" })).toBe(false)
    expect(
      client.send({
        type: "start",
        args: [],
        cols: 80,
        rows: 24,
        native: true,
      })
    ).toBe(false)
    socket.open()
    expect(client.send({ type: "input", data: "y" })).toBe(true)
    expect(JSON.parse(socket.sent[0])).toEqual({ type: "input", data: "y" })
  })

  it("reconnects with a growing, capped delay that resets on open", () => {
    client.connect()
    sockets[0].drop()
    expect(handlers.onClose).toHaveBeenLastCalledWith(false)
    vi.advanceTimersByTime(99)
    expect(sockets).toHaveLength(1)
    vi.advanceTimersByTime(1)
    expect(sockets).toHaveLength(2)
    sockets[1].drop()
    vi.advanceTimersByTime(199)
    expect(sockets).toHaveLength(2)
    vi.advanceTimersByTime(1)
    expect(sockets).toHaveLength(3)
    sockets[2].drop()
    vi.advanceTimersByTime(400)
    expect(sockets).toHaveLength(4)
    sockets[3].drop()
    vi.advanceTimersByTime(399)
    expect(sockets).toHaveLength(4)
    vi.advanceTimersByTime(1)
    expect(sockets).toHaveLength(5)
    sockets[4].open()
    sockets[4].drop()
    expect(handlers.onClose).toHaveBeenLastCalledWith(true)
    vi.advanceTimersByTime(100)
    expect(sockets).toHaveLength(6)
  })

  it("does not reconnect after close()", () => {
    client.connect()
    client.close()
    expect(sockets[0].closed).toBe(true)
    vi.advanceTimersByTime(10_000)
    expect(sockets).toHaveLength(1)
    expect(client.connected).toBe(false)
  })

  describe("liveness", () => {
    it("reconnects when nothing arrives for 15 s", () => {
      client.connect()
      sockets[0].open()
      vi.advanceTimersByTime(14_000)
      expect(sockets).toHaveLength(1)
      vi.advanceTimersByTime(2_000)
      expect(sockets[0].closed).toBe(true)
      expect(handlers.onClose).toHaveBeenCalledWith(true)
      expect(sockets).toHaveLength(2)
    })

    it("stays alive while pings arrive, and does not dispatch them", () => {
      client.connect()
      sockets[0].open()
      for (let i = 0; i < 10; i++) {
        vi.advanceTimersByTime(5_000)
        sockets[0].receive('{"type":"ping"}')
      }
      expect(sockets).toHaveLength(1)
      expect(handlers.onState).not.toHaveBeenCalled()
      expect(handlers.onError).not.toHaveBeenCalled()
      expect(handlers.onOutput).not.toHaveBeenCalled()
    })

    it("counts binary frames as liveness", () => {
      client.connect()
      sockets[0].open()
      for (let i = 0; i < 5; i++) {
        vi.advanceTimersByTime(8_000)
        sockets[0].receive(new ArrayBuffer(1))
      }
      expect(sockets).toHaveLength(1)
    })

    it("ensureFresh keeps a fresh socket and replaces a stale one", () => {
      client.connect()
      sockets[0].open()
      vi.advanceTimersByTime(4_000)
      client.ensureFresh(7_000)
      expect(sockets).toHaveLength(1)
      vi.advanceTimersByTime(4_000)
      client.ensureFresh(7_000)
      expect(sockets[0].closed).toBe(true)
      expect(sockets).toHaveLength(2)
    })

    it("ensureFresh reconnects now when not connected", () => {
      client.connect()
      sockets[0].open()
      sockets[0].drop()
      expect(sockets).toHaveLength(1)
      client.ensureFresh(7_000)
      expect(sockets).toHaveLength(2)
      vi.advanceTimersByTime(10_000)
      // the pending backoff timer was cancelled
      expect(sockets).toHaveLength(2)
    })

    it("ensureFresh does nothing after close()", () => {
      client.connect()
      client.close()
      client.ensureFresh(1)
      expect(sockets).toHaveLength(1)
    })

    describe("whenConnected", () => {
      it("resolves true at once when open", async () => {
        client.connect()
        sockets[0].open()
        await expect(client.whenConnected(5_000)).resolves.toBe(true)
      })

      it("resolves true when a reconnect opens", async () => {
        client.connect()
        sockets[0].open()
        vi.advanceTimersByTime(20_000)
        client.ensureFresh(7_000)
        const result = client.whenConnected(5_000)
        vi.advanceTimersByTime(1_000)
        sockets[sockets.length - 1].open() // the watchdog may add one too
        await expect(result).resolves.toBe(true)
        expect(vi.getTimerCount()).toBe(1) // only the watchdog remains
      })

      it("resolves false on timeout", async () => {
        client.connect()
        const result = client.whenConnected(5_000)
        vi.advanceTimersByTime(5_000)
        await expect(result).resolves.toBe(false)
      })

      it("resolves false on close()", async () => {
        client.connect()
        const result = client.whenConnected(5_000)
        client.close()
        await expect(result).resolves.toBe(false)
        expect(vi.getTimerCount()).toBe(0)
      })
    })
  })
})
