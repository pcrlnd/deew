import { useEffect, useRef } from "react"
import {
  ChartNoAxesGantt,
  Copy,
  LoaderCircle,
  Play,
  Square,
  SquareTerminal,
} from "lucide-react"

import { Button } from "@/components/ui/button"
import { copyText } from "@/lib/clipboard"
import { flashClass, useFlash } from "@/lib/use-flash"
import type { OutputView } from "@/lib/runner"
import { cn } from "@/lib/utils"

interface CommandBarProps {
  command: string
  problems: string[]
  running: boolean
  /** Stop was pressed and the job hasn't ended yet. */
  stopping: boolean
  /** Drives the Run button colour. */
  runState: "idle" | "running" | "success" | "failure"
  /** Changes every time a job ends, so two equal outcomes both flash. */
  endCount: number
  canRun: boolean
  onRun: () => void
  onStop: () => void
  /** Which output the window shows. */
  view: OutputView
  onViewChange: (view: OutputView) => void
}

const VIEWS = [
  {
    value: "cli",
    label: "CLI",
    title: "Show the terminal output of the CLI",
    Icon: SquareTerminal,
  },
  {
    value: "native",
    label: "GUI",
    title: "Show progress bars, warnings and pop-up prompts",
    Icon: ChartNoAxesGantt,
  },
] as const

export function CommandBar({
  command,
  problems,
  running,
  stopping,
  runState,
  endCount,
  canRun,
  onRun,
  onStop,
  view,
  onViewChange,
}: CommandBarProps) {
  const {
    phase: copyPhase,
    kind: copyKind,
    start: startCopyFlash,
    attach: attachCopy,
  } = useFlash("fast")
  const copy = async () => {
    try {
      await copyText(command)
      startCopyFlash("success")
    } catch (error) {
      console.error("copy failed", error)
      startCopyFlash("failure")
    }
  }

  // Result flash: colour instantly, then fade out.
  const {
    phase: runPhase,
    kind: runKind,
    start: startRunFlash,
    clear: clearRunFlash,
    attach: attachRun,
  } = useFlash()
  const runStateRef = useRef(runState)
  useEffect(() => {
    runStateRef.current = runState
  })
  useEffect(() => {
    const kind = runStateRef.current
    if (running || endCount === 0 || (kind !== "success" && kind !== "failure"))
      return
    startRunFlash(kind)
    return clearRunFlash
  }, [endCount, running, startRunFlash, clearRunFlash])

  // Ignore Stop clicks right after a run starts, so a double-click on Run
  // doesn't stop the job it just started.
  const runningSinceRef = useRef(0)
  useEffect(() => {
    if (running) runningSinceRef.current = performance.now()
  }, [running])

  const state = !running ? "idle" : stopping ? "stopping" : "running"
  const click = () => {
    if (state === "idle") onRun()
    else if (
      state === "running" &&
      performance.now() - runningSinceRef.current >= 500
    )
      onStop()
  }
  const runClass = state === "idle" ? flashClass(runPhase, runKind) : ""
  const item = "col-start-1 row-start-1 flex items-center justify-center gap-1"
  const hide = (active: boolean) => (active ? "" : "invisible")

  return (
    <div className="@container flex flex-col gap-1">
      <div className="flex flex-col gap-2 @min-[520px]:flex-row @min-[520px]:items-stretch">
        <div className="flex min-h-7 min-w-0 items-stretch overflow-hidden rounded-lg border border-input bg-muted/40 @min-[520px]:flex-1">
          <div className="min-w-0 flex-1 px-2.5 py-[5px]">
            <div
              className="ui-selectable line-clamp-3 font-mono text-xs break-all whitespace-pre-wrap"
              data-testid="command"
              title={command}
            >
              {command}
            </div>
          </div>
          <div className="w-px shrink-0 self-stretch bg-border" />
          <button
            type="button"
            ref={attachCopy}
            className={cn(
              "group flex w-7 shrink-0 items-center justify-center self-stretch text-muted-foreground transition-colors outline-none hover:bg-muted hover:text-foreground focus-visible:bg-muted",
              flashClass(copyPhase, copyKind, "fast")
            )}
            aria-label="Copy command"
            title="Copy command"
            onClick={copy}
          >
            <Copy className="size-3 group-active:translate-y-px" />
          </button>
        </div>
        <div className="flex items-stretch gap-2">
          <Button
            ref={attachRun}
            size="sm"
            variant={state === "idle" ? "default" : "destructive"}
            className={cn(
              "flex-1 @min-[520px]:h-auto @min-[520px]:flex-none",
              state === "stopping" && "disabled:opacity-100",
              runClass
            )}
            onClick={click}
            disabled={state === "idle" ? !canRun : state === "stopping"}
          >
            <span className="grid">
              <span
                className={cn(item, hide(state === "idle"))}
                aria-hidden={state !== "idle"}
              >
                <Play data-icon="inline-start" />
                Run
              </span>
              <span
                className={cn(item, hide(state === "running"))}
                aria-hidden={state !== "running"}
              >
                <Square data-icon="inline-start" />
                Stop
              </span>
              <span
                className={cn(item, hide(state === "stopping"))}
                aria-hidden={state !== "stopping"}
              >
                <LoaderCircle
                  data-icon="inline-start"
                  className="animate-spin"
                />
                Stopping...
              </span>
            </span>
          </Button>
          <div
            role="group"
            aria-label="Output"
            className="flex shrink-0 items-stretch overflow-hidden rounded-lg border border-input"
          >
            {VIEWS.map(({ value, label, title, Icon }) => (
              <button
                key={value}
                type="button"
                aria-pressed={view === value}
                title={title}
                onClick={() => onViewChange(value)}
                className={cn(
                  "flex items-center gap-1 px-2 text-sm font-medium transition-colors outline-none focus-visible:ring-3 focus-visible:ring-ring/50 focus-visible:ring-inset disabled:pointer-events-none [&_svg]:size-4",
                  view === value
                    ? "bg-primary text-primary-foreground"
                    : "hover:bg-muted"
                )}
              >
                <Icon />
                {label}
              </button>
            ))}
          </div>
        </div>
      </div>
      {problems.length > 0 && (
        <p className="text-xs text-destructive">{problems.join(" ")}</p>
      )}
    </div>
  )
}
