import { describe, expect, it } from "vitest"

import { INFO_COMMANDS } from "@/lib/deew-args"
import {
  activeTab,
  encodeBlock,
  excludesEncode,
  needsConfirmation,
  utilityBlock,
} from "@/lib/utilities"

describe("utilityBlock", () => {
  const idle = { encoding: false, utilityRunning: false }

  it("allows everything when nothing runs", () => {
    for (const { flag } of INFO_COMMANDS)
      expect(utilityBlock(flag, idle)).toBeNull()
  })

  it("greys out only -gt and -gc during an encode, with a reason", () => {
    const encoding = { encoding: true, utilityRunning: false }
    for (const { flag } of INFO_COMMANDS) {
      const reason = utilityBlock(flag, encoding)
      if (flag === "-gt" || flag === "-gc") {
        expect(reason).toMatch(/encode/)
        expect(excludesEncode(flag)).toBe(true)
      } else {
        expect(reason).toBeNull()
        expect(excludesEncode(flag)).toBe(false)
      }
    }
    expect(utilityBlock("-gt", encoding)).toMatch(/ffmpeg/)
    expect(utilityBlock("-gc", encoding)).toMatch(/config\.toml/)
  })

  it("allows one utility at a time", () => {
    for (const { flag } of INFO_COMMANDS)
      expect(
        utilityBlock(flag, { encoding: false, utilityRunning: true })
      ).not.toBeNull()
  })
})

describe("encodeBlock", () => {
  it("refuses an encode only while -gt or -gc runs", () => {
    expect(encodeBlock(null)).toBeNull()
    for (const { flag } of INFO_COMMANDS) {
      expect(encodeBlock(flag) !== null).toBe(flag === "-gt" || flag === "-gc")
    }
    expect(encodeBlock("-gt")).toContain("-gt")
  })
})

describe("activeTab", () => {
  it("shows the saved view unless the utilities are shown", () => {
    expect(activeTab("cli", false)).toBe("cli")
    expect(activeTab("native", false)).toBe("native")
    expect(activeTab("native", true)).toBe("utilities")
  })
})

describe("needsConfirmation", () => {
  it("only -gc overwrites user data", () => {
    expect(INFO_COMMANDS.filter((c) => needsConfirmation(c.flag))).toEqual(
      INFO_COMMANDS.filter((c) => c.flag === "-gc")
    )
  })
})
