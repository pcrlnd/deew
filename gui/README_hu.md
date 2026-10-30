<p align="center"><a href="https://github.com/pcrlnd/deew/blob/main/gui/README.md">English readme</a>
 • <a href="https://github.com/pcrlnd/deew/blob/main/gui/README_hu.md">Magyar leírás</a></p>

# deew GUI

Asztali felület a [deew](../README_hu.md)-hoz, Tauri v2, React, shadcn/ui és
xterm.js felhasználásával (a [tauri-ui](https://github.com/agmmnn/tauri-ui) alapjaira építve).

## Követelmények

Node.js 22+, Rust toolchain és a [Tauri rendszerfüggőségei](https://tauri.app/start/prerequisites/)
(Linuxon: WebKitGTK 4.1, libsoup 3, GTK 3, ...).

## Parancsok

```bash
npm install
npm run tauri dev                  # fejlesztői app hot reloaddal
npm run tauri build                # release build és telepítők
npm run tauri build -- --bundles deb

npm run sidecar                    # csak a deew sidecar buildelése
npm run typecheck && npm run lint && npm test
cargo test --workspace             # előtte kell egy `npm run sidecar`
```

A `tauri dev` / `tauri build` automatikusan buildeli a sidecart
(`scripts/build-sidecar.mjs`: a gyökér crate `cargo build --release`-e, ami a
`src-tauri/binaries/deew-<target-triple>` helyre másolódik). Mielőtt a `gui` mappában
sima `cargo check` / `cargo clippy` / `cargo test` parancsokat használnál, futtasd egyszer
az `npm run sidecar`-t, mert a Tauri build script megköveteli, hogy a sidecar fájl létezzen.

## Webes mód (böngésző)

Ugyanez a frontend böngészőben is fut a `deew-web` (`web/`) szerverrel: ez egy szerver
bináris, ami kiszolgálja a buildelt frontendet, és HTTP + WebSocket kapcsolaton
(`/api/*`) kommunikál vele. A deew-t azon a gépen futtatja, ahol a szerver fut; natív
dialógusok nincsenek, ezért a fájlokat és mappákat egy szerveroldali fájlböngészőben
lehet kiválasztani, és drag & drop sincs. Tauri runtime nélkül a frontend automatikusan
a webes transportot választja (`src/lib/runner.ts`).

Fejlesztés (két terminál, utána nyisd meg a <http://localhost:3000> címet; a Vite a
`/api`-t, a WebSocketet is beleértve, a `127.0.0.1:8765`-re proxyzza):

```bash
cargo run -p deew-web              # API szerver a 127.0.0.1:8765 címen
npm run dev                        # frontend hot reloaddal
```

Éles használatra: egyetlen bináris, beágyazott frontenddel. Előbb a frontendet buildeld,
mert fordításkor kerül beágyazásra:

```bash
npm run build
cargo build -p deew-web --release
```

Tedd a `deew` CLI-t a `deew-web` bináris mellé (a configjával együtt, ahogy a CLI-nál).
Opciók: `--host <addr>` és `--port <n>` (alapértelmezés: `127.0.0.1:8765`).

A bejelentkezés opcionális. Ha kötelezővé akarod tenni, add hozzá a CLI configjához:

```toml
[deew_web]
    user = 'me'
    pass = '$argon2id$...'
```

a hasht pedig a `deew-web --hash-password` paranccsal készítsd el (kétszer kéri a
jelszót, és kiírja a bemásolandó sort). A usert és a passt együtt kell beállítani; ha
mindkettő üres, nincs bejelentkezés.

Biztonsági megjegyzések:

- Alapértelmezés szerint a localhostra bindol, és ilyenkor csak a `localhost`,
  `127.0.0.1` és `[::1]` Host fejlécet fogadja el (DNS rebinding elleni védelem). Bárki,
  aki bejelentkezés nélkül eléri a szervert, futtathatja a deew-t és olvashatja a
  fájlrendszert annak a felhasználónak a jogaival, aki a szervert futtatja.
- A szerver sima HTTP-t használ: ha nem loopback címre bindolod, bejelentkezéskor a
  jelszó titkosítatlanul megy át. Ha az internetre akarod kitenni, tegyél elé egy TLS
  reverse proxyt (nginx, Caddy, ...), ami továbbítja a WebSocket upgrade-eket és az
  eredeti `Host` fejlécet.
- Az oldalak szigorú CSP-vel mennek ki (nincsenek inline scriptek), ezért van a téma
  snippet a `public/theme-init.js` fájlban.

## Kimenet: CLI vagy GUI

A Stop gomb utáni kapcsoló választja ki, mit mutat az ablak a futásból:

- **CLI** (alapértelmezett): a CLI terminálkimenete.
- **GUI**: natív folyamatjelző sávok fájlonként, a figyelmeztetések és hibák sorokként,
  a CLI i/n kérdései pedig felugró dialógusként. A terminál egy összecsukható "CLI
  output" szakaszban továbbra is elérhető (a Segédeszközök menü parancsainál, amelyeknek
  nincs folyamatjelzőjük, magától kinyílik).

A futás módja az indításakor rögzül, ezért a kapcsoló futás közben le van tiltva. A
választás a CLI configjába kerül (a `deew-web`-nél is), egy saját szakaszba, és kézzel is
szerkeszthető:

```toml
[gui]
    progress = 'cli' # vagy 'native'
```

A kapcsoló csak létező `config.toml`-t módosít (futtasd egyszer a deew-t, hogy létrejöjjön),
különben "Setting not saved" üzenetet ír. A GUI mód a CLI által írt eseményeket olvassa,
amikor a `DEEW_EVENTS_FILE` be van állítva, ezért deew 3.6.2 vagy újabb kell hozzá.
A `deew-gui` és a `deew-web` is a mellettük lévő `deew` binárist futtatja; régebbinél
a GUI mód egy megjegyzést mutat, hogy a CLI nem jelent folyamatot, ilyenkor válts
CLI kimenetre.

## Felépítés

- `src/lib/deew-args.ts`: az opciók állapotából argv, és idézőjelezés a megjelenítéshez (unit tesztelve)
- `src/lib/runner.ts`: Tauri / webes transport; `web-api.ts`, `run-client.ts`: a webes mód HTTP és WebSocket kliense
- `src/components/`: címsor, segédeszközök menü, opciók űrlap, parancssáv, terminál panel, folyamat panel
- `core/src/pty_session.rs`: Tauri-független PTY session (unit tesztelve)
- `core/`: Tauri-független backend (CLI felderítés, probe-olás, PTY session)
- `src-tauri/src/lib.rs`: Tauri parancsok: `run_deew`, `write_stdin`, `resize`, `stop`
