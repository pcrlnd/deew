// Builds the root `deew` crate in release mode and copies the binary to
// src-tauri/binaries/deew-<target-triple>[.exe], where Tauri expects the
// `bundle.externalBin` sidecar ("binaries/deew").
//
// The target is TAURI_ENV_TARGET_TRIPLE when set (the Tauri CLI sets it for
// beforeBuildCommand / beforeDevCommand), else the host triple of rustc.

import { execFileSync, spawnSync } from "node:child_process"
import { copyFileSync, mkdirSync } from "node:fs"
import { dirname, join, resolve } from "node:path"
import { fileURLToPath } from "node:url"

const guiDir = resolve(dirname(fileURLToPath(import.meta.url)), "..")
const manifest = resolve(guiDir, "..", "Cargo.toml")
const binariesDir = join(guiDir, "src-tauri", "binaries")

function hostTriple() {
  const out = execFileSync("rustc", ["-vV"], { encoding: "utf8" })
  const match = /^host:\s*(\S+)\s*$/m.exec(out)
  if (!match)
    throw new Error("could not determine the host triple from `rustc -vV`")
  return match[1]
}

function cargoBuild(triple) {
  const args = ["build", "--release", "--manifest-path", manifest]
  if (triple) args.push("--target", triple)
  console.log(`> cargo ${args.join(" ")}`)
  const result = spawnSync("cargo", args, { stdio: "inherit" })
  if (result.error) throw result.error
  if (result.status !== 0) process.exit(result.status ?? 1)
}

function builtBinary(triple) {
  const exe = (triple ?? hostTriple()).includes("windows") ? "deew.exe" : "deew"
  // cargo honours CARGO_TARGET_DIR (relative to the working directory).
  const base = process.env.CARGO_TARGET_DIR
    ? resolve(process.cwd(), process.env.CARGO_TARGET_DIR)
    : join(dirname(manifest), "target")
  return join(base, ...(triple ? [triple] : []), "release", exe)
}

const requested = process.env.TAURI_ENV_TARGET_TRIPLE || undefined
const target = requested ?? hostTriple()
const ext = target.includes("windows") ? ".exe" : ""
const dest = join(binariesDir, `deew-${target}${ext}`)
mkdirSync(binariesDir, { recursive: true })

if (target === "universal-apple-darwin") {
  const parts = ["aarch64-apple-darwin", "x86_64-apple-darwin"].map((t) => {
    cargoBuild(t)
    return builtBinary(t)
  })
  execFileSync("lipo", ["-create", "-output", dest, ...parts], {
    stdio: "inherit",
  })
} else {
  cargoBuild(requested)
  copyFileSync(builtBinary(requested), dest)
}
console.log(`sidecar: ${dest}`)
