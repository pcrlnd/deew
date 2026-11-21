import { useSyncExternalStore } from "react"

/** The resolved colour theme. */
export type Theme = "light" | "dark"

/** What the user picked: a fixed theme or following the OS. */
export type ThemeMode = Theme | "system"

/** Legacy persisted key; only ever removed now. */
const LEGACY_STORAGE_KEY = "deew-theme"

/** Mirror of the mode (deew-gui: avoids a flash, the config wins; deew-web: the only store). */
const MODE_STORAGE_KEY = "deew-theme-mode"

export function themeFromPrefersDark(prefersDark: boolean): Theme {
  return prefersDark ? "dark" : "light"
}

/** Any unknown value is "system". */
export function parseThemeMode(value: unknown): ThemeMode {
  return value === "light" || value === "dark" ? value : "system"
}

export function resolveTheme(mode: ThemeMode, prefersDark: boolean): Theme {
  return mode === "system" ? themeFromPrefersDark(prefersDark) : mode
}

const query = () => window.matchMedia("(prefers-color-scheme: dark)")

let mode: ThemeMode = "system"
let theme: Theme = "light"
const listeners = new Set<() => void>()

function apply() {
  document.documentElement.classList.toggle("dark", theme === "dark")
  listeners.forEach((l) => l())
}

function readStoredMode(): ThemeMode {
  try {
    return parseThemeMode(localStorage.getItem(MODE_STORAGE_KEY))
  } catch {
    return "system"
  }
}

/** Call once before rendering: starts from the stored mode (else the OS). */
export function initTheme() {
  try {
    localStorage.removeItem(LEGACY_STORAGE_KEY)
  } catch {
    // Best effort cleanup.
  }
  mode = readStoredMode()
  theme = resolveTheme(mode, query().matches)
  apply()
  query().addEventListener("change", (e) => {
    if (mode !== "system") return
    theme = themeFromPrefersDark(e.matches)
    apply()
  })
}

/** Applies the mode; `persist` mirrors it into localStorage. */
export function setThemeMode(
  next: ThemeMode,
  { persist }: { persist: boolean }
) {
  mode = next
  theme = resolveTheme(mode, query().matches)
  if (persist) {
    try {
      localStorage.setItem(MODE_STORAGE_KEY, mode)
    } catch {
      // Best effort: the mode still applies for this session.
    }
  }
  apply()
}

const subscribe = (l: () => void) => {
  listeners.add(l)
  return () => listeners.delete(l)
}

export function useTheme(): Theme {
  return useSyncExternalStore(subscribe, () => theme)
}

export function useThemeMode(): ThemeMode {
  return useSyncExternalStore(subscribe, () => mode)
}
