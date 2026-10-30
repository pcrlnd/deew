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
  CLI's y/n questions as pop-up dialogs. The terminal stays available in a
  collapsible "CLI output" section (it opens by itself for the Utilities menu
  commands, which have no progress).

The mode of a run is fixed when it starts, so the toggle is disabled while a job
runs. The choice is saved in the CLI's config (same in `deew-web`), in its own
section, and can also be edited by hand:

```toml
[gui]
    progress = 'cli' # or 'native'
```

The toggle only edits an existing `config.toml` (run deew once to create it);
otherwise it shows "Setting not saved". The GUI mode reads progress events that
the CLI writes when `DEEW_EVENTS_FILE` is set, so it needs deew 3.6.2 or newer.
Both `deew-gui` and `deew-web` run the `deew` binary next to them; with an older
one the GUI mode shows a note that the CLI does not report progress, switch to
CLI output then.

## Layout

- `src/lib/deew-args.ts`: option state to argv and display quoting (unit tested)
- `src/lib/runner.ts`: Tauri / web transport; `web-api.ts`, `run-client.ts`: HTTP and WebSocket client of the web mode
- `src/components/`: title bar, utilities menu, options form, command bar, terminal pane, progress pane
- `core/src/pty_session.rs`: Tauri-independent PTY session (unit tested)
- `core/`: Tauri-independent backend (CLI discovery, probing, PTY session)
- `src-tauri/src/lib.rs`: Tauri commands `run_deew`, `write_stdin`, `resize`, `stop`
