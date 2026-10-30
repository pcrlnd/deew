// Pure model of the native progress view: the event lines of the CLI
// (`DEEW_EVENTS_FILE`), the state they build up and the labels derived from it.

import { formatDuration } from "@/lib/terminal-format"

export type Stage = "starting" | "ffmpeg" | "measure" | "encode"

export type ProgressEvent =
  | { type: "hello"; version: string }
  | { type: "jobs"; files: string[] }
  | {
      type: "progress"
      job: number
      stage: Stage
      percent: number | null
      db: number | null
    }
  | { type: "done"; job: number }
  | { type: "warning"; text: string }
  | { type: "error"; text: string }
  | { type: "prompt"; text: string }
  | { type: "answered" }

const STAGES: readonly string[] = ["starting", "ffmpeg", "measure", "encode"]

const isIndex = (value: unknown): value is number =>
  typeof value === "number" && Number.isInteger(value) && value >= 0

const isNumberOrNull = (value: unknown): value is number | null =>
  value === null || (typeof value === "number" && Number.isFinite(value))

/** Parses one event line; `null` for anything invalid or unknown. */
export function parseProgressEvent(line: string): ProgressEvent | null {
  let value: unknown
  try {
    value = JSON.parse(line)
  } catch {
    return null
  }
  if (!value || typeof value !== "object") return null
  const m = value as Record<string, unknown>
  switch (m.type) {
    case "hello":
      return typeof m.version === "string"
        ? { type: "hello", version: m.version }
        : null
    case "jobs":
      return Array.isArray(m.files) &&
        m.files.every((f) => typeof f === "string")
        ? { type: "jobs", files: m.files as string[] }
        : null
    case "progress":
      if (
        !isIndex(m.job) ||
        typeof m.stage !== "string" ||
        !STAGES.includes(m.stage)
      )
        return null
      // Absent percent / db mean unknown.
      if (!isNumberOrNull(m.percent ?? null) || !isNumberOrNull(m.db ?? null))
        return null
      return {
        type: "progress",
        job: m.job,
        stage: m.stage as Stage,
        percent: (m.percent as number | null | undefined) ?? null,
        db: (m.db as number | null | undefined) ?? null,
      }
    case "done":
      return isIndex(m.job) ? { type: "done", job: m.job } : null
    case "warning":
    case "error":
    case "prompt":
      return typeof m.text === "string" ? { type: m.type, text: m.text } : null
    case "answered":
      return { type: "answered" }
    default:
      return null
  }
}

export interface JobProgress {
  file: string
  stage: "waiting" | Stage
  /** 0-100; null = unknown (indeterminate). */
  percent: number | null
  db: number | null
  done: boolean
}

export interface ProgressNote {
  tone: "warning" | "error"
  text: string
}

export interface ProgressState {
  jobs: JobProgress[]
  notes: ProgressNote[]
  /** The question of an open y/n prompt. */
  prompt: string | null
  /** Whether the CLI sent `hello`, i.e. it can report progress at all. */
  cliReports: boolean
}

export const EMPTY_PROGRESS: ProgressState = {
  jobs: [],
  notes: [],
  prompt: null,
  cliReports: false,
}

const waitingJob = (file: string): JobProgress => ({
  file,
  stage: "waiting",
  percent: 0,
  db: null,
  done: false,
})

/** Returns the list with job `index` replaced (growing it if needed). */
function withJob(
  jobs: JobProgress[],
  index: number,
  update: (job: JobProgress) => JobProgress
): JobProgress[] {
  const out = [...jobs]
  while (out.length <= index) out.push(waitingJob(""))
  out[index] = update(out[index])
  return out
}

/** Applies one event; never mutates `state`. */
export function reduceProgress(
  state: ProgressState,
  event: ProgressEvent
): ProgressState {
  switch (event.type) {
    case "hello":
      return { ...state, cliReports: true }
    case "jobs":
      return { ...state, jobs: event.files.map(waitingJob) }
    case "progress":
      return {
        ...state,
        jobs: withJob(state.jobs, event.job, (job) => ({
          ...job,
          stage: event.stage,
          percent: event.percent,
          db: event.db,
        })),
      }
    case "done":
      return {
        ...state,
        jobs: withJob(state.jobs, event.job, (job) => ({
          ...job,
          percent: 100,
          done: true,
        })),
      }
    case "warning":
    case "error":
      return {
        ...state,
        notes: [...state.notes, { tone: event.type, text: event.text }],
      }
    case "prompt":
      return { ...state, prompt: event.text }
    case "answered":
      return { ...state, prompt: null }
  }
}

const STAGE_LABELS: Record<JobProgress["stage"], string> = {
  waiting: "waiting",
  starting: "starting",
  ffmpeg: "ffmpeg",
  measure: "DEE: measure",
  encode: "DEE: encode",
}

export function stageLabel(job: JobProgress): string {
  const label = STAGE_LABELS[job.stage]
  return job.db === null ? label : `${label} (${job.db} dB)`
}

/** Mean percent over all jobs (done = 100, unknown = 0); null without jobs. */
export function overallPercent(jobs: JobProgress[]): number | null {
  if (jobs.length === 0) return null
  const sum = jobs.reduce(
    (total, job) => total + (job.done ? 100 : (job.percent ?? 0)),
    0
  )
  return sum / jobs.length
}

export type RunStatus =
  | { kind: "running" }
  | { kind: "success"; ms: number | null }
  | { kind: "exit"; code: number | null }
  | { kind: "stopped" }
  | { kind: "error"; message: string }

export function statusText(status: RunStatus): string {
  switch (status.kind) {
    case "running":
      return "Running"
    case "success":
      return status.ms === null
        ? "Finished"
        : `Ran for ${formatDuration(status.ms)}`
    case "exit":
      return `Exited with code ${status.code === null ? "unknown" : status.code}`
    case "stopped":
      return "Stopped"
    case "error":
      return status.message
  }
}

/** Utilities-menu commands (`--version` and the like) encode nothing. */
export const isInfoRun = (args: string[]): boolean => !args.includes("-i")
