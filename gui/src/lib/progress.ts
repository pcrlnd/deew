// Pure model of the native progress view: the event lines of the CLI
// (`DEEW_EVENTS_FILE`), the state they build up and the labels derived from it.

import { formatDuration } from "@/lib/terminal-format"

export type Stage = "starting" | "ffmpeg" | "decode" | "measure" | "encode"

export type Gating = "dialogue" | "level"

export type Decoder = "harletty" | "truehdd"

export type JobResult = "finished" | "failed" | "stopped"

export type ProgressEvent =
  | { type: "hello"; version: string }
  | { type: "jobs"; files: string[] }
  | {
      type: "progress"
      job: number
      stage: Stage
      percent: number | null
      db: number | null
      /** Decode stage only (newer CLIs): the decoder in use. */
      decoder?: Decoder
      /** `-mo` only: how the measurement was gated and the speech share. */
      gating?: Gating
      speech?: number
      /** `-mo` only: program loudness (LKFS), loudness range (LU), true peak (dBTP). */
      program?: number
      lra?: number
      true_peak?: number
    }
  | {
      type: "done"
      job: number
      /** Absent from an older CLI: finished, duration unknown. */
      result?: JobResult
      /** Whole seconds the job took. */
      took?: number
      /** Plain-text reason of a failed job. */
      error?: string
    }
  | { type: "warning"; text: string }
  | { type: "error"; text: string }
  | { type: "prompt"; text: string }
  | { type: "answered" }

const STAGES: readonly string[] = [
  "starting",
  "ffmpeg",
  "decode",
  "measure",
  "encode",
]

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
        ...(m.decoder === "harletty" || m.decoder === "truehdd"
          ? { decoder: m.decoder }
          : {}),
        ...(m.gating === "dialogue" || m.gating === "level"
          ? { gating: m.gating }
          : {}),
        ...(typeof m.speech === "number" ? { speech: m.speech } : {}),
        ...(typeof m.program === "number" ? { program: m.program } : {}),
        ...(typeof m.lra === "number" ? { lra: m.lra } : {}),
        ...(typeof m.true_peak === "number" ? { true_peak: m.true_peak } : {}),
      }
    case "done":
      if (!isIndex(m.job)) return null
      return {
        type: "done",
        job: m.job,
        ...(m.result === "finished" ||
        m.result === "failed" ||
        m.result === "stopped"
          ? { result: m.result }
          : {}),
        ...(typeof m.took === "number" && Number.isFinite(m.took) && m.took >= 0
          ? { took: Math.floor(m.took) }
          : {}),
        ...(typeof m.error === "string" ? { error: m.error } : {}),
      }
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
  /** Decoder of the decode stage; absent from an older CLI (= truehdd). */
  decoder?: Decoder
  gating?: Gating
  speech?: number
  program?: number
  lra?: number
  true_peak?: number
  /** The job is over (finished, failed or stopped). */
  done: boolean
  /** How it ended; an old CLI sends none (= finished). */
  result?: JobResult
  took?: number
  error?: string
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
          decoder: event.decoder,
          gating: event.gating,
          speech: event.speech,
          program: event.program,
          lra: event.lra,
          true_peak: event.true_peak,
        })),
      }
    case "done":
      return {
        ...state,
        jobs: withJob(state.jobs, event.job, (job) => ({
          ...job,
          percent:
            event.result === "failed" || event.result === "stopped"
              ? job.percent
              : 100,
          done: true,
          result: event.result,
          took: event.took,
          error: event.error,
        })),
      }
    case "warning":
    case "error":
      return {
        ...state,
        notes: [
          ...state.notes,
          {
            // deew reports its Ctrl-C handling as an error; that is a stop
            // the user asked for, not a failure.
            tone:
              event.type === "error" && event.text === "interrupted"
                ? "warning"
                : event.type,
            text: event.text,
          },
        ],
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
  ffmpeg: "ffmpeg decode",
  decode: "truehdd decode",
  measure: "DEE measure",
  encode: "DEE encode",
}

export function stageLabel(job: JobProgress): string {
  const label =
    job.stage === "decode"
      ? `${job.decoder ?? "truehdd"} decode`
      : STAGE_LABELS[job.stage]
  const level = levelText(job)
  return level ? `${label} (${level})` : label
}

/** Like the CLI: `-20 dB, dialogue-gated, 37% speech, program -18.9 LKFS,
 * LRA 13.1 LU, true peak -4.9 dBTP`; empty without a level. */
function levelText(job: JobProgress): string {
  if (job.db === null) return ""
  const parts = [`${job.db} dB`]
  if (job.gating) parts.push(`${job.gating}-gated`)
  if (job.speech !== undefined) parts.push(`${job.speech}% speech`)
  if (job.program !== undefined && job.gating === "dialogue")
    parts.push(`program ${job.program.toFixed(1)} LKFS`)
  if (job.lra !== undefined) parts.push(`LRA ${job.lra.toFixed(1)} LU`)
  if (job.true_peak !== undefined)
    parts.push(`true peak ${job.true_peak.toFixed(1)} dBTP`)
  return parts.join(", ")
}

/** `10:15` or `1:02:05` like the CLI's `clock`. */
export function formatClock(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds))
  const h = Math.floor(total / 3600)
  const m = Math.floor(total / 60) % 60
  const s = total % 60
  const ss = String(s).padStart(2, "0")
  return h > 0 ? `${h}:${String(m).padStart(2, "0")}:${ss}` : `${m}:${ss}`
}

/**
 * The status of a job like the CLI shows it, without the file name: the
 * stage while running, `finished (-25 dB) - took 10:15`, `failed: reason`
 * or `stopped`.
 */
export function jobStatusText(job: JobProgress): string {
  if (!job.done) return stageLabel(job)
  if (job.result === "failed")
    return job.error ? `failed: ${job.error}` : "failed"
  if (job.result === "stopped") return "stopped"
  const level = levelText(job)
  const took = job.took === undefined ? "" : ` - took ${formatClock(job.took)}`
  return `finished${level ? ` (${level})` : ""}${took}`
}

/** `45.55%` like the CLI bar: `100%` when finished, at most `99.99%` before. */
export function formatPercent(percent: number | null): string {
  if (percent === null) return ""
  if (percent >= 100) return "100%"
  return `${Math.min(percent, 99.99).toFixed(2)}%`
}

/** Whether the job finished (an old CLI's `done` counts as finished). */
export const isFinished = (job: JobProgress): boolean =>
  job.done && (job.result === undefined || job.result === "finished")

/** Mean percent over all jobs (finished = 100, unknown = 0); null without jobs. */
export function overallPercent(jobs: JobProgress[]): number | null {
  if (jobs.length === 0) return null
  const sum = jobs.reduce(
    (total, job) => total + (isFinished(job) ? 100 : (job.percent ?? 0)),
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

/** One run in the history of the native view. */
export interface RunEntry {
  id: number
  command: string
  progress: ProgressState
  status: RunStatus
}

export interface RunHistory {
  runs: RunEntry[]
  nextId: number
}

export const EMPTY_HISTORY: RunHistory = { runs: [], nextId: 1 }

/** Number of runs kept in the history. */
export const MAX_RUNS = 50

export type HistoryAction =
  | { type: "begin"; command: string }
  | { type: "event"; event: ProgressEvent }
  | { type: "end"; status: RunStatus }
  | { type: "restart" }

function updateLast(
  history: RunHistory,
  update: (entry: RunEntry) => RunEntry
): RunHistory {
  if (history.runs.length === 0) return history
  const runs = [...history.runs]
  runs[runs.length - 1] = update(runs[runs.length - 1])
  return { ...history, runs }
}

/** Applies one action; never mutates `history`. */
export function reduceHistory(
  history: RunHistory,
  action: HistoryAction
): RunHistory {
  switch (action.type) {
    case "begin": {
      const entry: RunEntry = {
        id: history.nextId,
        command: action.command,
        progress: EMPTY_PROGRESS,
        status: { kind: "running" },
      }
      return {
        runs: [...history.runs, entry].slice(-MAX_RUNS),
        nextId: history.nextId + 1,
      }
    }
    case "event":
      return updateLast(history, (e) => ({
        ...e,
        progress: reduceProgress(e.progress, action.event),
      }))
    case "end":
      return updateLast(history, (e) => ({
        ...e,
        status: action.status,
        progress: reduceProgress(e.progress, { type: "answered" }),
      }))
    case "restart":
      return updateLast(history, (e) => ({
        ...e,
        progress: EMPTY_PROGRESS,
        status: { kind: "running" },
      }))
  }
}

/** Bar colours, the CLI's 256-colour codes (src/ui.rs). */
export const BAR_COLORS = {
  decode: "#00afff",
  measure: "#ffaf00",
  encode: "#ff005f",
  finished: "#5faf00",
  failed: "#d70000",
  stopped: "#8a8a8a",
} as const

/**
 * Fill colour of a job's bar and whether it is dimmed. Green only for a
 * finished job; an unfinished job of a failed run keeps its stage colour,
 * dimmed.
 */
export function barColor(
  job: JobProgress,
  runFailed: boolean
): { color: string; dim: boolean } {
  if (isFinished(job)) return { color: BAR_COLORS.finished, dim: false }
  if (job.result === "failed") return { color: BAR_COLORS.failed, dim: false }
  if (job.result === "stopped") return { color: BAR_COLORS.stopped, dim: false }
  const color =
    job.stage === "measure"
      ? BAR_COLORS.measure
      : job.stage === "encode"
        ? BAR_COLORS.encode
        : BAR_COLORS.decode
  return { color, dim: runFailed }
}
