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

## DDP encoding has never been so easy!

![img](https://telegra.ph/file/4e75ac457c8f122dfc9a9.gif)
<!---https://i.kek.sh/f2Iv7nZ2ucf.gif--->

# Description
- handles Dolby's XML input fuckery and creating the RF64 intermediate in the background, giving you a proper tool
- runs multiple encodes in parallel for batch encoding (see config)
- supports WSL path conversion for the Windows version of DEE (see config)
- automatic sample rate conversion using ffmpeg's soxr resampler in case of an unsupported sample rate
  - for DD/DDP/AC4 unsupported rates will be converted to 48 kHz
  - for TrueHD unsupported rates will be converted to 48 kHz if source sample rate is lower than 72 kHz, otherwise will be converted to 96 kHz
- automatic channel swapping for 7.1 sources (DEE swaps Ls Rs with Lrs Rrs for some reason)
- automatically compensates for DEE's 256 sample delay (DD and DDP encoding)
- works even with video inputs
- versatile delay option that supports ms, s and also frame@fps formats, parsing delay from filenames also supported

# Requirements
- ffmpeg
- ffprobe
- Dolby Encoding Engine

# Dolby Encoding Engine installation
- install [DEE](https://customer.dolby.com/content-creation-and-delivery/dolby-encoding-engine-with-ac-4) (if you use macOS, install [DME](https://customer.dolby.com/content-creation-and-delivery/dolby-media-encoder-with-ac-4))
  - for TrueHD encoding support you need the Windows version
  - if you use WSL using the Windows version is recommended
  - if you use the Windows version of DEE under Linux (and not from WSL) / macOS install `wine-binfmt`
- if DEE throws `Failed to load library "...dll".` errors when you run deew install [VisualCppRedist AIO](https://github.com/abbodi1406/vcredist/releases)

# deew (CLI) installation
### with standalone build (Windows 10-11/Linux):
- grab the latest build (`deew` / `deew.exe`) from: [https://github.com/pcrlnd/deew/releases](https://github.com/pcrlnd/deew/releases)
- on Linux make it executable: `chmod +x deew`
- run with: `deew` (`deew.exe` on Windows)\
*(run the binary from terminal, doubleclicking it won't work)*
- on the first run it will create a config file
- updating: grab the latest build from: [https://github.com/pcrlnd/deew/releases](https://github.com/pcrlnd/deew/releases)

### build from source (Windows/Linux/macOS):
- install Rust if you don't have it already: [https://rustup.rs](https://rustup.rs)
- run: `cargo install deew`
- run with: `deew`
- on the first run it will create a config file
- updating: run the same command again: `cargo install deew`

### Usage
- `deew -h`

# GUI
- download `deew-gui` / `deew-gui.exe` from [https://github.com/pcrlnd/deew/releases](https://github.com/pcrlnd/deew/releases) and put it in the same folder as the CLI binary (it looks for `deew` next to itself, `deew.exe` on Windows)
- on Linux make the binaries executable: `chmod +x deew deew-gui`
- Linux needs the WebKitGTK runtime: `libwebkit2gtk-4.1-0` (Debian/Ubuntu), `webkit2gtk4.1` (Fedora), `webkit2gtk-4.1` (Arch)
- Windows 10/11 already include the needed `WebView2` runtime
- the GUI has its own version number, independent from the CLI
- build from source: `cd gui && npm install && npm run tauri build -- --no-bundle` (the binary will be in `gui/target/release/`), for development use `npm run tauri dev`
- more details (development, web mode, security notes): [GUI readme](https://github.com/pcrlnd/deew/blob/main/gui/README.md)

# Web UI
- `deew-web` is the GUI in your browser, can be handy for a headless server (no WebKit needed)
- download `deew-web` / `deew-web.exe` from [https://github.com/pcrlnd/deew/releases](https://github.com/pcrlnd/deew/releases) and put it next to the CLI binary, 
- on Linux run `chmod +x deew-web`
- run it and open [http://127.0.0.1:8765](http://127.0.0.1:8765), use `--host 0.0.0.0` to make it reachable from other machines
- login: run the `deew-web --hash-password` command and paste the hash into your `config.toml` file
- build from source: `cd gui && npm install && npm run build && cargo build -p deew-web --release` (the binary will be in `gui/target/release/`)

# Setup system PATH variable
If you don't want to use full paths for the binaries in your config or when you use them from CLI, I suggest to setup system PATH variables
### Windows:
- open `cmd.exe` as administrator
- run a `setx /m PATH "%PATH%;[location]"` command for each path that contains binaries\
  *(replace* `[location]` *with the path)*
- for example:
```bat
setx /m PATH "%PATH%;C:\bin\dee"
setx /m PATH "%PATH%;C:\bin\ffmpeg"
```
### Linux/macOS:
- add a `PATH="[location]:$PATH"` line in your `~/.bashrc` or `~/.zshrc` file for each path that contains a binary\
  *(replace* `[location]` *with the path)*
- for example:
```sh
PATH="/usr/local/bin/dee:$PATH"
PATH="/usr/local/bin/ffmpeg:$PATH"
```

# Examples
`deew -i *thd`\
TrueHD -> DDP encoding

`deew -b 768 -i *flac`\
DDP encoding at 768kbps

`deew -f dd -i *dts -k`\
`deew -f ddp -i *dts`\
encode multiple formats/bitrates while creating the temp file only once

# Discussion and Support
[https://t.me/deew_supp](https://t.me/deew_supp)
