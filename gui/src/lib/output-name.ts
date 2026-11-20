// Output file names, for the preview of -on / -ot. This mirrors
// src/output_name.rs of the CLI rule by rule; both are tested with the
// vectors of tests/fixtures/output_names.json. No Tauri / DOM dependencies.

import type { DdpAtmosMode, DeewOptions, Format } from "@/lib/deew-args"

/** The placeholders of a template, in the order the CLI lists them. */
export const PLACEHOLDERS = [
  "stem",
  "name",
  "format",
  "codec",
  "channels",
  "bitrate",
  "lang",
  "title",
  "track",
] as const
export type Placeholder = (typeof PLACEHOLDERS)[number]

/** The template of the config when it has none. */
export const DEFAULT_TEMPLATE = "{stem}"

/** The extensions of deew's outputs. */
const OUTPUT_EXTENSIONS = ["ac3", "ec3", "eb3", "thd", "ac4", "aac"]

/** The config's template from the settings answer; anything but a non-empty string gives `{stem}`. */
export function outputTemplateOf(settings: {
  outputTemplate?: unknown
}): string {
  const t = settings.outputTemplate
  return typeof t === "string" && t !== "" ? t : DEFAULT_TEMPLATE
}

/** What separates the parts of a name. */
const SEPARATORS = [".", " ", "_", "-"]

/** The values of the placeholders for one output; empty means unknown. */
export type NameValues = Record<Placeholder, string>

type Part = { text: string } | { field: Placeholder }

/** A parsed output name: a template, or a literal name (`-on`). */
export interface Template {
  parts: Part[]
}

export type Parsed = { template: Template } | { error: string }

function validList(): string {
  return PLACEHOLDERS.map((p) => `{${p}}`).join(", ")
}

/** A name as it is, braces are ordinary characters. */
export function literalTemplate(text: string): Template {
  return { parts: [{ text }] }
}

/** Parses a template; the error is the CLI's message. */
export function parseTemplate(text: string): Parsed {
  if (text.includes("/") || text.includes("\\")) {
    return {
      error:
        "the template can't contain a path separator, use -o for the folder",
    }
  }
  const unbalanced = {
    error: `unbalanced brace in the template, the valid placeholders are ${validList()}`,
  }
  const parts: Part[] = []
  let literal = ""
  const chars = [...text]
  for (let i = 0; i < chars.length; i++) {
    const c = chars[i]
    if (c === "{") {
      let field = ""
      let closed = false
      for (i++; i < chars.length; i++) {
        if (chars[i] === "}") {
          closed = true
          break
        }
        if (chars[i] === "{") return unbalanced
        field += chars[i]
      }
      if (!closed) return unbalanced
      const known = PLACEHOLDERS.find((p) => p === field)
      if (known === undefined) {
        return {
          error: `unknown placeholder {${field}}, the valid placeholders are ${validList()}`,
        }
      }
      if (literal !== "") {
        parts.push({ text: literal })
        literal = ""
      }
      parts.push({ field: known })
    } else if (c === "}") {
      return unbalanced
    } else {
      literal += c
    }
  }
  if (literal !== "") parts.push({ text: literal })
  return { template: { parts } }
}

/**
 * Characters that are illegal on Windows (`<>:"/\|?*`, control characters)
 * become `_`, on every platform.
 */
export function sanitize(text: string): string {
  let out = ""
  for (const c of text) {
    const code = c.codePointAt(0) ?? 0
    const control = code < 0x20 || (code >= 0x7f && code < 0xa0)
    out += control || '<>:"/\\|?*'.includes(c) ? "_" : c
  }
  return out
}

function trimStart(text: string, chars: readonly string[]): string {
  let i = 0
  while (i < text.length && chars.includes(text[i])) i++
  return text.slice(i)
}

function trimEnd(text: string, chars: readonly string[]): string {
  let i = text.length
  while (i > 0 && chars.includes(text[i - 1])) i--
  return text.slice(0, i)
}

/** The output extension the template's text ends with (lowercase), if any. */
function tailExtension(template: Template): string | null {
  const last = template.parts.at(-1)
  if (last === undefined || !("text" in last)) return null
  const dot = last.text.lastIndexOf(".")
  if (dot < 0) return null
  const ext = last.text.slice(dot + 1).toLowerCase()
  return OUTPUT_EXTENSIONS.includes(ext) ? ext : null
}

type Item =
  | { kind: "text"; text: string }
  | { kind: "value"; text: string }
  | { kind: "empty" }

/** The name without the extension handling. */
function renderBase(template: Template, values: NameValues): string {
  const items: Item[] = template.parts.map((part): Item => {
    if ("text" in part) return { kind: "text", text: sanitize(part.text) }
    const value = sanitize(values[part.field])
    return value === "" ? { kind: "empty" } : { kind: "value", text: value }
  })
  const anyEmpty = items.some((i) => i.kind === "empty")
  // An empty value drops out with one adjacent separator: the separators on
  // both sides of it become the one on its left (the one on its right if
  // there is none).
  const done: Item[] = []
  for (let index = 0; index < items.length; index++) {
    const item = items[index]
    if (item.kind === "empty") {
      let left = ""
      const previous = done.at(-1)
      if (previous?.kind === "text") {
        const kept = trimEnd(previous.text, SEPARATORS)
        left = previous.text.slice(kept.length)
        previous.text = kept
      }
      let right = ""
      const next = items[index + 1]
      if (next?.kind === "text") {
        const rest = trimStart(next.text, SEPARATORS)
        right = next.text.slice(0, next.text.length - rest.length)
        next.text = rest
      }
      done.push({ kind: "text", text: left === "" ? right : left })
    } else if (item.kind === "text" && item.text === "") {
      // consumed by an empty value
    } else {
      done.push(item)
    }
  }
  if (anyEmpty) {
    // separators of the template at the ends of the name
    for (const item of done) {
      if (item.kind !== "text") break
      item.text = trimStart(item.text, SEPARATORS)
      if (item.text !== "") break
    }
    for (let i = done.length - 1; i >= 0; i--) {
      const item = done[i]
      if (item.kind !== "text") break
      item.text = trimEnd(item.text, SEPARATORS)
      if (item.text !== "") break
    }
  }
  const name = done
    .map((item) => (item.kind === "empty" ? "" : item.text))
    .join("")
  return trimEnd(name, [".", " "])
}

export type Rendered = { name: string } | { error: string }

/** The file name for `values` with the extension `ext` (without the dot). */
export function renderName(
  template: Template,
  values: NameValues,
  ext: string
): Rendered {
  // the default name stays what it always was, whatever characters the
  // input's name has
  const only = template.parts[0]
  if (
    template.parts.length === 1 &&
    "field" in only &&
    only.field === "stem" &&
    values.stem !== ""
  ) {
    return { name: `${values.stem}.${ext}` }
  }
  const name = renderBase(template, values)
  if (name === "") return { error: "the output name is empty" }
  const tail = tailExtension(template)
  if (tail === null) return { name: `${name}.${ext}` }
  if (tail === ext) return { name }
  return {
    error: `the name ends with .${tail}, but the extension of this output is .${ext}, leave it out or end the name with .${ext}`,
  }
}

/** The extension of the output (`Format::extension` of the CLI). */
export function outputExtension(
  format: Format,
  opts: { ddp71Bluray: boolean; atmosMode: DdpAtmosMode }
): string {
  switch (format) {
    case "dd":
      return "ac3"
    case "ddp":
      return opts.ddp71Bluray ? "eb3" : "ec3"
    case "ddp-atmos":
      return opts.atmosMode === "bluray" ? "eb3" : "ec3"
    case "ac4":
    case "ac4-atmos":
    case "ac4-ims":
      return "ac4"
    case "aac":
      return "aac"
    case "thd":
    case "thd-atmos":
      return "thd"
  }
}

/** The label of the output format as the CLI's summary shows it. */
export function codecLabel(format: Format): string {
  const labels: Record<Format, string> = {
    dd: "DD",
    ddp: "DDP",
    "ddp-atmos": "DDP Atmos",
    thd: "TrueHD",
    "thd-atmos": "TrueHD Atmos",
    ac4: "AC4",
    "ac4-atmos": "AC4 Atmos",
    "ac4-ims": "AC4 IMS",
    aac: "AAC",
  }
  return labels[format]
}

/**
 * The channels of the output (`resolve_layout` and `Layout::name_label` of
 * the CLI): `2.0`, `5.1`, `7.1`, `5.0`, `7.0`; empty when unknown.
 */
export function channelsLabel(
  format: Format,
  atmosMode: DdpAtmosMode,
  channels: number | null,
  downmix: string,
  disableLfe: boolean
): string {
  switch (format) {
    case "ac4-ims":
      return "2.0"
    case "ddp-atmos":
      return atmosMode === "streaming" ? "5.1" : "7.1"
    case "thd-atmos":
      return "7.1"
    case "ac4-atmos":
      return ""
    default:
      break
  }
  if (channels === null || ![1, 2, 5, 6, 7, 8].includes(channels)) return ""
  const slots = channels === 5 ? 6 : channels === 7 ? 8 : channels
  const dm = downmix === "none" ? null : Number(downmix)
  const effective =
    dm ?? ((format === "dd" || format === "ac4") && slots === 8 ? 6 : null)
  const out = format === "aac" ? 2 : (effective ?? slots)
  const hasLfe = channels === 6 || channels === 8
  const noLfe = channels === 5 || channels === 7
  const removed =
    out >= 6 &&
    (noLfe || (disableLfe && hasLfe)) &&
    (format === "dd" || format === "ddp")
  if (removed) return out === 6 ? "5.0" : "7.0"
  const names: Record<number, string> = {
    1: "1.0",
    2: "2.0",
    6: "5.1",
    8: "7.1",
  }
  return names[out] ?? ""
}

/** `Movie` of `/x/Movie.mkv` (the name after the last separator, without its extension). */
function splitName(path: string): { stem: string; name: string } {
  const base = path.slice(
    Math.max(path.lastIndexOf("/"), path.lastIndexOf("\\")) + 1
  )
  const dot = base.lastIndexOf(".")
  // leading dots of the name do not start an extension
  const name =
    dot > 0 && [...base.slice(0, dot)].some((c) => c !== ".")
      ? base.slice(0, dot)
      : base
  const stem = name.replace(/ ?DELAY [-|+]?[0-9]+m?s/g, "")
  return { stem, name }
}

/** What the preview needs of one probed input. */
export interface NameInput {
  path: string
  channels: number | null
  language: string | null
  title: string | null
  master: boolean
}

/** What the preview needs of the options. */
export interface NameContext {
  options: Pick<
    DeewOptions,
    | "format"
    | "downmix"
    | "disableLfe"
    | "trackIndex"
    | "outputName"
    | "outputTemplate"
  >
  /** The config's `[output] template`. */
  configTemplate: string
  atmosMode: DdpAtmosMode
  /** The output is 7.1 DDP in the Blu-ray profile. */
  ddp71Bluray: boolean
  /** The bitrate of the encode in kbps, `null` when not known. */
  bitrate: number | null
  /** The normalized `-ti`, shown as given. */
  track: string
}

/** The template text in effect (an empty field follows the config). */
export function effectiveTemplateText(
  outputTemplate: string | null,
  configTemplate: string
): string {
  return outputTemplate === null || outputTemplate.trim() === ""
    ? configTemplate
    : outputTemplate
}

export interface NamePreview {
  path: string
  /** The output file name, or the reason there is none. */
  result: Rendered
}

/** The output names of the inputs, like the CLI makes them. */
export function previewNames(
  files: readonly NameInput[],
  ctx: NameContext,
  single: boolean
): NamePreview[] {
  const o = ctx.options
  const ext = outputExtension(o.format, {
    ddp71Bluray: ctx.ddp71Bluray,
    atmosMode: ctx.atmosMode,
  })
  const name = o.outputName.trim()
  let parsed: Parsed
  if (single && name !== "") {
    parsed =
      name.includes("/") || name.includes("\\")
        ? {
            error:
              "the name can't contain a path separator, use -o for the folder",
          }
        : { template: literalTemplate(name) }
  } else {
    parsed = parseTemplate(
      effectiveTemplateText(o.outputTemplate, ctx.configTemplate)
    )
  }
  const thd = o.format === "thd" || o.format === "thd-atmos"
  return files.map((file) => {
    if ("error" in parsed) return { path: file.path, result: parsed }
    const { stem, name: full } = splitName(file.path)
    const values: NameValues = {
      stem,
      name: full,
      format: o.format,
      codec: codecLabel(o.format),
      channels: channelsLabel(
        o.format,
        ctx.atmosMode,
        file.channels,
        o.downmix,
        o.disableLfe
      ),
      bitrate: thd || ctx.bitrate === null ? "" : String(ctx.bitrate),
      lang: file.master ? "" : (file.language ?? ""),
      title: file.master ? "" : (file.title ?? ""),
      track: ctx.track,
    }
    return { path: file.path, result: renderName(parsed.template, values, ext) }
  })
}
