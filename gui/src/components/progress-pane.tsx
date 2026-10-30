import { CircleCheck, CircleX } from "lucide-react"

import { NoteLine } from "@/components/note-line"
import {
  overallPercent,
  stageLabel,
  statusText,
  type JobProgress,
  type ProgressState,
  type RunStatus,
} from "@/lib/progress"
import { cn } from "@/lib/utils"

interface ProgressPaneProps {
  progress: ProgressState
  /** Null before the first run. */
  status: RunStatus | null
  /** The current / last run was started in native mode. */
  native: boolean
  /** The current / last run only prints information (no encode, no bars). */
  info: boolean
  className?: string
}

const formatPercent = (percent: number | null): string =>
  percent === null ? "" : `${percent.toFixed(2)}%`

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

/** One job: file name, stage, bar and percent (stacked when narrow). */
function JobRow({ job, failed }: { job: JobProgress; failed: boolean }) {
  const percent = job.done ? 100 : job.percent
  const stage = job.done ? "done" : stageLabel(job)
  return (
    <div className="grid grid-cols-[minmax(0,1fr)_auto] items-center gap-x-3 gap-y-1 @min-[520px]:grid-cols-[minmax(0,1fr)_10rem_minmax(0,12rem)_4.5rem]">
      <span
        className="order-1 truncate font-mono @min-[520px]:order-none"
        title={job.file}
      >
        {job.file || "…"}
      </span>
      <span className="order-3 col-span-2 truncate text-muted-foreground @min-[520px]:order-none @min-[520px]:col-span-1">
        {stage}
      </span>
      <div className="order-4 col-span-2 @min-[520px]:order-none @min-[520px]:col-span-1">
        <Bar job={job} failed={failed} />
      </div>
      <span className="order-2 text-right tabular-nums @min-[520px]:order-none">
        {formatPercent(percent)}
      </span>
    </div>
  )
}

/** Native view of a run: progress bars, warnings, errors and the status. */
export function ProgressPane({
  progress,
  status,
  native,
  info,
  className,
}: ProgressPaneProps) {
  const running = status?.kind === "running"
  const ended = status !== null && !running
  const failed = ended && status.kind !== "success"
  const overall = overallPercent(progress.jobs)
  const doneCount = progress.jobs.filter((j) => j.done).length
  const silentCli =
    native && ended && !info && !progress.cliReports && status !== null

  return (
    <div
      className={cn(
        "@container flex flex-col gap-2 overflow-y-auto rounded-lg border border-input bg-background p-3 text-xs",
        className
      )}
    >
      {overall !== null && !info && (
        <p className="font-medium">
          {doneCount} of {progress.jobs.length} files done ·{" "}
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
      {ended && (
        <p className="flex items-start gap-1">
          {status.kind === "success" ? (
            <CircleCheck className="mt-[3px] size-3 shrink-0 text-green-600" />
          ) : (
            <CircleX className="mt-[3px] size-3 shrink-0 text-destructive" />
          )}
          <span className="min-w-0 break-words">{statusText(status)}</span>
        </p>
      )}
      {status === null && (
        <p className="text-muted-foreground">
          Progress of the encodes is shown here.
        </p>
      )}
      {running && progress.jobs.length === 0 && !info && (
        <p className="text-muted-foreground">
          {native
            ? "Waiting for deew…"
            : "This run was started with the CLI output."}
        </p>
      )}
    </div>
  )
}
