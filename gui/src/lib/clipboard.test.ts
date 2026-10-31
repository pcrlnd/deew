import { afterEach, beforeEach, describe, expect, it, vi } from "vitest"

import { copyText } from "@/lib/clipboard"

// No DOM in the test environment, so stub just what copyText touches.
class FakeElement {
  style: Record<string, string> = {}
  value = ""
  readOnly = false
  removed = false
  focused = false
  select = vi.fn()
  setSelectionRange = vi.fn()
  remove = () => {
    this.removed = true
  }
  focus = () => {
    this.focused = true
  }
}

const exec = vi.fn()
let created: FakeElement[]
let active: FakeElement
let appended: FakeElement[]

function setup(secure: boolean, clipboard: unknown) {
  vi.stubGlobal("window", { isSecureContext: secure })
  vi.stubGlobal("navigator", { clipboard })
}

beforeEach(() => {
  exec.mockReset().mockReturnValue(true)
  created = []
  appended = []
  active = new FakeElement()
  vi.stubGlobal("HTMLElement", FakeElement)
  vi.stubGlobal("document", {
    activeElement: active,
    body: { appendChild: (el: FakeElement) => appended.push(el) },
    createElement: () => {
      const el = new FakeElement()
      created.push(el)
      return el
    },
    execCommand: exec,
  })
})

afterEach(() => {
  vi.unstubAllGlobals()
})

describe("copyText", () => {
  it("uses the Clipboard API in a secure context", async () => {
    const writeText = vi.fn().mockResolvedValue(undefined)
    setup(true, { writeText })
    await copyText("hello")
    expect(writeText).toHaveBeenCalledWith("hello")
    expect(exec).not.toHaveBeenCalled()
  })

  it("falls back to execCommand without navigator.clipboard", async () => {
    setup(false, undefined)
    await copyText("hello")
    expect(exec).toHaveBeenCalledWith("copy")
    expect(appended).toEqual(created)
    expect(created[0].value).toBe("hello")
    expect(created[0].removed).toBe(true)
    expect(active.focused).toBe(true)
  })

  it("falls back when not a secure context", async () => {
    const writeText = vi.fn()
    setup(false, { writeText })
    await copyText("hello")
    expect(writeText).not.toHaveBeenCalled()
    expect(exec).toHaveBeenCalledWith("copy")
  })

  it("falls back when writeText rejects", async () => {
    const writeText = vi.fn().mockRejectedValue(new Error("denied"))
    setup(true, { writeText })
    await copyText("hello")
    expect(exec).toHaveBeenCalledWith("copy")
    expect(created[0].removed).toBe(true)
  })

  it("rejects when execCommand returns false", async () => {
    setup(false, undefined)
    exec.mockReturnValue(false)
    await expect(copyText("hello")).rejects.toThrow("copy failed")
    expect(created[0].removed).toBe(true)
  })
})
