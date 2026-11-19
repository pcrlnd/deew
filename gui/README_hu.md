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
  a CLI i/n kérdései pedig felugró dialógusként.
- **Segédeszközök** (csavarkulcs ikon, felirat nélkül): a Segédeszközök menü
  parancsainak kimenete, lásd lent. Nem kerül a configba; a CLI és a GUI azt
  mutatja tovább, amit eddig.

A futás módja az indításakor rögzül; a kapcsoló csak azt váltja, mit mutat az ablak,
futás közben is (a CLI kimenettel indított futásnál a GUI nézet megjegyzést mutat). A
választás a CLI configjába kerül (a `deew-web`-nél is), egy saját szakaszba, és kézzel is
szerkeszthető:

```toml
[gui]
    progress = 'cli' # vagy 'native'
    output = '' # kimeneti mappa (-o), üres: a home mappa
    scaling = '100%' # a deew-gui ablak nagyítása, '80%' - '150%'
    use_system_font = false # true: a rendszer betűtípusa az IBM Plex Sans helyett
```

A `scaling` a `deew-gui` ablak nagyítását állítja (80-150%, alapértelmezés: 100%);
induláskor lép életbe, ezért módosítás után indítsd újra az alkalmazást. A
`deew-web` figyelmen kívül hagyja, ott a böngésző saját nagyítását használd
(Ctrl +/-). A `use_system_font = true` beállítással a felület szövege a
mellékelt IBM Plex Sans helyett az operációs rendszer betűtípusát használja
(`deew-gui` és `deew-web` esetén is; a terminál megtartja a fix szélességű
betűtípusát). A GUI ezt a két kulcsot soha nem módosítja, kézzel szerkeszd őket.

A Kimeneti mappa mező értéke is ide kerül (`output`), és induláskor betöltődik.

A kapcsoló csak létező `config.toml`-t módosít (futtasd egyszer a deew-t, hogy létrejöjjön),
különben "Setting not saved" üzenetet ír. A GUI mód a CLI által írt eseményeket olvassa,
amikor a `DEEW_EVENTS_FILE` be van állítva, ezért deew 5.0.0 vagy újabb kell hozzá (`deew-gui`/`deew-web` 0.5.0 és újabb).
A `deew-gui` és a `deew-web` is a mellettük lévő `deew` binárist futtatja; régebbinél
a GUI mód egy megjegyzést mutat, hogy a CLI nem jelent folyamatot, ilyenkor válts
CLI kimenetre.

## Segédeszközök

A Segédeszközök menü (a `deew-gui` címsorában, a `deew-web` eszköztárában) a CLI
egyszeri parancsait futtatja: Help, Full help, Version, Changelog, List bitrates,
Print logos, Show config, Generate config és Download tools. Egy elem kiválasztása
átvált a Segédeszközök fülre (a CLI és a GUI utáni harmadik fül), és a parancs a
saját terminálján fut; a következő parancs törli az előző
kimenetét, a Stop pedig leállítja a futót.

- Külön pszeudo-terminálban futnak, ezért egy kódolás közben is működnek, és nem
  érintik annak terminálját, folyamatjelzőjét, előzményeit és Stop gombját.
  Egyszerre egy segédparancs fut.
- A Download tools (`-gt`) lecseréli az ffmpeg-et, a truehdd-t és a dee_shim-et,
  a Generate config (`-gc`) pedig újraírja a `config.toml`-t, ezért kódolás
  közben mindkettő szürke, és amíg valamelyik fut, kódolás sem indítható. A
  háttérrendszerek ezt is kikényszerítik.
- A Generate config mindig rákérdez: felülírja a `config.toml`-t az
  alapértékekkel, beleértve a `[gui]` beállításokat, az alapértelmezett
  bitrate-eket és a `deew-web` bejelentkezést.
- A háttérrendszerek csak ezeket a rögzített kapcsolókat fogadják el (`-h`, `-fh`,
  `-v`, `-cl`, `-lb`, `-pl`, `-c`, `-gc`, `-gt`), más argumentumot soha. A
  `deew-web` a `/api/info` WebSocketen szolgálja ki őket (ugyanaz a bejelentkezés,
  Host és Origin ellenőrzés, mint a `/api/run`-nál); a kimenet csak a kérő
  fülhöz megy, a fül bezárása leállítja a parancsot.

## Formátumok

A formátum legördülő menü a DD, DDP, DDP Atmos, AC4, AC4 Atmos, AC-4 IMS, AAC és
TrueHD formátumokat kínálja. Az opciók űrlap a CLI szabályait követi: a `-dm` csak
DD/DDP/AC4/AAC esetén, a `-dn` csak DD/DDP/AC4 esetén érhető el, az Atmos
formátumoknak (TrueHD Atmos input, truehdd-vel dekódolva) pedig nincs `-dm`-jük
és `-mo`-juk. GUI kimenetben az Atmos formátumok egy extra dekódolási szakaszt
mutatnak a truehdd-hez. Maguk az eszközök (DEE 6.7, truehdd, dee_shim) a CLI
mellé vagy a PATH-ba kerülnek, lásd a [fő leírást](../README_hu.md).

A „Surround phase shift” (`-sps`), „LFE low-pass filter” (`-lpf`) és „Surround
3 dB attenuation” (`-s3a`) jelölőnégyzetek a config értékeit mutatják
(`surround_phase_shift`, `low_pass_filter`, `surround_3db_attenuation`), és csak
akkor kerül jelző a parancsba, ha az adott futásra megváltoztatod valamelyiket.
A fázistolás DD/DDP/DDP Atmos esetén, az `-lpf` csak DD/DDP esetén, az `-s3a`
DD/DDP/TrueHD esetén érhető el (TrueHD-nél a CLI előbb rákérdez, mert a kimenet
nem lesz azonos a forrással; a kérdés a prompt ablakban / terminálban jelenik
meg); máshol szürkék.

Az **AC-4 IMS** (`ac4-ims`, immersive stereo, `.ac4`) 5.1 / 5.0 / 7.0 / 7.1 vagy
Atmos inputot (TrueHD Atmos, ADM BWF, DAMF) fogad, de Atmos és csatornaalapú
inputot nem egyszerre (a GUI figyelmeztet a sztereó / mono és a vegyes
inputokra). Bitrátái: 64, 96, 128, 144, 256 és 320 kbps (alapértelmezett: a
config `ac4_ims` értéke). A `-dm`, `-dl` és `-dn` szürke hozzá, a DRC
alapértelmezetten `none`, az „AC-4 IMS mode” választó (`--ac4-ims-mode`,
`general` vagy `music`) csak `music` esetén kerül a parancsba, ami a `music_light`
DRC-t választja (bekapcsolva a többi DRC érték szürke).

**Atmos masterek**: az ADM BWF (`chna` és `axml` chunkot tartalmazó `.wav`) és a
DAMF (`.atmos` a `.atmos.audio` és `.atmos.metadata` fájlokkal) felismerése
ugyanaz a fejléc-ellenőrzés, mint a CLI-ben (a GUI a CLI-vel vizsgáltatja meg az
inputokat), ffprobe nélkül; „ADM BWF Atmos master” / „DAMF Atmos master”
néven jelennek meg. DDP Atmos, TrueHD Atmos, AC4 Atmos és AC-4 IMS formátumra
kódolhatók, DD/DDP-re pedig (renderelés, más inputtal nem együtt); TrueHD, AC4
és AAC esetén a GUI figyelmeztet. A `-ti`-t figyelmen kívül hagyják. A fájlválasztó
ablakban és a webes fájlböngészőben nincs kiterjesztés-szűrő, minden `.wav` és
`.atmos` kiválasztható.

**Sávindex** (`-ti`): az `N` vagy `a:N` az N. hangsáv, az `s:N` a konténer N
indexű streamje (hangstreamnek kell lennie, különben hibát mutat az input). A GUI
ugyanazt a streamet vizsgálja, amelyet a CLI használni fog.

macOS-en a `deew-gui-macos` / `deew-web-macos` release binárisokat használd
(`chmod +x`, mint Linuxon).

## Felépítés

- `src/lib/deew-args.ts`: az opciók állapotából argv, és idézőjelezés a megjelenítéshez (unit tesztelve)
- `src/lib/runner.ts`: Tauri / webes transport; `web-api.ts`, `run-client.ts`: a webes mód HTTP és WebSocket kliense
- `src/components/`: címsor, segédeszközök menü, opciók űrlap, parancssáv, terminál panel, folyamat panel
- `core/src/pty_session.rs`: Tauri-független PTY session (unit tesztelve)
- `core/`: Tauri-független backend (CLI felderítés, probe-olás, PTY session)
- `src-tauri/src/lib.rs`: Tauri parancsok: `run_deew`, `write_stdin`, `resize`, `stop`
