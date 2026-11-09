export type Platform = "windows" | "linux" | "macos" | "other"

type NavigatorWithUAData = Navigator & {
  userAgentData?: { platform?: string }
}

export function detectPlatform(): Platform {
  if (typeof navigator === "undefined") return "other"
  const nav = navigator as NavigatorWithUAData
  const p = `${nav.userAgentData?.platform ?? ""} ${nav.platform ?? ""} ${nav.userAgent ?? ""}`
  if (/android|iphone|ipad|ipod|cros/i.test(p)) return "other"
  if (/macintosh|mac os|macintel/i.test(p)) return "macos"
  if (/win/i.test(p)) return "windows"
  if (/linux|x11/i.test(p)) return "linux"
  return "other"
}

export const REPO = "https://github.com/pcrlnd/deew"
export const RELEASES = `${REPO}/releases`
export const LATEST = `${REPO}/releases/latest/download/`
export const KOFI = "https://ko-fi.com/pcrlnd"
export const HOME = "./"
export const TOOL_CHAIN = "tool-chain"
