import { describe, expect, it } from "vitest"

import { parseThemeMode, resolveTheme, themeFromPrefersDark } from "./theme"

describe("theme", () => {
  it("maps the OS preference", () => {
    expect(themeFromPrefersDark(true)).toBe("dark")
    expect(themeFromPrefersDark(false)).toBe("light")
  })
  it("system follows the OS, a fixed mode does not", () => {
    expect(resolveTheme("system", true)).toBe("dark")
    expect(resolveTheme("system", false)).toBe("light")
    expect(resolveTheme("light", true)).toBe("light")
    expect(resolveTheme("dark", false)).toBe("dark")
  })
  it("parses a stored mode, anything else is system", () => {
    expect(parseThemeMode("light")).toBe("light")
    expect(parseThemeMode("dark")).toBe("dark")
    expect(parseThemeMode("system")).toBe("system")
    expect(parseThemeMode("x")).toBe("system")
    expect(parseThemeMode(null)).toBe("system")
    expect(parseThemeMode(undefined)).toBe("system")
  })
})
