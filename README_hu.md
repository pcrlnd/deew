[![builds](https://img.shields.io/github/actions/workflow/status/pcrlnd/deew/build-cli.yaml?logo=github&style=flat-square)](https://github.com/pcrlnd/deew/actions/workflows/build-cli.yaml)
[![github_release](https://img.shields.io/github/v/release/pcrlnd/deew?logo=github&color=70920c&style=flat-square)](https://github.com/pcrlnd/deew/releases)
[![crates_release](https://img.shields.io/crates/v/deew?style=flat-square)](https://crates.io/crates/deew)
[![license](https://img.shields.io/github/license/pcrlnd/deew?color=blueviolet&style=flat-square)](https://github.com/pcrlnd/deew/blob/main/LICENSE)
\
[![telegram](https://telegram-badge.vercel.app/api/telegram-badge?channelId=@deew_supp&style=flat-square&label=Discussion+and+support)](https://t.me/deew_supp)
\
[![commits](https://img.shields.io/github/last-commit/pcrlnd/deew?color=355ab8&logo=github&style=flat-square)](https://github.com/pcrlnd/deew/commits/main)
[![open_issues](https://img.shields.io/github/issues/pcrlnd/deew?color=718bcd&logo=github&style=flat-square)](https://github.com/pcrlnd/deew/issues)
[![closed_issues](https://img.shields.io/github/issues-closed/pcrlnd/deew?color=253e80&logo=github&style=flat-square)](https://github.com/pcrlnd/deew/issues?q=is%3Aissue+is%3Aclosed)
\
[![name](https://img.shields.io/badge/platform-win%20%7C%20linux%20%7C%20osx-eeeeee?style=flat-square)](https://github.com/pcrlnd/deew)
[![name](https://img.shields.io/badge/DEE-5.1.0--5.2.1-green?logoColor=white&logo=data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAABAAAAAMCAYAAABr5z2BAAAA9ElEQVQokZXSzypFURQG8N89LsnEv1KGBp5CKU9wBygPIJ5AeYM7MjLAzMBAmRuYyszU3AOgG6GQpZ19OM69Tny1Wu39rVXfWt9qRcQaljCOUbygh3c/MYQJDOMZ9zhvYwEb+hG1n9aAmlaBkfzYwQo2cZUbqnGJdSxjP/eMiYi9+MRiRKjEWXzjpMZ1MnNUVORM1+R1ck6jrNa4qZJrD5irRFrUHW4aahRNJN7w2lTQpCBhBpN/VfBU4w5yTr53a1war0/BPK7zQW1lS0tsYxa7eMDcFxMRhxW7/ovjpOAUj9madMppabe/nHKyOvWkcXu4+AAd1Ju1TsOvFgAAAABJRU5ErkJggg==&color=eeeeee&style=flat-square)](https://customer.dolby.com/content-creation-and-delivery/dolby-encoding-engine-with-ac-4)
<hr>
<p align="center"><img width="400" src="https://raw.githubusercontent.com/pcrlnd/deew/main/logo/logo-text-side-by-side.svg"><br>Dolby Encoding Engine Wrapper</p>

<p align="center"><a href="https://github.com/pcrlnd/deew/blob/main/README.md">English readme</a>
 • <a href="https://github.com/pcrlnd/deew/blob/main/README_hu.md">Magyar leírás</a></p>

## DDP encode-olás még sosem volt ilyen egyszerű!

![img](website/terminal.gif)

# Leírás
- kezeli a Dolby XML input baromságait és az RF64 intermediate fájlok létrehozását a háttérben, rendes felületet adva
- több kódolást párhuzamosan futtat batch encodingnál (lásd config)
- támogatja a WSL útvonalak konvertálását a DEE Windows verziójához (lásd config)
- automatikus mintavételezésiráta-konvertálás ffmpeg soxr resamplerét használva nem támogatott mintavételezési ráta esetén
  - DD/DDP/AC4 esetén a mintavételezési rátát 48 kHz-re konvertálja
  - TrueHD esetén a mintavételezési rátát 48 kHz-re konvertálja, ha a forrásé kisebb mint 72 kHz, fölötte 96 kHz-re
- automatikus csatornafelcserélés 7.1-es forrásoknál (DEE valamiért megcseréli az Ls, Rs csatornákat az Lrs, Rrs-sel)
- automatikusan kompenzálja a DEE 256 mintavételezés eltolását (DD és DDP encoding esetén)
- akár videó inputokkal is működik
- sokoldalú delay opció, ami támogat ms, s and és frame@fps formát is, delay parse-olása fájlnévből szintén támogatott

# Követelmények
- ffmpeg
- ffprobe
- Dolby Encoding Engine

# Dolby Encoding Engine telepítése
- telepítsd fel a [DEE](https://customer.dolby.com/content-creation-and-delivery/dolby-encoding-engine-with-ac-4) (ha macOS-t használsz akkor a [DME](https://customer.dolby.com/content-creation-and-delivery/dolby-media-encoder-with-ac-4)-t)
  - TrueHD encode-oláshoz csak a Windows-verzió használható
  - ha WSL-et használsz, a Windows-verzió használata ajánlott
  - ha a Windows-verziót használod Linux (és nem WSL) vagy macOS alól, telepítsd fel a `wine-binfmt`-t
- ha DEE `Failed to load library "...dll".` hibákat dob deew használatakor, telepítsd fel a [VisualCppRedist AIO](https://github.com/abbodi1406/vcredist/releases)-t

# deew (CLI) telepítése
### standalone buildet használva (Windows 10-11/Linux):
- tölsd le a legfrissebb buildet (`deew` / `deew.exe`) innen: [https://github.com/pcrlnd/deew/releases](https://github.com/pcrlnd/deew/releases)
- Linuxon tedd futtathatóvá: `chmod +x deew`
- futtasd: `deew` (Windowson `deew.exe`)\
*(terminálból futtasd, duplaklikk nem fog működni)*
- az első futtatáskor készíteni fog egy config fájlt
- frissítés: tölsd le a legfrissebb buildet innen: [https://github.com/pcrlnd/deew/releases](https://github.com/pcrlnd/deew/releases)

### forrásból buildelve (Windows/Linux/macOS):
- telepítsd a Rustot, ha még nincs fent: [https://rustup.rs](https://rustup.rs)
- futtasd a következő parancsot: `cargo install deew`
- futtasd: `deew`
- az első futtatáskor készíteni fog egy config fájlt
- frissítés: futtasd újra ugyanezt a parancsot: `cargo install deew`

### Használat
- `deew -h`

# GUI
- tölsd le a `deew-gui` / `deew-gui.exe` fájlt innen: [https://github.com/pcrlnd/deew/releases](https://github.com/pcrlnd/deew/releases), és told a CLI binárisod mellé ugyanabba a mappába (a mellette lévő `deew` fájlt keresi, Windowson `deew.exe`-t)
- Linuxon tedd futtathatóvá a binárisokat: `chmod +x deew deew-gui`
- Linuxon szükség van a WebKitGTK runtime-ra: `libwebkit2gtk-4.1-0` (Debian/Ubuntu), `webkit2gtk4.1` (Fedora), `webkit2gtk-4.1` (Arch)
- Windows 10/11 már tartalmazza a szükséges `WebView2` runtime-ot
- a GUI saját verziószámot használ, a CLI-tól függetlenül
- forrásból buildelve: `cd gui && npm install && npm run tauri build -- --no-bundle` (a binárist a `gui/target/release/` mappában találod), fejlesztéshez használd: `npm run tauri dev`
- részletesebb leírás (fejlesztés, webes mód, biztonsági megjegyzések): [GUI leírás](https://github.com/pcrlnd/deew/blob/main/gui/README_hu.md)

# Web UI
- a `deew-web` a GUI a böngésződben, hasznos lehet egy headless szerverhez (nem kell hozzá WebKit)
- tölsd le a `deew-web` / `deew-web.exe` fájlt innen: [https://github.com/pcrlnd/deew/releases](https://github.com/pcrlnd/deew/releases), és tedd a CLI binary mellé
- Linuxon futtasd: `chmod +x deew-web`
- indítsd el, és nyisd meg: [http://127.0.0.1:8765](http://127.0.0.1:8765), használd `--host 0.0.0.0`-t, hogy más gépekről is elérhető legyen
- bejelentkezés: futtasd a `deew-web --hash-password` parancsot, és másold a hash-t a `config.toml` fájlodba
- forrásból buildelve: `cd gui && npm install && npm run build && cargo build -p deew-web --release` (a binárist a `gui/target/release/` mappában találod)

# Rendszer PATH változók beállítása
Ha nem szeretnéd a teljes elérési utat használni a binárisokhoz a configban, vagy amikor CLI-ből használod őket, javaslom a rendszer PATH változók beállítását
### Windows:
- nyisd meg `cmd.exe`-t adminként
- futtas egy `setx /m PATH "%PATH%;[location]"` parancsot minden mappával, amiben binary van\
  *(a* `[location]`*-t cseréld le az elérési útra)*
- például:
```bat
setx /m PATH "%PATH%;C:\bin\dee"
setx /m PATH "%PATH%;C:\bin\ffmpeg"
```
### Linux/macOS:
- adj hozzá egy `PATH="[location]:$PATH"` sort a `~/.bashrc` vagy `~/.zshrc` fájlodhoz, minden mappával, amiben binary van\
  *(a* `[location]`*-t cseréld le az elérési útra)*
- például:
```sh
PATH="/usr/local/bin/dee:$PATH"
PATH="/usr/local/bin/ffmpeg:$PATH"
```

# Példák
`deew -i *thd`\
TrueHD -> DDP encode-olás

`deew -b 768 -i *flac`\
DDP encode-olás 768kbps-kel

`deew -f dd -i *dts -k`\
`deew -f ddp -i *dts`\
több formátum/bitráta kódolása egy ideiglenes fájl készítésével

# Beszélgetés és support
[https://t.me/deew_supp](https://t.me/deew_supp)
