import { readFileSync } from "node:fs"
import { resolve } from "node:path"
import { describe, expect, it } from "vitest"

import { DEFAULT_OPTIONS } from "./deew-args"
import {
  channelsLabel,
  codecLabel,
  literalTemplate,
  outputExtension,
  parseTemplate,
  previewNames,
  renderName,
  type NameContext,
  type NameInput,
  type NameValues,
  type Template,
} from "./output-name"

interface Vector {
  text: string
  literal?: boolean
  ext?: string
  values: NameValues
  expected?: string
  error?: string
}

// the vectors of the CLI's Rust test (src/output_name.rs)
const vectors: Vector[] = JSON.parse(
  readFileSync(
    resolve(import.meta.dirname, "../../../tests/fixtures/output_names.json"),
    "utf8"
  )
)

describe("shared output name vectors", () => {
  it("has the vectors", () => {
    expect(vectors.length).toBeGreaterThan(20)
  })

  it.each(vectors.map((v) => [v.text, v] as const))("%s", (_text, v) => {
    let template: Template
    if (v.literal === true) {
      template = literalTemplate(v.text)
    } else {
      const parsed = parseTemplate(v.text)
      if ("error" in parsed) {
        expect(v.error).toBeDefined()
        expect(parsed.error).toContain(v.error)
        return
      }
      template = parsed.template
    }
    const rendered = renderName(template, v.values, v.ext ?? "ec3")
    if (v.expected !== undefined) {
      expect(rendered).toEqual({ name: v.expected })
    } else {
      expect(rendered).toHaveProperty("error")
      expect("error" in rendered && rendered.error).toContain(v.error)
    }
  })
})

describe("labels", () => {
  it("uses the CLI's codec labels", () => {
    expect(codecLabel("ac4-ims")).toBe("AC4 IMS")
    expect(codecLabel("thd-atmos")).toBe("TrueHD Atmos")
    expect(codecLabel("ddp")).toBe("DDP")
  })

  it("names the extension", () => {
    const o = { ddp71Bluray: false, atmosMode: "streaming" } as const
    expect(outputExtension("dd", o)).toBe("ac3")
    expect(outputExtension("ddp", o)).toBe("ec3")
    expect(outputExtension("ddp", { ...o, ddp71Bluray: true })).toBe("eb3")
    expect(outputExtension("ddp-atmos", { ...o, atmosMode: "bluray" })).toBe(
      "eb3"
    )
    expect(outputExtension("ac4-ims", o)).toBe("ac4")
    expect(outputExtension("thd-atmos", o)).toBe("thd")
    expect(outputExtension("aac", o)).toBe("aac")
  })

  it("names the channels of the output", () => {
    const f = channelsLabel
    expect(f("ddp", "streaming", 6, "none", false)).toBe("5.1")
    expect(f("ddp", "streaming", 8, "none", false)).toBe("7.1")
    expect(f("ddp", "streaming", 8, "2", false)).toBe("2.0")
    expect(f("ddp", "streaming", 1, "none", false)).toBe("1.0")
    expect(f("dd", "streaming", 8, "none", false)).toBe("5.1")
    expect(f("ddp", "streaming", 6, "none", true)).toBe("5.0")
    expect(f("ddp", "streaming", 5, "none", false)).toBe("5.0")
    expect(f("ddp", "streaming", 7, "none", false)).toBe("7.0")
    // a format that can't leave the LFE out keeps it silent
    expect(f("ac4", "streaming", 5, "none", false)).toBe("5.1")
    expect(f("aac", "streaming", 6, "none", false)).toBe("2.0")
    expect(f("ddp-atmos", "streaming", null, "none", false)).toBe("5.1")
    expect(f("ddp-atmos", "standard", null, "none", false)).toBe("7.1")
    expect(f("ddp-atmos", "bluray", null, "none", false)).toBe("7.1")
    expect(f("thd-atmos", "streaming", null, "none", false)).toBe("7.1")
    expect(f("ac4-atmos", "streaming", null, "none", false)).toBe("")
    expect(f("ac4-ims", "streaming", 6, "none", false)).toBe("2.0")
    expect(f("ddp", "streaming", null, "none", false)).toBe("")
  })
})

describe("previewNames", () => {
  const file = (patch: Partial<NameInput> = {}): NameInput => ({
    path: "/in/Movie DELAY -5ms.mkv",
    channels: 6,
    language: "eng",
    title: null,
    master: false,
    ...patch,
  })
  const ctx = (
    options: Partial<NameContext["options"]> = {},
    patch: Partial<NameContext> = {}
  ): NameContext => ({
    options: { ...DEFAULT_OPTIONS, ...options },
    configTemplate: "{stem}",
    atmosMode: "streaming",
    ddp71Bluray: false,
    bitrate: 1024,
    track: "0",
    ...patch,
  })
  const names = (
    files: NameInput[],
    c: NameContext,
    single = files.length === 1
  ) =>
    previewNames(files, c, single).map((p) =>
      "name" in p.result ? p.result.name : `error: ${p.result.error}`
    )

  it("is the stem and the extension by default", () => {
    expect(names([file()], ctx())).toEqual(["Movie.ec3"])
    expect(names([file()], ctx({ format: "dd" }))).toEqual(["Movie.ac3"])
  })

  it("follows the config's template, an empty field too", () => {
    const config = ctx({ outputTemplate: "" }, { configTemplate: "{name}" })
    expect(names([file()], config)).toEqual(["Movie DELAY -5ms.ec3"])
    expect(
      names(
        [file()],
        ctx({ outputTemplate: "{stem}" }, { configTemplate: "{name}" })
      )
    ).toEqual(["Movie.ec3"])
  })

  it("fills every placeholder", () => {
    const c = ctx({
      outputTemplate:
        "{stem} {format} {codec} {channels} {bitrate} {lang} {track}",
    })
    expect(names([file()], c)).toEqual(["Movie ddp DDP 5.1 1024 eng 0.ec3"])
    // missing values drop out
    const none = file({ language: null })
    expect(
      names([none], ctx({ outputTemplate: "{stem}.{lang}.{title}.ddp" }))
    ).toEqual(["Movie.ddp.ec3"])
  })

  it("has no bitrate for TrueHD and no tags for masters", () => {
    const c = ctx({ format: "thd", outputTemplate: "{stem} {bitrate}" })
    expect(names([file()], c)).toEqual(["Movie.thd"])
    const master = file({ master: true, title: "x" })
    expect(
      names([master], ctx({ outputTemplate: "{stem} {lang} {title}" }))
    ).toEqual(["Movie.ec3"])
  })

  it("uses the name for a single file and the template otherwise", () => {
    const c = ctx({ outputName: "Name", outputTemplate: "{name}" })
    expect(names([file()], c)).toEqual(["Name.ec3"])
    expect(names([file(), file({ path: "/in/b.mkv" })], c)).toEqual([
      "Movie DELAY -5ms.ec3",
      "b.ec3",
    ])
  })

  it("rejects a folder in the name", () => {
    for (const outputName of ["Out/Name", "Out\\Name"]) {
      expect(names([file()], ctx({ outputName }))[0]).toContain(
        "the name can't contain a path separator, use -o for the folder"
      )
    }
  })

  it("reports errors per file", () => {
    expect(names([file()], ctx({ outputTemplate: "{nope}" }))[0]).toContain(
      "error: unknown placeholder"
    )
    expect(names([file()], ctx({ outputName: "x.ac3" }))[0]).toContain(
      "extension of this output is .ec3"
    )
    expect(
      names([file()], ctx({ outputName: "x.ec3" }, { ddp71Bluray: true }))[0]
    ).toContain("is .eb3")
  })
})
