import { useLayoutEffect, useRef } from "react"
import { CircleCheck, CircleX } from "lucide-react"

import { NoteLine } from "@/components/note-line"
import {
  barColor,
  formatPercent,
  isFinished,
  jobStatusText,
  overallPercent,
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
  const finished = isFinished(job)
  const { color, dim } = barColor(job, failed)
  return (
    <div className="relative h-1.5 overflow-hidden rounded-full bg-muted-foreground/25">
      {indeterminate ? (
        <div
          className="absolute inset-y-0 w-2/5 rounded-full motion-safe:animate-indeterminate motion-reduce:w-full motion-reduce:opacity-40"
          style={{ backgroundColor: color, opacity: dim ? 0.5 : undefined }}
        />
      ) : (
        <div
          className="h-full rounded-full transition-[width] duration-100"
          style={{
            width: `${finished ? 100 : (job.percent ?? 0)}%`,
            backgroundColor: color,
            opacity: dim ? 0.5 : undefined,
          }}
        />
      )}
    </div>
  )
}

/**
 * One job, two lines: the file name, then the status (like the CLI shows it),
 * the bar and the percent. Odd rows get a tinted background.
 */
function JobRow({
  job,
  failed,
  odd,
}: {
  job: JobProgress
  failed: boolean
  odd: boolean
}) {
  const percent = isFinished(job) ? 100 : job.percent
  const status = jobStatusText(job)
  return (
    <div
      className={cn(
        "flex min-w-0 flex-col gap-y-1 rounded-md px-2 py-1.5",
        odd && "bg-neutral-100 dark:bg-muted/60"
      )}
    >
      <span className="min-w-0 truncate font-mono" title={job.file}>
        {job.file || "…"}
      </span>
      <div className="flex min-w-0 items-center gap-x-3">
        <span
          className={cn(
            "min-w-0 flex-1 truncate",
            job.done && job.result === "failed"
              ? "text-destructive"
              : "text-muted-foreground"
          )}
          title={status}
        >
          {status}
        </span>
        <div className="w-20 shrink-[2] @min-[520px]:w-40">
          <Bar job={job} failed={failed} />
        </div>
        <span className="w-14 shrink-0 text-right tabular-nums">
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
  const doneCount = progress.jobs.filter(isFinished).length
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
        <JobRow key={i} job={job} failed={failed} odd={i % 2 === 1} />
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
      {runs.length === 0 && (
        <p className="text-muted-foreground">
          Progress of the encodes is shown here.
        </p>
      )}
      {runs.map((run, i) => (
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
