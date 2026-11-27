import { describe, expect, it } from "vitest"

import {
  barColor,
  BAR_COLORS,
  EMPTY_HISTORY,
  EMPTY_PROGRESS,
  formatClock,
  formatPercent,
  isFinished,
  jobStatusText,
  overallPercent,
  parseProgressEvent,
  reduceHistory,
  reduceProgress,
  stageLabel,
  statusText,
  type JobProgress,
  type RunHistory,
  type ProgressEvent,
  type ProgressState,
} from "./progress"

const job = (over: Partial<JobProgress> = {}): JobProgress => ({
  file: "a.ac3",
  stage: "waiting",
  percent: 0,
  db: null,
  done: false,
  ...over,
})

describe("parseProgressEvent", () => {
  it("parses every event type", () => {
    expect(parseProgressEvent('{"type":"hello","version":"3.7.0"}')).toEqual({
      type: "hello",
      version: "3.7.0",
    })
    expect(parseProgressEvent('{"type":"jobs","files":["a","b"]}')).toEqual({
      type: "jobs",
      files: ["a", "b"],
    })
    expect(
      parseProgressEvent(
        '{"type":"progress","job":1,"stage":"measure","percent":12.5,"db":-3.2}'
      )
    ).toEqual({
      type: "progress",
      job: 1,
      stage: "measure",
      percent: 12.5,
      db: -3.2,
    })
    expect(
      parseProgressEvent(
        '{"type":"progress","job":2,"stage":"decode","percent":40,"db":null}'
      )
    ).toEqual({
      type: "progress",
      job: 2,
      stage: "decode",
      percent: 40,
      db: null,
    })
    expect(
      parseProgressEvent(
        '{"type":"progress","job":0,"stage":"ffmpeg","percent":null,"db":null}'
      )
    ).toEqual({
      type: "progress",
      job: 0,
      stage: "ffmpeg",
      percent: null,
      db: null,
    })
    expect(
      parseProgressEvent(
        '{"type":"progress","job":1,"stage":"measure","percent":null,"db":-20,"gating":"dialogue","speech":37,"program":-18.9,"lra":13.1,"true_peak":null}'
      )
    ).toEqual({
      type: "progress",
      job: 1,
      stage: "measure",
      percent: null,
      db: -20,
      gating: "dialogue",
      speech: 37,
      program: -18.9,
      lra: 13.1,
    })
    expect(parseProgressEvent('{"type":"done","job":2}')).toEqual({
      type: "done",
      job: 2,
    })
    expect(parseProgressEvent('{"type":"warning","text":"w"}')).toEqual({
      type: "warning",
      text: "w",
    })
    expect(parseProgressEvent('{"type":"error","text":"e"}')).toEqual({
      type: "error",
      text: "e",
    })
    expect(parseProgressEvent('{"type":"prompt","text":"Sure?"}')).toEqual({
      type: "prompt",
      text: "Sure?",
    })
    expect(parseProgressEvent('{"type":"answered","extra":1}')).toEqual({
      type: "answered",
    })
  })

  it("rejects invalid and unknown lines", () => {
    for (const line of [
      "",
      "nope",
      "42",
      "null",
      '{"type":"nope"}',
      '{"type":"hello"}',
      '{"type":"jobs","files":[1]}',
      '{"type":"progress","job":-1,"stage":"encode","percent":1,"db":null}',
      '{"type":"progress","job":0,"stage":"bogus","percent":1,"db":null}',
      '{"type":"progress","job":0,"stage":"encode","percent":"1","db":null}',
      '{"type":"done"}',
      '{"type":"warning"}',
      '{"type":"prompt","text":1}',
    ])
      expect(parseProgressEvent(line)).toBeNull()
  })
})

describe("decoder of progress events", () => {
  it("parses the decoder and ignores unknown ones", () => {
    const line = (d: string) =>
      `{"type":"progress","job":0,"stage":"decode","decoder":"${d}","percent":1,"db":null}`
    expect(parseProgressEvent(line("harletty"))).toMatchObject({
      decoder: "harletty",
    })
    expect(parseProgressEvent(line("truehdd"))).toMatchObject({
      decoder: "truehdd",
    })
    expect(parseProgressEvent(line("x"))).not.toHaveProperty("decoder")
    expect(
      parseProgressEvent(
        '{"type":"progress","job":0,"stage":"decode","percent":1,"db":null}'
      )
    ).not.toHaveProperty("decoder")
  })

  it("reduces the decoder into the job", () => {
    const events: ProgressEvent[] = [
      { type: "jobs", files: ["a.mkv"] },
      {
        type: "progress",
        job: 0,
        stage: "decode",
        decoder: "harletty",
        percent: 5,
        db: null,
      },
    ]
    const state = events.reduce(reduceProgress, EMPTY_PROGRESS)
    expect(stageLabel(state.jobs[0])).toBe("harletty decode")
  })
})

describe("reduceProgress", () => {
  const apply = (events: ProgressEvent[], from = EMPTY_PROGRESS) =>
    events.reduce(reduceProgress, from)

  it("follows a decode stage followed by the encode stage", () => {
    const state = apply([
      { type: "jobs", files: ["a.thd"] },
      { type: "progress", job: 0, stage: "decode", percent: 50, db: null },
    ])
    expect(stageLabel(state.jobs[0])).toBe("truehdd decode")
    const next = apply(
      [{ type: "progress", job: 0, stage: "encode", percent: 10, db: null }],
      state
    )
    expect(next.jobs[0]).toMatchObject({ stage: "encode", percent: 10 })
    expect(stageLabel(next.jobs[0])).toBe("DEE encode")
  })

  it("follows a run", () => {
    const state = apply([
      { type: "hello", version: "1" },
      { type: "jobs", files: ["a", "b"] },
    ])
    expect(state.cliReports).toBe(true)
    expect(state.jobs).toEqual([job({ file: "a" }), job({ file: "b" })])
    const next = apply(
      [
        {
          type: "progress",
          job: 0,
          stage: "encode",
          percent: 40,
          db: -2,
        },
        { type: "done", job: 1 },
      ],
      state
    )
    expect(next.jobs[0]).toEqual(
      job({ file: "a", stage: "encode", percent: 40, db: -2 })
    )
    expect(next.jobs[1]).toEqual(job({ file: "b", percent: 100, done: true }))
  })

  it("grows the list for an unknown job", () => {
    const state = reduceProgress(EMPTY_PROGRESS, {
      type: "progress",
      job: 2,
      stage: "ffmpeg",
      percent: null,
      db: null,
    })
    expect(state.jobs).toHaveLength(3)
    expect(state.jobs[2]).toMatchObject({ file: "", stage: "ffmpeg" })
    expect(state.jobs[0]).toMatchObject({ file: "", stage: "waiting" })
  })

  it("collects notes and handles the prompt", () => {
    let state = apply([
      { type: "warning", text: "w" },
      { type: "error", text: "e" },
      { type: "prompt", text: "Overwrite?" },
    ])
    expect(state.notes).toEqual([
      { tone: "warning", text: "w" },
      { tone: "error", text: "e" },
    ])
    expect(state.prompt).toBe("Overwrite?")
    state = reduceProgress(state, { type: "answered" })
    expect(state.prompt).toBeNull()
  })

  it("shows deew's 'interrupted' error as a warning", () => {
    const state = apply([{ type: "error", text: "interrupted" }])
    expect(state.notes).toEqual([{ tone: "warning", text: "interrupted" }])
  })

  it("never mutates its input", () => {
    const base: ProgressState = {
      jobs: [job({ file: "a" })],
      notes: [{ tone: "warning", text: "w" }],
      prompt: null,
      cliReports: false,
    }
    const copy = structuredClone(base)
    const events: ProgressEvent[] = [
      { type: "hello", version: "1" },
      { type: "jobs", files: ["x"] },
      { type: "progress", job: 3, stage: "encode", percent: 1, db: null },
      { type: "done", job: 0 },
      { type: "warning", text: "n" },
      { type: "prompt", text: "q" },
      { type: "answered" },
    ]
    for (const event of events) {
      reduceProgress(base, event)
      expect(base).toEqual(copy)
    }
    expect(EMPTY_PROGRESS).toEqual({
      jobs: [],
      notes: [],
      prompt: null,
      cliReports: false,
    })
  })
})

describe("stageLabel", () => {
  it("labels the decode stage with the decoder", () => {
    expect(stageLabel(job({ stage: "decode", decoder: "harletty" }))).toBe(
      "harletty decode"
    )
    expect(stageLabel(job({ stage: "decode", decoder: "truehdd" }))).toBe(
      "truehdd decode"
    )
  })

  it("names the stages and appends the loudness", () => {
    expect(stageLabel(job())).toBe("waiting")
    expect(stageLabel(job({ stage: "starting" }))).toBe("starting")
    expect(stageLabel(job({ stage: "ffmpeg" }))).toBe("ffmpeg decode")
    expect(stageLabel(job({ stage: "decode" }))).toBe("truehdd decode")
    expect(stageLabel(job({ stage: "measure" }))).toBe("DEE measure")
    expect(
      stageLabel(
        job({ stage: "measure", db: -33, gating: "dialogue", speech: 16 })
      )
    ).toBe("DEE measure (-33 dB, dialogue-gated, 16% speech)")
    expect(
      stageLabel(job({ stage: "measure", db: -25, gating: "level" }))
    ).toBe("DEE measure (-25 dB, level-gated)")
    expect(
      stageLabel(
        job({
          stage: "measure",
          db: -20,
          gating: "dialogue",
          speech: 37,
          program: -18.9,
          lra: 13.1,
          true_peak: -4.9,
        })
      )
    ).toBe(
      "DEE measure (-20 dB, dialogue-gated, 37% speech, program -18.9 LKFS, LRA 13.1 LU, true peak -4.9 dBTP)"
    )
    expect(
      stageLabel(
        job({
          stage: "measure",
          db: -20,
          gating: "level",
          program: -20,
          lra: 13,
          true_peak: 0,
        })
      )
    ).toBe("DEE measure (-20 dB, level-gated, LRA 13.0 LU, true peak 0.0 dBTP)")
    expect(stageLabel(job({ stage: "encode", db: -3.5 }))).toBe(
      "DEE encode (-3.5 dB)"
    )
  })
})

describe("overallPercent", () => {
  it("averages, counting done as 100 and unknown as 0", () => {
    expect(overallPercent([])).toBeNull()
    expect(
      overallPercent([
        job({ done: true, percent: 100 }),
        job({ percent: 50 }),
        job({ percent: null }),
        job({ done: true, percent: 20 }),
      ])
    ).toBe(62.5)
  })
})

describe("statusText", () => {
  it("describes every outcome", () => {
    expect(statusText({ kind: "running" })).toBe("Running")
    expect(statusText({ kind: "success", ms: 5550 })).toBe("Ran for 5.55sec")
    expect(statusText({ kind: "success", ms: null })).toBe("Finished")
    expect(statusText({ kind: "exit", code: 2 })).toBe("Exited with code 2")
    expect(statusText({ kind: "exit", code: null })).toBe(
      "Exited with code unknown"
    )
    expect(statusText({ kind: "stopped" })).toBe("Stopped")
    expect(statusText({ kind: "error", message: "boom" })).toBe("boom")
  })
})

describe("formatPercent", () => {
  it("keeps two decimals until the job is finished", () => {
    expect(formatPercent(45.55)).toBe("45.55%")
    expect(formatPercent(0)).toBe("0.00%")
    expect(formatPercent(99.996)).toBe("99.99%")
    expect(formatPercent(100)).toBe("100%")
    expect(formatPercent(null)).toBe("")
  })
})

describe("reduceHistory", () => {
  const begin = (h: RunHistory, command: string) =>
    reduceHistory(h, { type: "begin", command })
  const jobs: ProgressEvent = { type: "jobs", files: ["a.ac3"] }

  it("appends a running entry on begin", () => {
    const h = begin(begin(EMPTY_HISTORY, "one"), "two")
    expect(h.runs.map((r) => r.command)).toEqual(["one", "two"])
    expect(h.runs[1].status).toEqual({ kind: "running" })
    expect(h.runs[1].progress).toBe(EMPTY_PROGRESS)
    expect(h.runs[0].id).not.toBe(h.runs[1].id)
  })

  it("applies events to the last entry only", () => {
    const h = reduceHistory(begin(begin(EMPTY_HISTORY, "a"), "b"), {
      type: "event",
      event: jobs,
    })
    expect(h.runs[0].progress).toBe(EMPTY_PROGRESS)
    expect(h.runs[1].progress.jobs).toHaveLength(1)
  })

  it("ignores events without entries", () => {
    expect(reduceHistory(EMPTY_HISTORY, { type: "event", event: jobs })).toBe(
      EMPTY_HISTORY
    )
  })

  it("sets the status on end and clears the prompt", () => {
    let h = begin(EMPTY_HISTORY, "a")
    h = reduceHistory(h, {
      type: "event",
      event: { type: "prompt", text: "Overwrite?" },
    })
    expect(h.runs[0].progress.prompt).toBe("Overwrite?")
    h = reduceHistory(h, { type: "end", status: { kind: "stopped" } })
    expect(h.runs[0].status).toEqual({ kind: "stopped" })
    expect(h.runs[0].progress.prompt).toBeNull()
  })

  it("restart resets only the last entry", () => {
    let h = begin(begin(EMPTY_HISTORY, "a"), "b")
    h = reduceHistory(h, { type: "event", event: jobs })
    h = reduceHistory(h, { type: "end", status: { kind: "stopped" } })
    const first = h.runs[0]
    h = reduceHistory(h, { type: "restart" })
    expect(h.runs[0]).toBe(first)
    expect(h.runs[1].progress).toBe(EMPTY_PROGRESS)
    expect(h.runs[1].status).toEqual({ kind: "running" })
    expect(h.runs[1].command).toBe("b")
  })

  it("keeps at most 50 entries, dropping the oldest", () => {
    let h = EMPTY_HISTORY
    for (let i = 0; i < 55; i++) h = begin(h, `run ${i}`)
    expect(h.runs).toHaveLength(50)
    expect(h.runs[0].command).toBe("run 5")
    expect(h.runs[49].command).toBe("run 54")
  })
})

describe("done events", () => {
  const apply = (events: ProgressEvent[], from = EMPTY_PROGRESS) =>
    events.reduce(reduceProgress, from)

  it("parses the outcome, duration and reason", () => {
    expect(
      parseProgressEvent(
        '{"type":"done","job":1,"result":"finished","took":615}'
      )
    ).toEqual({ type: "done", job: 1, result: "finished", took: 615 })
    expect(
      parseProgressEvent(
        '{"type":"done","job":0,"result":"failed","took":3,"error":"DEE: oops"}'
      )
    ).toEqual({
      type: "done",
      job: 0,
      result: "failed",
      took: 3,
      error: "DEE: oops",
    })
    expect(
      parseProgressEvent('{"type":"done","job":0,"result":"stopped","took":0}')
    ).toEqual({ type: "done", job: 0, result: "stopped", took: 0 })
  })

  it("ignores invalid extras and keeps an old CLI's done", () => {
    expect(
      parseProgressEvent('{"type":"done","job":0,"result":"x","took":"1"}')
    ).toEqual({ type: "done", job: 0 })
    expect(parseProgressEvent('{"type":"done","job":0,"took":-1}')).toEqual({
      type: "done",
      job: 0,
    })
  })

  it("stores the outcome; failed and stopped keep their percent", () => {
    const base = apply([
      { type: "jobs", files: ["a", "b", "c"] },
      { type: "progress", job: 1, stage: "encode", percent: 40, db: -25 },
    ])
    const state = apply(
      [
        { type: "done", job: 0, result: "finished", took: 5 },
        { type: "done", job: 1, result: "failed", took: 9, error: "oops" },
        { type: "done", job: 2 },
      ],
      base
    )
    expect(state.jobs[0]).toMatchObject({ done: true, percent: 100, took: 5 })
    expect(state.jobs[1]).toMatchObject({
      done: true,
      percent: 40,
      result: "failed",
      error: "oops",
    })
    expect(state.jobs[2]).toMatchObject({ done: true, percent: 100 })
    expect(state.jobs.map(isFinished)).toEqual([true, false, true])
    expect(overallPercent(state.jobs)).toBeCloseTo((100 + 40 + 100) / 3)
  })
})

describe("jobStatusText", () => {
  it("formats the clock like the CLI", () => {
    expect(formatClock(0)).toBe("0:00")
    expect(formatClock(65)).toBe("1:05")
    expect(formatClock(615)).toBe("10:15")
    expect(formatClock(3600)).toBe("1:00:00")
    expect(formatClock(3725)).toBe("1:02:05")
  })

  it("shows the stage while running", () => {
    expect(jobStatusText(job({ stage: "encode", db: -25, percent: 3 }))).toBe(
      "DEE encode (-25 dB)"
    )
  })

  it("shows finished with and without level and duration", () => {
    const done = { done: true, percent: 100 }
    expect(
      jobStatusText(job({ ...done, result: "finished", db: -25, took: 615 }))
    ).toBe("finished (-25 dB) - took 10:15")
    expect(jobStatusText(job({ ...done, result: "finished", took: 5 }))).toBe(
      "finished - took 0:05"
    )
    expect(
      jobStatusText(job({ ...done, result: "finished", took: 3725 }))
    ).toBe("finished - took 1:02:05")
    expect(
      jobStatusText(
        job({
          ...done,
          result: "finished",
          took: 5,
          db: -24,
          gating: "dialogue",
          speech: 45,
          program: -21.6,
          lra: 10.3,
          true_peak: -2.8,
        })
      )
    ).toBe(
      "finished (-24 dB, dialogue-gated, 45% speech, program -21.6 LKFS, LRA 10.3 LU, true peak -2.8 dBTP) - took 0:05"
    )
  })

  it("handles an old CLI's done", () => {
    expect(jobStatusText(job({ done: true, percent: 100 }))).toBe("finished")
    expect(jobStatusText(job({ done: true, percent: 100, db: -25 }))).toBe(
      "finished (-25 dB)"
    )
  })

  it("shows failed and stopped", () => {
    expect(
      jobStatusText(job({ done: true, result: "failed", error: "DEE: oops" }))
    ).toBe("failed: DEE: oops")
    expect(jobStatusText(job({ done: true, result: "failed" }))).toBe("failed")
    expect(jobStatusText(job({ done: true, result: "stopped" }))).toBe(
      "stopped"
    )
  })
})

describe("barColor", () => {
  it("uses the CLI stage colours", () => {
    expect(barColor(job({ stage: "ffmpeg" }), false).color).toBe("#00afff")
    expect(barColor(job({ stage: "decode" }), false).color).toBe("#00afff")
    expect(barColor(job({ stage: "measure" }), false).color).toBe("#ffaf00")
    expect(barColor(job({ stage: "encode" }), false).color).toBe("#ff005f")
  })

  it("is green only for a finished job", () => {
    const done = job({ stage: "encode", done: true, percent: 100 })
    expect(barColor(done, false)).toEqual({
      color: BAR_COLORS.finished,
      dim: false,
    })
    expect(barColor({ ...done, result: "finished" }, true).color).toBe(
      BAR_COLORS.finished
    )
    expect(barColor(job({ stage: "encode" }), true).color).not.toBe(
      BAR_COLORS.finished
    )
  })

  it("marks failed and stopped jobs", () => {
    const end = { stage: "encode", done: true } as const
    expect(barColor(job({ ...end, result: "failed" }), true).color).toBe(
      BAR_COLORS.failed
    )
    expect(barColor(job({ ...end, result: "stopped" }), true).color).toBe(
      BAR_COLORS.stopped
    )
  })

  it("dims the stage colour of a running job in a failed run", () => {
    expect(barColor(job({ stage: "measure" }), true)).toEqual({
      color: "#ffaf00",
      dim: true,
    })
    expect(barColor(job({ stage: "measure" }), false).dim).toBe(false)
  })
})
