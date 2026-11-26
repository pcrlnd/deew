import { useEffect, useState } from "react"
import { getVersion } from "@tauri-apps/api/app"
import { getCurrentWindow } from "@tauri-apps/api/window"
import { Copy, Minus, Square, X } from "lucide-react"

import { ThemeMenu } from "@/components/theme-menu"
import { UtilitiesMenu } from "@/components/utilities-menu"
import type { ThemeMode } from "@/lib/theme"
import type { InfoFlag } from "@/lib/deew-args"
import { inTauri } from "@/lib/runner"
import { cn } from "@/lib/utils"

function WindowButton({
  label,
  onClick,
  className,
  children,
}: {
  label: string
  onClick: () => void
  className?: string
  children: React.ReactNode
}) {
  return (
    <button
      type="button"
      aria-label={label}
      title={label}
      tabIndex={-1}
      onClick={onClick}
      className={cn(
        "inline-flex h-full w-8 items-center justify-center text-muted-foreground transition-colors outline-none hover:bg-muted hover:text-foreground [&_svg]:size-3",
        className
      )}
    >
      {children}
    </button>
  )
}

/** Window buttons for the undecorated window (Windows style). */
function WindowControls() {
  const [maximized, setMaximized] = useState(false)

  useEffect(() => {
    const win = getCurrentWindow()
    let unlisten: (() => void) | undefined
    let cancelled = false
    const update = () =>
      void win
        .isMaximized()
        .then((value) => {
          if (!cancelled) setMaximized(value)
        })
        .catch(console.error)
    update()
    win
      .onResized(update)
      .then((fn) => {
        if (cancelled) fn()
        else unlisten = fn
      })
      .catch(console.error)
    return () => {
      cancelled = true
      unlisten?.()
    }
  }, [])

  const win = getCurrentWindow()
  return (
    <>
      <WindowButton
        label="Minimize"
        onClick={() => void win.minimize().catch(console.error)}
      >
        <Minus />
      </WindowButton>
      <WindowButton
        label={maximized ? "Restore" : "Maximize"}
        onClick={() => void win.toggleMaximize().catch(console.error)}
      >
        {maximized ? <Copy className="-scale-x-100" /> : <Square />}
      </WindowButton>
      <WindowButton
        label="Close"
        onClick={() => void win.close().catch(console.error)}
        className="hover:bg-red-600 hover:text-white"
      >
        <X />
      </WindowButton>
    </>
  )
}

/** The logo and "deew-gui <version>" of the custom title bar. */
function Brand() {
  const [version, setVersion] = useState<string>()

  useEffect(() => {
    let cancelled = false
    getVersion()
      .then((value) => {
        if (!cancelled) setVersion(value)
      })
      .catch(console.error)
    return () => {
      cancelled = true
    }
  }, [])

  return (
    <div
      data-tauri-drag-region
      className="pointer-events-none flex items-center gap-2 pl-3"
    >
      <img src="/icon.png" alt="" className="size-4" />
      <span className="text-xs font-semibold">
        {version ? `deew-gui ${version}` : "deew-gui"}
      </span>
    </div>
  )
}

/**
 * Replaces the native title bar (or, with `nativeTitleBar`, a toolbar under it). Only elements that carry
 * `data-tauri-drag-region` themselves drag the window (double click
 * maximizes), so the brand is pointer-transparent.
 */
export function TitleBar({
  encoding,
  utilityRunning,
  cliMissing = false,
  onRunInfo,
  onThemeChange,
  nativeTitleBar = false,
}: {
  encoding: boolean
  utilityRunning: boolean
  cliMissing?: boolean
  onRunInfo: (flag: InfoFlag) => void
  onThemeChange: (mode: ThemeMode) => void
  /** The OS draws the title bar: a plain toolbar, no drag region or window buttons. */
  nativeTitleBar?: boolean
}) {
  return (
    <header
      data-tauri-drag-region={nativeTitleBar ? undefined : true}
      className="flex h-[27px] shrink-0 items-center border-b border-border bg-background select-none"
    >
      {nativeTitleBar ? (
        <div className="flex-1" />
      ) : (
        <>
          <Brand />
          <div data-tauri-drag-region className="h-full flex-1" />
        </>
      )}
      <div className="mr-1 flex items-center gap-1">
        <UtilitiesMenu
          encoding={encoding}
          utilityRunning={utilityRunning}
          cliMissing={cliMissing}
          onRun={onRunInfo}
        />
        <ThemeMenu onChange={onThemeChange} />
      </div>
      {inTauri() && !nativeTitleBar && <WindowControls />}
    </header>
  )
}
