import { useSyncExternalStore } from "react"

export type Route = "home" | "tool-chain"

// Dev server and Pages serve /tool-chain, `python3 -m http.server` serves
// /tool-chain.html. Anything else is home.
export function routeFromPath(pathname: string): Route {
  return /\/tool-chain(\.html)?$/.test(pathname) ? "tool-chain" : "home"
}

type HistoryState = { scrollY?: number } | null

// Bumped on every navigation (push or popstate); the store snapshot.
let navId = 0
const listeners = new Set<() => void>()

// What the next route render should scroll to, consumed by consumeScrollTarget.
export type ScrollTarget = { y: number } | { hash: string } | null
let pending: ScrollTarget = null

function emit() {
  navId++
  listeners.forEach((l) => l())
}

function currentScrollY() {
  return Math.round(window.scrollY)
}

function saveScroll() {
  const state = (history.state ?? {}) as Record<string, unknown>
  history.replaceState({ ...state, scrollY: currentScrollY() }, "")
}

let initialized = false

function init() {
  if (initialized || typeof window === "undefined") return
  initialized = true
  history.scrollRestoration = "manual"

  // Keep the current entry's scroll position up to date, so back/forward and
  // reload can restore it.
  let timer: number | undefined
  window.addEventListener(
    "scroll",
    () => {
      window.clearTimeout(timer)
      timer = window.setTimeout(saveScroll, 150)
    },
    { passive: true }
  )
  window.addEventListener("pagehide", saveScroll)

  window.addEventListener("popstate", () => {
    const state = history.state as HistoryState
    pending = { y: state?.scrollY ?? 0 }
    emit()
  })

  const state = history.state as HistoryState
  if (state?.scrollY) pending = { y: state.scrollY }
  else if (location.hash) pending = { hash: location.hash }
}

// Run before React's first render, so the initial scroll target is pending
// when Site's first layout effect runs.
if (typeof window !== "undefined") init()

function subscribe(cb: () => void) {
  init()
  listeners.add(cb)
  return () => {
    listeners.delete(cb)
  }
}

export function navigate(to: string) {
  init()
  const url = new URL(to, location.href)
  saveScroll()
  history.pushState({ scrollY: 0 }, "", url.pathname + url.search + url.hash)
  pending = url.hash ? { hash: url.hash } : { y: 0 }
  emit()
}

export function consumeScrollTarget(): ScrollTarget {
  const t = pending
  pending = null
  return t
}

// Subscribes the component to navigation; returns the current route.
export function useRoute(): Route {
  useSyncExternalStore(
    subscribe,
    () => navId,
    () => 0
  )
  return routeFromPath(location.pathname)
}

export function applyScroll(target: ScrollTarget) {
  if (!target) return
  if ("y" in target) {
    window.scrollTo(0, target.y)
    return
  }
  let id = target.hash.slice(1)
  try {
    id = decodeURIComponent(id)
  } catch {
    // keep the raw id
  }
  const el = id ? document.getElementById(id) : null
  if (el) el.scrollIntoView()
  else window.scrollTo(0, 0)
}
