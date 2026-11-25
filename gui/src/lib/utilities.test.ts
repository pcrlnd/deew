import { describe, expect, it } from "vitest"

import { INFO_COMMANDS } from "@/lib/deew-args"
import {
  activeTab,
  encodeBlock,
  excludesEncode,
  needsConfirmation,
  CLI_MISSING_REASON,
  utilityBlock,
} from "@/lib/utilities"

const EXCLUSIVE: string[] = ["-gt", "-gc", "-u", "-u all"]

describe("utilityBlock", () => {
  const idle = { encoding: false, utilityRunning: false }

  it("allows everything when nothing runs", () => {
    for (const { flag } of INFO_COMMANDS)
      expect(utilityBlock(flag, idle)).toBeNull()
  })

  it("greys out only -gt, -gc and the updates during an encode, with a reason", () => {
    const encoding = { encoding: true, utilityRunning: false }
    for (const { flag } of INFO_COMMANDS) {
      const reason = utilityBlock(flag, encoding)
      if (EXCLUSIVE.includes(flag)) {
        expect(reason).toMatch(/encode/)
        expect(excludesEncode(flag)).toBe(true)
      } else {
        expect(reason).toBeNull()
        expect(excludesEncode(flag)).toBe(false)
      }
    }
    expect(utilityBlock("-gt", encoding)).toMatch(/ffmpeg/)
    expect(utilityBlock("-gc", encoding)).toMatch(/config\.toml/)
    expect(utilityBlock("-u", encoding)).toMatch(/deew/)
    expect(utilityBlock("-u all", encoding)).toMatch(/encode/)
  })

  it("without the CLI only -u all is offered, with the reason", () => {
    const missing = { encoding: false, utilityRunning: false, cliMissing: true }
    for (const { flag } of INFO_COMMANDS) {
      if (flag === "-u all") expect(utilityBlock(flag, missing)).toBeNull()
      else expect(utilityBlock(flag, missing)).toBe(CLI_MISSING_REASON)
    }
    expect(CLI_MISSING_REASON).toMatch(
      /deew CLI not found.*Update and install all/
    )
    const present = { ...missing, cliMissing: false }
    for (const { flag } of INFO_COMMANDS)
      expect(utilityBlock(flag, present)).toBeNull()
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
      expect(encodeBlock(flag) !== null).toBe(EXCLUSIVE.includes(flag))
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
  it("only -gc and the updates change user files", () => {
    expect(
      INFO_COMMANDS.filter((c) => needsConfirmation(c.flag)).map((c) => c.flag)
    ).toEqual(["-gc", "-u", "-u all"])
  })
})
