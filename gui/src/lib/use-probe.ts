import { useEffect, useMemo, useRef, useState } from "react"

import { summarizeProbe, type ProbeSummary } from "@/lib/probe"
import { probeInputs } from "@/lib/runner"
import { errorMessage } from "@/lib/web-api"

export type ProbeState =
  | { status: "idle" }
  | { status: "probing" }
  | { status: "done"; summary: ProbeSummary }
  | { status: "error"; error: string }

/** Wait this long after the last change of the track index. */
const TRACK_INDEX_DEBOUNCE_MS = 300

type Outcome = { summary: ProbeSummary } | { error: string }

/**
 * Probes `inputs` whenever they or the track index change. `trackIndex` is
 * the normalized `-ti` text (`null` while the field is invalid). Responses of
 * superseded requests are dropped.
 */
export function useProbe(
  inputs: readonly string[],
  trackIndex: string | null
): ProbeState {
  const key = useMemo(
    () =>
      inputs.length === 0 || trackIndex === null
        ? null
        : JSON.stringify([inputs, trackIndex]),
    [inputs, trackIndex]
  )
  const [done, setDone] = useState<{ key: string; outcome: Outcome } | null>(
    null
  )
  const requestCounter = useRef(0)
  const lastTrackIndex = useRef(trackIndex)

  useEffect(() => {
    const changedIndex = lastTrackIndex.current !== trackIndex
    lastTrackIndex.current = trackIndex
    if (key === null || trackIndex === null) return
    const id = ++requestCounter.current
    let cancelled = false
    const current = () => !cancelled && id === requestCounter.current
    const timer = window.setTimeout(
      () => {
        probeInputs([...inputs], trackIndex).then(
          (result) => {
            if (current()) {
              setDone({
                key,
                outcome: { summary: summarizeProbe(result.files) },
              })
            }
          },
          (error) => {
            if (current()) {
              setDone({ key, outcome: { error: errorMessage(error) } })
            }
          }
        )
      },
      changedIndex ? TRACK_INDEX_DEBOUNCE_MS : 0
    )
    return () => {
      window.clearTimeout(timer)
      // Anything still in flight is stale now.
      cancelled = true
    }
  }, [key, inputs, trackIndex])

  // The result only counts for the exact inputs / index it was made for.
  if (key === null) return { status: "idle" }
  if (done === null || done.key !== key) return { status: "probing" }
  return "error" in done.outcome
    ? { status: "error", error: done.outcome.error }
    : { status: "done", summary: done.outcome.summary }
}
