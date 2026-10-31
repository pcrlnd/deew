import {
  forwardRef,
  useCallback,
  useEffect,
  useImperativeHandle,
  useRef,
} from "react"
import { FitAddon } from "@xterm/addon-fit"
import { WebglAddon } from "@xterm/addon-webgl"
import { Terminal, type ITheme } from "@xterm/xterm"
import "@xterm/xterm/css/xterm.css"
import "@fontsource/ibm-plex-mono/400.css"
import "@fontsource/ibm-plex-mono/700.css"

import { fittingFontSize } from "@/lib/fitting-font-size"
import {
  TAP_SLOP_PX,
  boundaryLeftover,
  momentumStep,
  pixelsToLines,
  releaseVelocity,
  type Sample,
} from "@/lib/touch-scroll"

const MAX_FONT = 13
const MIN_COLS = 60
// Only the bundled font, no fallbacks (the cell size is measured from it).
const FONT_FAMILY = '"IBM Plex Mono"'
const FONT_FACES = [
  `400 ${MAX_FONT}px ${FONT_FAMILY}`,
  `700 ${MAX_FONT}px ${FONT_FAMILY}`,
]

function scrollableAncestor(el: HTMLElement): HTMLElement | null {
  for (let p = el.parentElement; p; p = p.parentElement) {
    const oy = getComputedStyle(p).overflowY
    if ((oy === "auto" || oy === "scroll") && p.scrollHeight > p.clientHeight)
      return p
  }
  return null
}

/**
 * Our own touch scrolling (xterm 6's gesture handling fights the page
 * scroller). Returns a cleanup function.
 */
function setupTouchScroll(el: HTMLElement, terminal: Terminal): () => void {
  el.style.touchAction = "none"
  let id: number | null = null
  let lastY = 0
  let startY = 0
  let moved = false
  let carry = 0
  let samples: Sample[] = []
  let raf = 0

  const rowHeight = () => {
    const screen = el.querySelector<HTMLElement>(".xterm-screen")
    const h = screen?.clientHeight || el.clientHeight
    return terminal.rows > 0 ? h / terminal.rows : 0
  }

  // Scrolls by a finger delta; the part the terminal can't take goes to the page.
  const apply = (dy: number) => {
    const rh = rowHeight()
    const r = pixelsToLines(dy, carry, rh)
    carry = r.carry
    if (r.lines === 0) return
    const b = terminal.buffer.active
    const split = boundaryLeftover(r.lines, b.viewportY, b.baseY, rh)
    if (split.lines !== 0) terminal.scrollLines(split.lines)
    if (split.leftoverPx !== 0) {
      const anc = scrollableAncestor(el)
      if (anc) anc.scrollTop += split.leftoverPx
      carry = 0
    }
  }

  const stopMomentum = () => {
    cancelAnimationFrame(raf)
    raf = 0
  }

  const onStart = (e: TouchEvent) => {
    stopMomentum()
    if (e.touches.length !== 1) {
      id = null
      return
    }
    const t = e.touches[0]
    id = t.identifier
    lastY = startY = t.clientY
    moved = false
    carry = 0
    samples = [{ t: e.timeStamp, y: t.clientY }]
  }

  const onMove = (e: TouchEvent) => {
    if (id === null) return
    const t = Array.from(e.touches).find((x) => x.identifier === id)
    if (!t) return
    if (!moved && Math.abs(t.clientY - startY) <= TAP_SLOP_PX) return
    moved = true
    e.preventDefault()
    e.stopPropagation()
    apply(t.clientY - lastY)
    lastY = t.clientY
    samples.push({ t: e.timeStamp, y: t.clientY })
    if (samples.length > 20) samples.shift()
  }

  const onEnd = (e: TouchEvent) => {
    if (id === null) return
    const wasMoved = moved
    id = null
    if (!wasMoved) return
    e.stopPropagation()
    let v = releaseVelocity(samples, e.timeStamp)
    let prev = performance.now()
    const tick = (now: number) => {
      const step = momentumStep(v, Math.max(1, now - prev))
      prev = now
      if (!step) {
        raf = 0
        return
      }
      v = step.velocity
      apply(step.dy)
      raf = requestAnimationFrame(tick)
    }
    raf = requestAnimationFrame(tick)
  }

  const onCancel = () => {
    id = null
  }

  const opts = { capture: true }
  el.addEventListener("touchstart", onStart, opts)
  el.addEventListener("touchmove", onMove, { capture: true, passive: false })
  el.addEventListener("touchend", onEnd, opts)
  el.addEventListener("touchcancel", onCancel, opts)
  return () => {
    stopMomentum()
    el.removeEventListener("touchstart", onStart, opts)
    el.removeEventListener("touchmove", onMove, opts)
    el.removeEventListener("touchend", onEnd, opts)
    el.removeEventListener("touchcancel", onCancel, opts)
    el.style.touchAction = ""
  }
}

export interface TerminalHandle {
  write: (data: Uint8Array | string) => void
  /** Resolves once everything written so far has been processed. */
  flush: () => Promise<void>
  /** Cursor column (0 = at the start of a line). */
  cursorX: () => number
  /** Whether anything has been written (including scrollback). */
  hasContent: () => boolean
  /** Clears the screen and the scrollback. */
  reset: () => void
  focus: () => void
  size: () => { cols: number; rows: number }
}

interface TerminalPaneProps {
  /** Keystrokes typed into the terminal. */
  onData: (data: string) => void
  onResize: (cols: number, rows: number) => void
  className?: string
}

const DARK: ITheme = {
  background: "#0a0a0a",
  foreground: "#e5e5e5",
  cursor: "#e5e5e5",
  cursorAccent: "#0a0a0a",
  selectionBackground: "#404040",
}

export const TerminalPane = forwardRef<TerminalHandle, TerminalPaneProps>(
  function TerminalPane({ onData, onResize, className }, ref) {
    const container = useRef<HTMLDivElement>(null)
    const term = useRef<Terminal | null>(null)
    const onDataRef = useRef(onData)
    const onResizeRef = useRef(onResize)

    useEffect(() => {
      onDataRef.current = onData
      onResizeRef.current = onResize
    })

    // Calls that arrive before the terminal exists (it waits for the font):
    // applied in order once it is created.
    const pending = useRef<Array<(t: Terminal) => void>>([])
    const run = useCallback((op: (t: Terminal) => void) => {
      const t = term.current
      if (t) op(t)
      else pending.current.push(op)
    }, [])

    useEffect(() => {
      const el = container.current
      if (!el) return
      let cancelled = false
      let teardown: (() => void) | undefined

      const create = () => {
        const terminal = new Terminal({
          fontFamily: FONT_FAMILY,
          fontSize: MAX_FONT,
          scrollback: 10000,
          cursorBlink: true,
          allowProposedApi: true,
          theme: DARK,
        })
        const fit = new FitAddon()
        terminal.loadAddon(fit)
        terminal.open(el)
        // The WebGL renderer draws box-drawing and block characters itself,
        // so they don't depend on the font (which has none). Without WebGL
        // the DOM renderer stays; terminal.dispose() disposes the addon.
        try {
          const webgl = new WebglAddon()
          webgl.onContextLoss(() => webgl.dispose())
          terminal.loadAddon(webgl)
        } catch (e) {
          console.warn(e)
        }
        const refit = () => {
          if (!(el.clientWidth > 0 && el.clientHeight > 0)) return
          // At least MIN_COLS columns: shrink the font when too narrow, grow
          // back (up to MAX_FONT) when there is room.
          for (let i = 0; i < 3; i++) {
            const dims = fit.proposeDimensions()
            if (!dims) break
            const current = terminal.options.fontSize ?? MAX_FONT
            const next = fittingFontSize(current, dims.cols, MIN_COLS, MAX_FONT)
            if (next === current) break
            terminal.options.fontSize = next
          }
          fit.fit()
        }
        // Subscribed before the first fit: a job started while the font was
        // loading got the default size and needs the real one.
        const dataSub = terminal.onData((d) => onDataRef.current(d))
        const resizeSub = terminal.onResize(({ cols, rows }) =>
          onResizeRef.current(cols, rows)
        )
        refit()
        const observer = new ResizeObserver(refit)
        observer.observe(el)
        const isTouch =
          window.matchMedia("(pointer: coarse)").matches ||
          navigator.maxTouchPoints > 0
        const stopTouch = isTouch ? setupTouchScroll(el, terminal) : undefined
        term.current = terminal
        const queued = pending.current
        pending.current = []
        for (const op of queued) op(terminal)
        teardown = () => {
          stopTouch?.()
          observer.disconnect()
          dataSub.dispose()
          resizeSub.dispose()
          terminal.dispose()
          term.current = null
        }
      }

      // xterm measures the cell size when it opens, so the font must be ready.
      // A failed load still creates the terminal: it must never stay blank.
      Promise.all(FONT_FACES.map((face) => document.fonts.load(face)))
        .catch(console.warn)
        .then(() => {
          if (!cancelled) create()
        })

      return () => {
        cancelled = true
        teardown?.()
      }
    }, [])

    useImperativeHandle(
      ref,
      () => ({
        write: (data) => run((t) => t.write(data)),
        flush: () =>
          new Promise<void>((resolve) => {
            const t = term.current
            if (t) t.write("", resolve)
            else pending.current.push((late) => late.write("", resolve))
          }),
        cursorX: () => term.current?.buffer.active.cursorX ?? 0,
        hasContent: () => {
          const b = term.current?.buffer.active
          return !!b && (b.baseY > 0 || b.cursorY > 0 || b.cursorX > 0)
        },
        reset: () => run((t) => t.reset()),
        focus: () => run((t) => t.focus()),
        size: () => ({
          cols: term.current?.cols ?? 80,
          rows: term.current?.rows ?? 24,
        }),
      }),
      [run]
    )

    return (
      <div
        className={className}
        style={{
          // The terminal stays dark in every UI theme.
          backgroundColor: DARK.background,
          borderColor: "#262626",
        }}
      >
        <div ref={container} className="size-full" />
      </div>
    )
  }
)
