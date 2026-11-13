import { describe, expect, it } from "vitest"

import type { Format } from "@/lib/deew-args"
import { isShimNoPhaseShift } from "@/lib/shim"

describe("isShimNoPhaseShift", () => {
  it("is true only for ddp-atmos", () => {
    expect(isShimNoPhaseShift("ddp-atmos")).toBe(true)
    const others: Format[] = [
      "dd",
      "ddp",
      "thd",
      "thd-atmos",
      "ac4",
      "ac4-atmos",
      "aac",
    ]
    for (const f of others) expect(isShimNoPhaseShift(f)).toBe(false)
  })
})
