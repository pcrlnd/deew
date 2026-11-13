import type { Route } from "@/lib/router"

// Per-route head values. They must match the static <title>, description and
// canonical in index.html and tool-chain.html (those are what crawlers and
// link previews see; this keeps the head right after in-app navigation).
export const PAGE_HEAD: Record<
  Route,
  { title: string; description: string; canonical: string }
> = {
  home: {
    title: "deew - Dolby Encoding Engine Wrapper",
    description:
      "A fast CLI and GUI for Dolby Encoding Engine. deew prepares your audio with ffmpeg and truehdd and encodes it to DD, DDP, TrueHD, AC4 and AAC, including Atmos.",
    canonical: "https://deew.app/",
  },
  "tool-chain": {
    title: "deew tool chain",
    description:
      "How deew chains ffmpeg, truehdd, dee_shim and the DEE command line tools: the whole chain, the blocks each format uses and the exact options of every block.",
    canonical: "https://deew.app/tool-chain",
  },
}

export function applyHead(route: Route) {
  const h = PAGE_HEAD[route]
  document.title = h.title
  setAttr(
    'meta[name="description"]',
    "meta",
    "name",
    "description",
    "content",
    h.description
  )
  setAttr(
    'link[rel="canonical"]',
    "link",
    "rel",
    "canonical",
    "href",
    h.canonical
  )
}

function setAttr(
  selector: string,
  tag: string,
  keyAttr: string,
  keyValue: string,
  attr: string,
  value: string
) {
  let el = document.head.querySelector(selector)
  if (!el) {
    el = document.createElement(tag)
    el.setAttribute(keyAttr, keyValue)
    document.head.appendChild(el)
  }
  el.setAttribute(attr, value)
}
