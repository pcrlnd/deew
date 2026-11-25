<p align="center"><a href="https://github.com/pcrlnd/deew/blob/main/gui/README.md">English readme</a>
 • <a href="https://github.com/pcrlnd/deew/blob/main/gui/README_hu.md">Magyar leírás</a></p>

# deew GUI

Desktop front end for [deew](../README.md), built with Tauri v2, React, shadcn/ui
and xterm.js (scaffolded with [tauri-ui](https://github.com/agmmnn/tauri-ui)).

## Requirements

Node.js 22+, a Rust toolchain and the [Tauri system dependencies](https://tauri.app/start/prerequisites/)
(on Linux: WebKitGTK 4.1, libsoup 3, GTK 3, ...).

## Commands

```bash
npm install
npm run tauri dev                  # dev app with hot reload
npm run tauri build                # release build and installers
npm run tauri build -- --bundles deb

npm run sidecar                    # build the deew sidecar only
npm run typecheck && npm run lint && npm test
cargo test --workspace            # needs `npm run sidecar` first
```

`tauri dev` / `tauri build` build the sidecar automatically
(`scripts/build-sidecar.mjs`: `cargo build --release` of the root crate, copied to
`src-tauri/binaries/deew-<target-triple>`). Run `npm run sidecar` once before
using plain `cargo check` / `cargo clippy` / `cargo test` in `gui`, because
the Tauri build script requires the sidecar file to exist.

## Web mode (browser)

The same frontend also runs in a browser against `deew-web` (`web/`), a server
binary that serves the built frontend and talks to it over HTTP + WebSocket
(`/api/*`). It runs deew on the machine where the server runs; there are no
native dialogs, so files and folders are picked in a server-side file browser,
and there is no drag & drop. Without a Tauri runtime the frontend selects the
web transport automatically (`src/lib/runner.ts`).

Development (two terminals, then open <http://localhost:3000>; Vite proxies
`/api`, including the WebSocket, to `127.0.0.1:8765`):

```bash
cargo run -p deew-web              # API server on 127.0.0.1:8765
npm run dev                        # frontend with hot reload
```

Production: a single binary with the frontend embedded. Build the frontend
first, since it is embedded at compile time:

```bash
npm run build
cargo build -p deew-web --release
```

Put the `deew` CLI next to the `deew-web` binary (and its config, as for the
CLI). Options: `--host <addr>` and `--port <n>` (default `127.0.0.1:8765`).

Login is optional. To require it, add to the CLI's config:

```toml
[deew_web]
    user = 'me'
    pass = '$argon2id$...'
```

and create the hash with `deew-web --hash-password` (asks for the password
twice, prints the line to paste). User and pass must be set together; with
both empty there is no login.

Security notes:

- It binds to localhost by default, and then only accepts `localhost`,
  `127.0.0.1` and `[::1]` as Host (DNS rebinding protection). Anyone who can
  reach the server without a login can run deew and read the file system as
  the user running it.
- The server speaks plain HTTP: with a login the password is sent unencrypted
  when bound to a non-loopback address. To expose it to the internet, put a TLS
  reverse proxy (nginx, Caddy, ...) in front, forwarding WebSocket upgrades and
  the original `Host` header.
- The pages are served with a strict CSP (no inline scripts), which is why the
  theme snippet lives in `public/theme-init.js`.

## Output: CLI or GUI

The toggle after the Stop button picks what the window shows of a run:

- **CLI** (default): the terminal output of the CLI.
- **GUI**: native progress bars per file, warnings and errors as lines, and the
  CLI's y/n questions as pop-up dialogs.
- **Utilities** (wrench icon, no label): the output of the Utilities menu's
  commands, see below. It is not saved in the config; CLI and GUI keep what they
  showed.

The mode of a run is fixed when it starts; the toggle only switches what is
shown, also while a job runs (a run started with the CLI output shows a note in
the GUI view). The choice is saved in the CLI's config (same in `deew-web`), in its own
section, and can also be edited by hand:

```toml
[gui]
    progress = 'cli' # or 'native'
    output = '' # output directory (-o), empty: home folder
    scaling = '100%' # window zoom of deew-gui, '80%' to '150%'
    use_system_font = false # true: the system's font instead of IBM Plex Sans
```

`scaling` zooms the `deew-gui` window (80-150%, default 100%); it is applied at
startup, so restart the app after changing it. `deew-web` ignores it, use the
browser's own zoom (Ctrl +/-). `use_system_font = true` makes the interface text
use the operating system's font instead of the bundled IBM Plex Sans (both
`deew-gui` and `deew-web`; the terminal keeps its monospace font). The GUI never
changes these two keys, edit them by hand.

The Output directory field is saved there too (`output`) and loaded on startup.

The toggle only edits an existing `config.toml` (run deew once to create it);
otherwise it shows "Setting not saved". The GUI mode reads progress events that
the CLI writes when `DEEW_EVENTS_FILE` is set, so it needs deew 5.0.0 or newer (`deew-gui`/`deew-web` 0.5.0 and newer).
Both `deew-gui` and `deew-web` run the `deew` binary next to them; with an older
one the GUI mode shows a note that the CLI does not report progress, use the
CLI output then.

## Utilities

The Utilities menu (title bar of `deew-gui`, toolbar of `deew-web`) runs the
CLI's one-off commands: Help, Full help, Version, Changelog, List bitrates, Print
logos, Show config, Generate config and Download tools. Choosing one switches to
the Utilities tab (third tab after CLI and GUI) and runs the command in its own
terminal; the next command clears the previous output, and Stop ends a
running one.

- They run in a separate pseudo-terminal, so they work while an encode is
  running and do not touch its terminal, progress, history or Stop button. One
  utility runs at a time.
- Download tools (`-gt`) replaces ffmpeg, truehdd and dee_shim, and Generate
  config (`-gc`) rewrites `config.toml`, so both are greyed out while an encode
  runs, and an encode cannot start while one of them runs. The backends enforce
  this too.
- Generate config always asks first: it overwrites `config.toml` with the
  defaults, including the `[gui]` settings, the default bitrates and the
  `deew-web` login.
- Only these fixed flags are accepted by the backends (`-h`, `-fh`, `-v`, `-cl`,
  `-lb`, `-pl`, `-c`, `-gc`, `-gt`), never other arguments. `deew-web` serves them
  on the WebSocket `/api/info` (same login, Host and Origin checks as `/api/run`);
  the output goes only to the tab that asked, and closing the tab ends the command.

## Formats

The format dropdown offers DD, DDP, DDP Atmos, AC4, AC4 Atmos, AC-4 IMS, AAC and
TrueHD. The options form follows the CLI rules: `-dm` is only available for
DD/DDP/AC4/AAC, `-dn` for DD/DDP/AC4, and the Atmos formats (TrueHD Atmos input,
decoded with truehdd) have neither `-dm` nor `-mo`. In GUI output the Atmos
formats show an extra decode stage for truehdd. The tools themselves (DEE 6.7,
truehdd, dee_shim) go next to the CLI or in your PATH, see the [main readme](../README.md).

The "Surround phase shift" (`-sps`), "LFE low-pass filter" (`-lpf`) and
"Surround 3 dB attenuation" (`-s3a`) checkboxes show the config's values
(`surround_phase_shift`, `low_pass_filter`, `surround_3db_attenuation`) and a
flag is only passed when you change one for the run. Phase shift is for
DD/DDP/DDP Atmos, `-lpf` for DD/DDP only and `-s3a` for DD/DDP/TrueHD (the CLI
asks first for TrueHD, because the output won't be identical to the source, and
the question appears in the prompt pop-up / terminal); elsewhere they are greyed out.

The "Warp mode" select (`-wm`) and the "Bed conform" checkbox (`-bc`) show the
config's values (`truehdd_warp_mode`, `truehdd_bed_conform`) in the same way; they
are for DDP Atmos, TrueHD Atmos, AC-4 Atmos, AC-4 IMS and Measure only, elsewhere
they are greyed out.

The "Output name" field (`-on`) names the output of a single input file (greyed
out with more files, after folders are expanded) and the "Output template" field
(`-ot`) the outputs of all of them; it starts with the config's `[output]
template` and is only passed when you change it. The name wins over the template,
both are greyed out with Measure only. Under the fields the output names of the
first files are shown as deew will make them, the same rules as the CLI.

**AC-4 IMS** (`ac4-ims`, immersive stereo, `.ac4`) takes 5.1 / 5.0 / 7.0 / 7.1 or
Atmos inputs (TrueHD Atmos, ADM BWF, DAMF), but not Atmos and channel-based inputs
in one run (the GUI warns about stereo / mono inputs and about mixed inputs). Its
bitrates are 64, 96, 128, 144, 256 and 320 kbps (default: `ac4_ims` of the
config). `-dm`, `-dl` and `-dn` are greyed out for it, DRC is `none` by default,
and the "AC-4 IMS mode" select (`--ac4-ims-mode`, `general` or `music`) is only
passed for `music`, which selects the DRC `music_light` (the other DRC values are
greyed out while it is on).

**Atmos masters**: ADM BWF (`.wav` with `chna` and `axml` chunks) and DAMF
(`.atmos` with its `.atmos.audio` and `.atmos.metadata`) are found by the same
header check as the CLI does (the GUI asks the CLI to probe the inputs), without
ffprobe, and shown as "ADM BWF Atmos master" / "DAMF Atmos master". They can be
encoded to DDP Atmos, TrueHD Atmos, AC4 Atmos and AC-4 IMS, and to DD/DDP (a
render, not together with other inputs); the GUI warns for TrueHD, AC4 and AAC.
`-ti` is ignored for them. Every `.wav` and `.atmos` can be picked in the file
dialog and the web file browser, there is no extension filter.

**Track index** (`-ti`): `N` or `a:N` is the Nth audio track, `s:N` the stream
with index N of the container (it has to be an audio stream, otherwise the input
shows an error). The GUI probes the same stream that the CLI will use.

On macOS use `deew-gui-macos.zip` (unzip it to get `deew-gui.app`) and the
`deew-web-macos` release binary (`chmod +x` it, like on Linux). The app does not
contain the CLI: either keep `deew-macos` next to `deew-gui.app` (with
`config.toml`, `dee/` and the tools), or put the app in Applications, where the
deew folder is `~/Library/Application Support/deew/` and Utilities → Update and install all
downloads the CLI there.

## Layout

- `src/lib/deew-args.ts`: option state to argv and display quoting (unit tested)
- `src/lib/runner.ts`: Tauri / web transport; `web-api.ts`, `run-client.ts`: HTTP and WebSocket client of the web mode
- `src/components/`: title bar, utilities menu, options form, command bar, terminal pane, progress pane
- `core/src/pty_session.rs`: Tauri-independent PTY session (unit tested)
- `core/`: Tauri-independent backend (CLI discovery, probing, PTY session)
- `src-tauri/src/lib.rs`: Tauri commands `run_deew`, `write_stdin`, `resize`, `stop`
