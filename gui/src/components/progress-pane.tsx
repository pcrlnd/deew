import { useLayoutEffect, useRef } from "react"
import { CircleCheck, CircleX } from "lucide-react"

import { NoteLine } from "@/components/note-line"
import {
  formatPercent,
  overallPercent,
  stageLabel,
  statusText,
  type JobProgress,
  type RunEntry,
} from "@/lib/progress"
import { cn } from "@/lib/utils"

interface ProgressPaneProps {
  runs: RunEntry[]
  className?: string
}

/** Bar of one job; `failed` marks an unfinished job of a failed run. */
function Bar({ job, failed }: { job: JobProgress; failed: boolean }) {
  const indeterminate = job.percent === null && !job.done
  return (
    <div className="relative h-1.5 overflow-hidden rounded-full bg-muted">
      {indeterminate ? (
        <div className="absolute inset-y-0 w-2/5 rounded-full bg-primary motion-safe:animate-indeterminate motion-reduce:w-full motion-reduce:opacity-40" />
      ) : (
        <div
          className={cn(
            "h-full rounded-full transition-[width] duration-100",
            job.done
              ? "bg-green-600"
              : failed
                ? "bg-destructive/60"
                : "bg-primary"
          )}
          style={{ width: `${job.done ? 100 : (job.percent ?? 0)}%` }}
        />
      )}
    </div>
  )
}

/**
 * One job: file name, stage, bar and percent. Narrow: file name and stage on
 * one line, bar and percent below; wide: all four in one row.
 */
function JobRow({ job, failed }: { job: JobProgress; failed: boolean }) {
  const percent = job.done ? 100 : job.percent
  const stage = job.done ? "done" : stageLabel(job)
  return (
    <div className="grid grid-cols-1 gap-y-1 @min-[520px]:grid-cols-[minmax(0,1fr)_10rem_minmax(0,12rem)_4.5rem] @min-[520px]:items-center @min-[520px]:gap-x-3">
      <div className="flex min-w-0 items-center gap-x-3 @min-[520px]:contents">
        <span className="min-w-0 flex-1 truncate font-mono" title={job.file}>
          {job.file || "…"}
        </span>
        <span className="shrink-0 text-right whitespace-nowrap text-muted-foreground @min-[520px]:min-w-0 @min-[520px]:shrink @min-[520px]:truncate @min-[520px]:text-left">
          {stage}
        </span>
      </div>
      <div className="flex items-center gap-x-3 @min-[520px]:contents">
        <div className="min-w-0 flex-1">
          <Bar job={job} failed={failed} />
        </div>
        <span className="w-14 shrink-0 text-right tabular-nums @min-[520px]:w-auto">
          {formatPercent(percent)}
        </span>
      </div>
    </div>
  )
}

/** One run: its command, then bars, notes and the final status. */
function RunSection({ run }: { run: RunEntry }) {
  const { progress, status } = run
  const running = status.kind === "running"
  const failed = !running && status.kind !== "success"
  const overall = overallPercent(progress.jobs)
  const doneCount = progress.jobs.filter((j) => j.done).length
  const silentCli = !running && !progress.cliReports

  return (
    <section className="flex flex-col gap-2">
      <p
        className="truncate font-mono text-muted-foreground"
        title={run.command}
      >
        {run.command}
      </p>
      {overall !== null && (
        <p className="font-medium">
          {doneCount} of {progress.jobs.length} files done -{" "}
          {overall.toFixed(0)}%
        </p>
      )}
      {progress.jobs.map((job, i) => (
        <JobRow key={i} job={job} failed={failed} />
      ))}
      {progress.notes.map((n, i) => (
        <NoteLine key={i} tone={n.tone} text={n.text} />
      ))}
      {silentCli && (
        <NoteLine
          tone="warning"
          text="This deew CLI does not report progress (needs deew 3.6.2 or newer), use the CLI output."
        />
      )}
      {!running && (
        <p className="flex items-start gap-1">
          {status.kind === "success" ? (
            <CircleCheck className="mt-[3px] size-3 shrink-0 text-green-600" />
          ) : (
            <CircleX className="mt-[3px] size-3 shrink-0 text-destructive" />
          )}
          <span className="min-w-0 break-words">{statusText(status)}</span>
        </p>
      )}
      {running && progress.jobs.length === 0 && (
        <p className="text-muted-foreground">Waiting for deew…</p>
      )}
    </section>
  )
}

/** Native view: every encode run of the session, oldest first. */
export function ProgressPane({ runs, className }: ProgressPaneProps) {
  const shown = runs.filter((r) => !r.info)
  const ref = useRef<HTMLDivElement>(null)
  const atBottom = useRef(true)

  // Keep following the end unless the user scrolled up to an older run.
  useLayoutEffect(() => {
    const el = ref.current
    if (el && atBottom.current) el.scrollTop = el.scrollHeight
  })

  return (
    <div
      ref={ref}
      onScroll={(e) => {
        const el = e.currentTarget
        atBottom.current =
          el.scrollHeight - el.scrollTop - el.clientHeight <= 24
      }}
      className={cn(
        "@container flex flex-col gap-3 overflow-y-auto rounded-lg border border-input bg-background p-3 text-xs",
        className
      )}
    >
      {shown.length === 0 && (
        <p className="text-muted-foreground">
          Progress of the encodes is shown here.
        </p>
      )}
      {shown.map((run, i) => (
        <div
          key={run.id}
          className={cn("flex flex-col", i > 0 && "border-t pt-3")}
        >
          <RunSection run={run} />
        </div>
      ))}
    </div>
  )
}
