import { describe, expect, it } from "vitest"

import {
  EMPTY_PROGRESS,
  isInfoRun,
  overallPercent,
  parseProgressEvent,
  reduceProgress,
  stageLabel,
  statusText,
  type JobProgress,
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
        '{"type":"progress","job":0,"stage":"ffmpeg","percent":null,"db":null}'
      )
    ).toEqual({
      type: "progress",
      job: 0,
      stage: "ffmpeg",
      percent: null,
      db: null,
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

describe("reduceProgress", () => {
  const apply = (events: ProgressEvent[], from = EMPTY_PROGRESS) =>
    events.reduce(reduceProgress, from)

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
  it("names the stages and appends the loudness", () => {
    expect(stageLabel(job())).toBe("waiting")
    expect(stageLabel(job({ stage: "starting" }))).toBe("starting")
    expect(stageLabel(job({ stage: "ffmpeg" }))).toBe("ffmpeg")
    expect(stageLabel(job({ stage: "measure" }))).toBe("DEE: measure")
    expect(stageLabel(job({ stage: "encode", db: -3.5 }))).toBe(
      "DEE: encode (-3.5 dB)"
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

describe("isInfoRun", () => {
  it("is true without -i", () => {
    expect(isInfoRun(["--version"])).toBe(true)
    expect(isInfoRun([])).toBe(true)
    expect(isInfoRun(["-i", "a.mkv", "-b", "448"])).toBe(false)
  })
})
