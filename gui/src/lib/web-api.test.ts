import { describe, expect, it, vi } from "vitest"

import { errorMessage, login, requestJson } from "./web-api"

const json = (status: number, body: unknown) =>
  new Response(JSON.stringify(body), { status })

describe("requestJson", () => {
  it("sends same-origin JSON and parses the answer", async () => {
    const fetchFn = vi.fn().mockResolvedValue(json(200, { version: "3.2.2" }))
    const result = await requestJson<{ version: string }>(
      "/api/probe",
      { method: "POST", body: { inputs: ["a"], trackIndex: 0 } },
      fetchFn
    )
    expect(result).toEqual({ version: "3.2.2" })
    const [path, init] = fetchFn.mock.calls[0]
    expect(path).toBe("/api/probe")
    expect(init.method).toBe("POST")
    expect(init.credentials).toBe("same-origin")
    expect(init.headers).toEqual({ "Content-Type": "application/json" })
    expect(JSON.parse(init.body)).toEqual({ inputs: ["a"], trackIndex: 0 })
  })

  it("returns undefined for 204", async () => {
    const fetchFn = vi
      .fn()
      .mockResolvedValue(new Response(null, { status: 204 }))
    await expect(
      requestJson("/api/logout", { method: "POST" }, fetchFn)
    ).resolves.toBeUndefined()
  })

  it("throws the server's error message", async () => {
    const fetchFn = vi
      .fn()
      .mockResolvedValue(json(400, { error: "not a directory" }))
    await expect(requestJson("/api/fs", {}, fetchFn)).rejects.toThrow(
      "not a directory"
    )
  })

  it("falls back to a generic message", async () => {
    const fetchFn = vi
      .fn()
      .mockResolvedValue(new Response("<html>", { status: 502 }))
    await expect(requestJson("/api/fs", {}, fetchFn)).rejects.toThrow(
      "Request failed (HTTP 502)"
    )
  })

  it("notifies on 401 and still throws", async () => {
    const notify = vi.fn()
    const fetchFn = vi
      .fn()
      .mockResolvedValue(json(401, { error: "unauthorized" }))
    await expect(
      requestJson("/api/version", {}, fetchFn, notify)
    ).rejects.toThrow("unauthorized")
    expect(notify).toHaveBeenCalledTimes(1)
  })

  it("does not notify on 401 when told not to (login)", async () => {
    const notify = vi.fn()
    const fetchFn = vi
      .fn()
      .mockResolvedValue(json(401, { error: "wrong username or password" }))
    await expect(
      requestJson(
        "/api/login",
        { method: "POST", notify401: false },
        fetchFn,
        notify
      )
    ).rejects.toThrow("wrong username or password")
    expect(notify).not.toHaveBeenCalled()
  })
})

describe("login", () => {
  it("sends the remember choice", async () => {
    const fetchFn = vi
      .fn()
      .mockResolvedValue(new Response(null, { status: 204 }))
    vi.stubGlobal("fetch", fetchFn)
    try {
      await login("bob", "pw", "30d")
      await login("bob", "pw")
    } finally {
      vi.unstubAllGlobals()
    }
    expect(JSON.parse(fetchFn.mock.calls[0][1].body)).toEqual({
      user: "bob",
      pass: "pw",
      remember: "30d",
    })
    expect(JSON.parse(fetchFn.mock.calls[1][1].body)).toEqual({
      user: "bob",
      pass: "pw",
    })
  })
})

describe("errorMessage", () => {
  it("unwraps errors and stringifies the rest", () => {
    expect(errorMessage(new Error("x"))).toBe("x")
    expect(errorMessage("y")).toBe("y")
  })
})
