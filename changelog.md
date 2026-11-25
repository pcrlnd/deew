# deew 5.6.0:
- raw TrueHD inputs (`.thd`, `.mlp`) keep their decode progress with the
  next truehdd release: deew passes `--progress` to truehdd when it reads
  the file itself, because truehdd will only count the frames (which deew
  uses for the percentage) when a progress bar is asked for
- new `-u, --update [all]`: updates deew, and `deew-gui`/`deew-web` when
  they are next to it, from the latest release; `-u all` also installs the
  missing ones (the download is checked against GitHub's SHA-256)
- `-v` also shows where deew itself is, and in a terminal it shows deew's
  version right away and each tool as soon as it answers
- deew needs dee_shim 0.4.0 and truehdd 0.6.3 or newer: a job that uses
  an older one stops with a message (`-gt` downloads the latest); jobs
  that don't use them are not affected
- `-mo` of 7.1 and 7.0 measures with dee_shim's `measure_loudness`, the
  real 7.1 loudness of all channels; before, it ran the DDP encoder and
  stopped it, which measured its 5.1 downmix and could read up to ~5 dB
  lower with loud surrounds (that is still what a 7.1/7.0 DDP encode sets
  its dialnorm from, `-mo` says so)
- 5.0 DD/DDP and 7.0 DDP are written without the silent LFE channel
  (dee_shim's `--unpadded-wav`), the output is the same; 5.0 DD and DDP
  now need dee_shim too
- the dee_shim dry-run check of `named_pipe://` inputs is gone (fixed
  since dee_shim 0.2.0)
- macOS: the tools are also looked up in `/opt/homebrew/bin` and
  `/usr/local/bin` (an app started from Finder doesn't get the `PATH` of
  the terminal)
- `deew-gui`/`deew-web` 1.0.1:
  - macOS: `deew-gui` is released as `deew-gui-macos.zip` with
    `deew-gui.app` in it, so it has its icon in the Dock and Finder; keep
    `deew-macos` next to `deew-gui.app`, the GUI uses that CLI
  - new "Update" (`-u`) and "Update and install all" (`-u all`) in
    Utilities, they ask first and can't run during an encode; without a
    deew CLI only "Update and install all" is available, and it downloads
    the CLI first
  - macOS: with `deew-gui.app` in Applications, the CLI, `config.toml`,
    `dee/` and the tools are in `~/Library/Application Support/deew/`
  - `deew-web.exe` has the deew icon
  - the Linux `deew-gui` uses the 128x128 icon as its window icon,
    which makes the binary about 1 MB smaller

# deew 5.5.3:
- new advanced `-p, --pipe auto|always|never` (only in `-fh`): overrides
  the config's `pipe` for one run
- new configs have `theme = 'system'` in the `[gui]` section
- `deew-gui`/`deew-web` 1.0.0:
  - 1.0.0 because the GUI is considered complete and stable now
  - the theme button opens a menu with Light, Dark and System (follows
    the operating system); `deew-gui` saves the choice as `theme` in the
    `[gui]` section of the config (`'system'`, `'light'` or `'dark'`,
    default `'system'`), `deew-web` starts on System and each browser
    keeps its own choice

# deew 5.5.2:
- piped encodes (DD, DDP, AC4, AAC and `-mo` when ffmpeg feeds DEE
  directly) show real progress instead of jumping from 0 to 100%: DEE
  prints no progress while it reads a pipe, so deew follows ffmpeg's
  position against the source's length, and in DEE's encoding step after
  the measurement the size of the output file; when the length of the
  source is unknown, the step that reads the source shows a spinner and
  the length found there is used for the rest

# deew 5.5.1:
- the encoder delay is trimmed correctly in two cases where the output
  started late:
  - AAC below 88 kbps: DEE's AAC encoder adds 1710 samples up to
    71 kbps (56, 64) and 1702 for 72-87 kbps (80), not 1600 (110 and
    102 samples late)
  - downmixes done by DEE add 32 samples: DD/DDP with `-dm`, DD from a
    7.1 or TrueHD Atmos source, `-dm` from an Atmos master and AAC from
    5.1/7.1 (32 samples late, 64 kbps AAC from 5.1: 141)
  - TrueHD, AC-4 IMS (ffmpeg downmixes) and DDP Atmos were already
    sample-exact; AC-4 is unchanged (it can't be checked without an
    AC-4 decoder)

# deew 5.5.0:
- new advanced `-wm, --warp-mode normal|warping|prologiciix|loro` and
  `-bc, --bed-conform on|off` for TrueHD Atmos sources: truehdd's warp
  mode (only used when the source has none) and bed conformance, config
  keys `truehdd_warp_mode` and `truehdd_bed_conform`; a kept (`-k`)
  decode is only reused with the same options
- new `-on, --output-name NAME`: the name of the output file for a
  single input file (counted after folders and wildcards are expanded);
  deew adds the extension (`.ec3`, `.eb3`, `.thd`, ...), a name with a
  folder is an error (`-o`/`-oi` set the folder), a different output
  extension is an error
- new `-ot, --output-template TEMPLATE` (config: `template` in `[output]`,
  default `{stem}`): a pattern for the output file names, deew fills in
  the placeholders for each input:
  - `{stem}`: the input's name without extension and delay tag
  - `{name}`: the input's name without extension, delay tag kept
  - `{format}`: the `-f` value (`ddp`, `thd-atmos`, ...)
  - `{codec}`: the output format as the summary shows it
  - `{channels}`: the output's channel layout (`2.0`, `5.1`, `7.1`)
  - `{bitrate}`: the bitrate in kbps (empty for TrueHD)
  - `{lang}`, `{title}`: the language and title tags of the audio track
  - `{track}`: the `-ti` value

  e.g. `{stem}.{lang}` gives `Movie.eng.ec3`; a placeholder with no value
  (e.g. no language tag) is left out together with the `.`, `_`, `-` or
  space next to it, characters Windows doesn't allow become `_`
- two inputs that would write the same output file (compared without
  regard to case) or an output that is its own input are an error before
  anything runs; `-on` and `-ot` can't be used together or with `-mo`
- `-h` is shorter: `--ddp-mode`, `--ddp-atmos-mode`, `--ac4-ims-mode`,
  `-dl`, `-in`, `-kl`, `-la`, `-pl` and `-ot` are only in `-fh` now
- `deew-gui`/`deew-web` 0.10.0:
  - "Warp mode" select and "Bed conform" checkbox for TrueHD Atmos
    sources (`-wm`, `-bc`): they start with the config's values and a
    flag is only passed when you change one
  - "Output name" (`-on`, greyed out unless exactly one file is added)
    and "Output template" (`-ot`, starts with the config's template and
    is only passed when you change it) fields; under them the first
    file's output name when the template isn't `{stem}`, or the first
    naming error; `--probe-json` has the `language` and `title` tags of
    the tracks for it

# deew 5.4.0:
- `-sps, --surround-phase-shift on|off` replaces `-nps`: it
  overrides the config's `surround_phase_shift` in both directions
  (`-sps on` turns the phase shift back on)
- new `-lpf, --low-pass-filter on|off` for DD and DDP: DEE's
  120 Hz low-pass filter on the LFE channel,
  on by default, config key: `low_pass_filter`
- new `-s3a, --surround-3db-attenuation on|off` for DD, DDP and
  TrueHD: 3 dB attenuation of the surround channels,
  off by default, config key: `surround_3db_attenuation`;
  TrueHD (6 and 8 channel outputs) asks first because the output is
  no longer identical to the source
- new `-fh`/`--full-help` shows all options, `-h` doesn't show the
  advanced options anymore (`-sps`, `-lpf`, `-s3a`)
- new `-f ac4-ims` (AC-4 immersive stereo, DEE's `dee_ac4ims_encoder`,
  `.ac4`): stereo for headphones and TV speakers from a 5.1 source (7.1
  is downmixed to 5.1 by ffmpeg, 5.0 gets a silent LFE), from a TrueHD
  Atmos source (decoded by truehdd) or from an Atmos master; bitrates
  64, 96, 128, 144, 256 and 320 kbps (default 256, config key
  `ac4_ims` in `[default_bitrates]`, configs without it use 256);
  `-dm`, `-dl` and `-dn` are errors with it
- new `--ac4-ims-mode general|music`: `music` uses DEE's music mode
  (no dialogue enhancement, DRC `music_light`, an other `-r` is an error);
  the default comes from the config's `ac4_ims` in `[default_modes]`
  (configs without it use `general`)
- Dolby Atmos masters are read by DEE directly (#75): ADM BWF (a wave
  file with `chna` and `axml` chunks, found by its headers) and DAMF
  (`.atmos` with its `.atmos.audio` and `.atmos.metadata`) work as
  inputs of `ddp-atmos`, `thd-atmos`, `ac4-atmos` and `ac4-ims` without
  truehdd and ffmpeg, `-mo` measures them, `dd`/`ddp` get a 5.1 (or `-dm
  2`/`1`) render of the master from DEE, `thd`, `ac4` and `aac` reject
  them; the summary shows the source as "ADM BWF Atmos master" /
  "DAMF Atmos master"; in a folder the `.atmos.audio` and
  `.atmos.metadata` of a DAMF are not inputs of their own, a DAMF with a
  missing part is an error
- `-i` takes wildcard patterns (`*`, `?`, `**`) on every platform, quoted
  or not: cmd.exe and PowerShell don't expand them, deew does; an
  existing path wins over a pattern, brackets are literal, hidden files
  are skipped, a pattern without a match is an error, the same file given
  twice is encoded once
- `-ti` takes `N`/`a:N` (the Nth audio track) and `s:N` (the stream with
  index N of the container, it has to be an audio stream); negative
  values are an error now
- deew keeps the computer from sleeping while it encodes or measures
- `dee_ac4ims_encoder` is in the tools `-v` looks at
- `-gc` asks before it overwrites an existing `config.toml` (`-np` skips
  the question); creating a missing config still doesn't ask
- `pipe = 'auto'` is the new default: on Windows deew uses the
  temporary `.wav` when 2 or more encodes run at once (DEE's named pipe
  made 4 parallel encodes about 3x slower), a single encode and
  Linux/macOS keep the pipe; `pipe = true` in older configs means
  `auto`, `pipe = 'always'` forces the pipe, `pipe = false` turns it off;
  the summary has a new `Piping` row
- `deew-gui`/`deew-web` 0.9.0:
  - the GUI asks deew to probe the inputs (`deew --probe-json`, hidden)
    instead of doing it itself, so what it shows (folders, patterns, `-ti`,
    Atmos masters, skipped files) always matches the deew next to it; an
    older deew is reported as too old
  - the GUI tab shows two lines per file, the name and below it the
    status text like the CLI's (`finished (-25 dB) - took 10:15`,
    `failed: reason`, `stopped`), the bar and the percent, on alternating
    row backgrounds; deew's `done` progress event carries the result, the
    duration and the failure reason for it
  - the CLI tab's terminal is dark grey instead of near black in the
    light theme
  - new Utilities tab (wrench icon, after the CLI and GUI tabs): the
    Utilities menu runs its commands there, in their own terminal, so
    they work while an encode is running (one utility at a time, the next
    one clears the previous output)
  - Download tools (`-gt`) and Generate config (`-gc`) are greyed out
    while an encode is running, and an encode can't start while one of
    them runs
  - Generate config asks for confirmation first, as it overwrites
    `config.toml` (including the `[gui]` settings, the default bitrates
    and the `deew-web` login)
  - new Full help (`-fh`) item in the Utilities menu
  - "No surround phase shift" is replaced by three on/off checkboxes,
    "Surround phase shift" (`-sps`), "LFE low-pass filter" (`-lpf`) and
    "Surround 3 dB attenuation" (`-s3a`): they start with the config's
    values and a flag is only passed when you change one; greyed out
    for formats that don't have them
  - new AC-4 IMS (`ac4-ims`) format with the "AC-4 IMS mode"
    (`--ac4-ims-mode`) select; `-dm`, `-dl` and `-dn` are greyed out for
    it, the music mode selects the DRC `music_light`, and stereo / mono
    inputs and Atmos mixed with channel-based inputs are reported; the
    select shows the config's default (`ac4_ims` in `[default_modes]`)
    and the GUI doesn't pass the flag for it
  - ADM BWF and DAMF Atmos masters are recognised as Atmos sources
    (without ffprobe, with the CLI's own detection) and the GUI warns
    about the formats that can't take them
  - the track index accepts `N`, `a:N` and `s:N`, and the GUI probes
    the stream that the CLI will use

# deew 5.3.1:
- `-mo` measures a TrueHD Atmos source with `dee_measure_loudness`
  (the multichannel loudness of the Atmos master) instead of stopping
  DEE's DDP Atmos encoder once it has measured
- `deew-gui`/`deew-web` 0.8.0:
  - new `scaling` key in the `[gui]` section of the config: zoom of the
    `deew-gui` window, `'80%'` to `'150%'`, default `'100%'`
    (`deew-web` uses the browser's zoom)
  - new `use_system_font` key in `[gui]`: `true` uses the system's font
    instead of the bundled IBM Plex Sans, default `false`

# deew 5.3.0:
- `-gt` downloads the glibc (gnu) build of truehdd on Linux instead of
  the musl one (DEE needs glibc anyway)
- `-r none` works for AC4 Atmos and is now its default, like for every
  other format that has it (DEE's AC4 Atmos encoder has the preset, plain
  AC4 doesn't and keeps `music_light`)
- `-nps` works for DDP Atmos too (dee_shim's `--phase90 off` is used)
- `-mo` also shows the loudness range, the true peak and, for
  dialogue-gated sources, the program loudness
- truehdd failures show truehdd's own error message
  (e.g. `Write error: ...`) instead of only the exit code
- a TrueHD Atmos source whose format changes mid-stream (truehdd splits
  its output) now stops with an error instead of encoding only the first
  part, and truehdd's extra files are removed from temp
- AC4 with `-dn` no longer writes a temporary `.wav`: ffmpeg pipes the
  audio into the measurement and again into the encode
- a warning when truehdd reports damage in a TrueHD Atmos source
  (access units replaced with silence or skipped, invalid branch points)
- 96 kHz TrueHD is lossless now: it's encoded through dee_shim with
  `--apodise off` (DEE's own encoder applies its apodising filter at
  96 kHz, which changes every sample)
- `deew-gui`/`deew-web` 0.7.0:
  - DRC `none` is shown greyed out where it isn't available instead of
    being hidden
  - "No surround phase shift" and DRC `none` for AC4 Atmos follow the CLI
    (need deew 5.3.0 or newer)
  - shows the loudness range and true peak of `-mo` too

# deew 5.2.0:
- ffmpeg's audio goes to DEE through a pipe instead of a temporary `.wav`
  (DD, DDP, AC4, AAC and `-mo`), so encodes need much less disk space and
  start sooner; on Windows DEE creates a named pipe that ffmpeg writes
  to. TrueHD, AC4 `-dn` and Atmos keep using temporary files.
  `pipe = false` in the config switches back to the `.wav`. On Windows
  7.1 DDP with the standard profile and 7.0 DDP (encoded with dee_shim)
  need `dee_shim 0.2.0` or newer for the named pipe, with an older one they
  use the `.wav`
- `-mo` measures the source as it is: `-f`, `-dm`, `-r` and `-dn` are
  ignored, and a TrueHD Atmos source is decoded with truehdd and measured
  by DEE's DDP Atmos encoder
- `-mo` shows how the loudness was gated, e.g. `-33 dB, dialogue-gated,
  16% speech`
- new `-nps`/`--no-phase-shift`: turns off the 90° phase shift of the
  surround channels for DD and DDP (`surround_phase_shift = false` in the
  config turns it off for every encode)
- `deew-gui`/`deew-web` 0.6.0:
  - new "No surround phase shift" option (DD and DDP)
  - `-mo` is available for Atmos sources and shows the gating and the
    speech share (needs deew 5.2.0 or newer)

# deew 5.1.0:
- `-dn`: the custom dialnorm is given to DEE directly
  (`--loudness-management skip:dialnorm=N`) for DD and DDP, so there is no
  separate loudness measurement before the encode anymore
- `-dn` works for DDP Atmos, TrueHD and TrueHD Atmos too (the same way),
  and with 7.1 input for DDP and TrueHD; AC4 still measures and edits the
  loudness manifest, and rejects 7.1 input (DEE's AC4 encoder downmixes it
  to 5.1 itself)
- `-mo` works with 7.1 input: DEE's DDP encoder measures it and is stopped
  as soon as the loudness is known (`dee_measure_loudness` doesn't take 7.1)
- `deew-gui`/`deew-web` 0.5.1:
  - `-dn` is available for DDP Atmos, TrueHD and TrueHD Atmos too (needs
    deew 5.1.0 or newer)

# deew 5.0.3:
- resizing the terminal quickly during encoding or `-gt` no longer leaves
  stale progress bar lines behind or overwrites the lines above them
  (or much less likely)
- the temporary `.atmos` files keep the non-ASCII characters of the input's
  name (they were transliterated to ASCII, so e.g. `Film_é` and `Film_e`
  got the same name)
- smaller binaries (~1.4 MB less): `deunicode` was removed and `regex` is
  built without its Unicode tables and speed-ups

# deew 5.0.2:
- resizing the terminal (or deew-gui) during encoding or `-gt` no longer
  duplicates or garbles the progress bars, deew draws them itself now
  instead of using indicatif
- `-gt`: the progress bars look like the encoding ones: `│` columns, bold
  cyan tool names, the remaining time after 5 seconds and two lines per tool
  below 80 columns

# deew 5.0.1:
- `-gt` on macOS: downloading ffmpeg and ffprobe failed with `the checksum
  file has no SHA256`
- `-gt`: the first error line no longer gets a red background up to the end
  of the line (e.g. in macOS Terminal)
- the colors are reset at the end of every line, improving `-h` and `-lb`

# deew 5.0.0:
- DEE 6.7.0 support: deew now drives DEE's command line tools
  (`dee_ddp_encoder`, `dee_dthd_encoder` etc...),
  DEE 5.2.1 and the XML interface are no longer supported
- new formats for `-f`: `ddp-atmos` (#31), `thd-atmos`, `ac4-atmos` and `aac`
  - `ddp-atmos`, `thd-atmos` and `ac4-atmos` take a TrueHD Atmos input, it is
    decoded with [truehdd](https://github.com/truehdd/truehdd) and re-encoded
    from the Atmos master
  - the input can be a raw `.thd` (decoded directly) or a container
    (mkv/mka/m2ts, the track is selected with `-ti` and piped from ffmpeg to
    truehdd)
  - the decoded length is checked against the input, a truncated decode is
    an error
  - `ddp-atmos` and `thd-atmos` require
    [dee_shim](https://github.com/truehdd/dee-shim), so DEE doesn't smooth the
    object positions a second time
  - `-dm` and `-mo` can't be used with the Atmos formats
- `aac` output is always 2.0 CBR (a DEE limitation),
  mono inputs and `-dm 1` are rejected
  - deew suggests qaac for AAC encoding
- `ac4` encoding now creates 2.0 or 5.1 output (it used to create immersive
  stereo from 5.1 sources only), `-dm` works for it
- `-dm 2` and `-dm 6` work for TrueHD: the TrueHD encoder can't downmix, so
  deew downmixes with ffmpeg using the coefficients of DEE's own downmix (the
  stereo downmix has a limiter against clipping, like DEE)
- new `--ddp-mode` option (`standard` / `bluray`, default from the config's
  new `[default_modes]` section) for 7.1 DDP: `standard` (the default) is
  E-AC-3 independent and dependent substreams, 384-1792 kbps, `.ec3`, which
  uses dee_shim; `bluray` is the Blu-ray profile (AC-3 core, 768-1664 kbps,
  `.eb3`), without dee_shim 7.1 DDP needs `--ddp-mode bluray` (otherwise it is
  an error), `-fs` was removed
- new `--ddp-atmos-mode` option (`streaming` / `standard` / `bluray`, default
  from `[default_modes]`) for `ddp-atmos`: `streaming` (the default) is 5.1
  with Atmos (384-1024 kbps, `.ec3`), `standard` is 7.1 with Atmos, E-AC-3
  only (768-2048 kbps, `.ec3`), `bluray` is 7.1 with Atmos with an AC-3 core
  (1152-1664 kbps, `.eb3`)
- new `-oi`/`--output-to-input` option: saves each output next to its input
  file, overrides `-o` and the config's output path
- the CLI uses the config's new `[output]` `path` as the output directory
  (overridden by `-o`), `to_input` is the default of `-oi`
- 5.0 and 7.0 inputs are supported (other 5- and 7-channel layouts are
  rejected), DD and DDP create a real 5.0/7.0, TrueHD and AC4 keep a silent LFE
- new `-dl`/`--disable-lfe` option for 5.1 and 7.1 inputs (5.0 and 7.0 inputs
  always behave this way) (#74):
  - DD and DDP create a real 5.0 (`lfe_on=0`), DDP creates a real 7.0 from 7.1
    input, which needs dee_shim (an error without it)
  - TrueHD and AC4 can't leave out the LFE channel, it is kept silent
    (with a note)
  - ignored with a note for AAC, `-dm 1`/`-dm 2`, the Atmos formats and
    mono/stereo inputs
- `-h` and `-lb` mark the values that use dee_shim in blue
- the default DRC is `none` now (`music_light` for AC4)
- `-r none` is rejected for AC4, and TrueHD with DRC `none` requires dee_shim
  (the TrueHD encoder has no such preset) (#29)
- custom dialnorm (`-dn`) is applied through a loudness manifest and only
  works for DD, DDP and AC4
- `-mo` measures the loudness with `dee_measure_loudness` instead of killing DEE
  when the dialnorm is printed, `-mo` and `-dn` don't work with 7.1 input
  (`dee_measure_loudness` supports up to 5.1)
- WSL/Linux path conversion was removed, use native version on every OS
- macOS support: universal (Apple silicon + x86_64) builds:
  `deew-macos`, `deew-gui-macos`, `deew-web-macos`
- config changes, run `deew -gc` to generate a new config:
  - the tool paths were removed (`ffmpeg_path`, `ffprobe_path`, `dee_path`),
    old configs still work and these keys are ignored
  - new `default_bitrates` keys: `ddp_7_1_standard`, `ddp_7_1_bluray`,
    `ddp_atmos_streaming`, `ddp_atmos_standard`, `ddp_atmos_bluray`,
    `ac4_5_1`, `ac4_atmos` and `aac_2_0` (`ddp_7_1` is ignored), the default of
    `ac4_2_0` is 256 now (was 320)
  - new `[output]` section (`path`, `to_input`) and `[default_modes]` section
    (`ddp`, `ddp_atmos`)
- ffmpeg, ffprobe, truehdd and dee_shim are looked for next to deew first,
  the DEE tools in a `dee` folder next to deew, then in your PATH (there's
  nothing to set in the config)
- new `-gt`/`--get-tools` option: downloads ffmpeg and ffprobe (latest
  stable, GPL, static), truehdd and dee_shim next to deew, in parallel with a
  progress bar for each, the downloads are checked against their SHA256
- new `-kl`/`--keep-log` option: keeps TrueHD's `.thd.log.html` (the `.mll`
  is always deleted) and writes a standalone `<name>.deew-log.html` report
  next to each output (also for failed jobs) with the settings, tool versions,
  command lines and the complete output of every stage
- the encoding summary shows the truehdd and dee_shim versions, `-v` prints the
  versions of the tools too (DEE, truehdd, dee_shim, ffmpeg, ffprobe), listed
  by name in one aligned column, with the path where each one was found
- if any of the tools (ffmpeg, truehdd, DEE) fails, the job fails with the
  error of the tool and deew exits with an error code (#63)
  - ffmpeg's error is its last line that names an error (e.g. `Error closing
    file: No space left on device`, with the temp path when the disk is full)
    instead of `Conversion failed!`
  - a DEE error printed without an `ERROR:` prefix is shown too
- new `-lb` layout: a tree grouped by codec (DD, DDP, AC4, AAC) with the
  channel layouts and the profiles of `--ddp-mode` and `--ddp-atmos-mode`
  below them, aligned values wrapped to the terminal width (at most 78
  columns), the default bitrates and modes (from the config) marked with
  `(default)`, and the bitrates of the new formats
- `-h` (and the usage shown with errors) wraps to the terminal width with the
  descriptions aligned, below 80 columns each description goes under its
  flags, the usage wraps between options and is at most 80 columns wide
- new CLI progress display:
  - each line shows the step (`ffmpeg decode`, `truehdd decode`,
    `DEE measure`, `DEE encode`) with its own color, the file name, the
    dialnorm, the bar, the percentage and the estimated time left (after the
    step has run for 5 seconds)
  - the measure and encode steps both go from 0 to 100% (non-Atmos TrueHD's
    first step is a pulsing bar)
  - the columns size themselves to the file names and the terminal width, long
    names are cut, below 80 columns each file uses two lines
  - finished and failed files are printed above the running ones
    (`finished: name (-27 dB) - took 1:12`, `failed: name - error`), the files
    waiting to start are shown as `+3 files in queue`
- the encoding summary shows the sample rate of the inputs (`48 kHz`), and
  the number of files moved to the Input(s) section
- the Atmos formats check the input's ffprobe profile before anything is
  decoded: a TrueHD input without Atmos is rejected right away (truehdd's
  "no Atmos presentation" check stays as a safety net)
- folder inputs: files without an audio track (e.g. `.nfo`, `.jpg`) or that
  ffprobe can't open are listed with the reason, and deew asks whether to skip
  them and encode the rest (`-np` skips them with a note), if nothing is left
  it is an error
- probing errors name the file and the reason (`asd.jpg has no audio track.`,
  `no audio track 3 (-ti), it has 2 (0-1)`, `ffprobe: Permission denied`)
  instead of a general ffprobe error, and all of them are listed at once (#71)
- Ctrl-C (also SIGTERM/SIGHUP on Unix, Ctrl-Break and console close on
  Windows) stops deew cleanly: no new job starts, the running tools (DEE,
  dee_shim, ffmpeg, truehdd) are killed, the temp files and the partial
  outputs of the stopped jobs are removed (finished jobs keep their outputs),
  the `-kl` report of a stopped job says `interrupted`, a summary line is
  printed (`interrupted: 2 jobs finished, 1 stopped, 3 not started`) and the
  exit code is 130; a second Ctrl-C exits at once, Ctrl-C at a prompt exits
  with 130 too
- logos: logos 3-11 were removed, logo 1's text is bold white, logo 2 is blue
  instead of orange, and logo 3 is a new 60-column ANSI art (`logo` is now
  1-3, any other value shows no logo)
- updated dependencies
- `deew-gui`/`deew-web` 0.5.0:
  - the format dropdown has DD, DDP, DDP Atmos, AC4, AC4 Atmos, AAC, TrueHD
    and TrueHD Atmos
  - the options follow the new rules: no `-fs`, `-dm` is available for
    DD/DDP/AC4/AAC and TrueHD (2.0 and 5.1), `-dn` for DD/DDP/AC4 only, `-mo`
    is not available for the Atmos formats
  - "DDP mode" (`--ddp-mode`) and "DDP Atmos mode" (`--ddp-atmos-mode`)
    dropdowns, always shown and greyed out unless they apply (7.1 DDP / DDP
    Atmos), defaulting to the config's `[default_modes]`
  - the options use as many columns as fit and are spread evenly over the
    rows, so every row is full width (5 + 5, 4 + 3 + 3, 3 + 3 + 2 + 2, ...)
  - the window can be made smaller (minimum 600 × 400, the page scrolls when
    needed) and the output area is at least twice as tall as before
  - "Disable LFE" (`-dl`) checkbox, greyed out unless the input is 5.1/7.1 and
    the output can use it (not the Atmos formats, not AAC, not `-dm 1`/`-dm 2`)
  - 5.0 and 7.0 inputs are recognised
  - DRC `none` is offered for all formats except AC4 and AC4 Atmos (AAC has no
    DRC)
  - "Keep log (`-kl`)" checkbox
  - values that use dee_shim show a puzzle icon (tooltip: "Uses dee_shim"):
    DDP Atmos, TrueHD Atmos, DRC `none` for TrueHD, DDP mode `standard` and its
    7.1 bitrates, every DDP Atmos mode and bitrate
  - the probe line shows `Atmos` before the channels of Atmos inputs
    (`Atmos 7.1 - 48 kHz - 24-bit - 1 file`); the Atmos formats warn about DDP
    Atmos, TrueHD without Atmos and other non-TrueHD inputs
  - folder files that aren't audio are listed as skipped under the probe line
    (`asd.jpg: skipped, no audio track`) instead of as errors, and probing
    errors name the reason (#71)
  - Stop sends Ctrl+C on Windows too (deew removes its temp files and partial
    outputs) and waits 10 s before killing; exit code 130 is shown as "Stopped"
  - Run and Stop are one button that switches between Run, Stop and
    "Stopping..." (with a spinner) and keeps the width of its widest label
  - the "Clear" button is called "Clear all" now
  - "Download tools" (`-gt`) at the end of the Utilities menu
  - the Output directory field shows the real default folder instead of
    `default (from config)` and is saved to the config's new `[output]` `path`
    key on every change, then loaded on startup
  - "Save next to input" (`-oi`) checkbox (first in the checkbox row), saved
    to `[output]` `to_input`, it greys out the Output directory field (its
    value is kept)
  - the native progress view shows each step from 0 to 100% and the step
    labels match the CLI (`DEE encode`, `truehdd decode`)
  - deew-web: "Keep me logged in" choice on the login page (until the browser
    is closed, 1 day, 7 days (default), 30 days; the last choice is
    remembered), the session cookie lasts as long as the server session,
    restarting deew-web still logs everyone out
  - needs deew 5.0.0 or newer

# deew 3.6.5:
- logo 1 uses a thin border
- the encoding summary shows the bit depths of mixed inputs (e.g. `16/24`)
  instead of `mixed bit depth`
- finished progress bars show `100%` instead of `100.00%`
- the progress bars are drawn in color in the terminal also when the GUI
  output is used in `deew-gui` and `deew-web` (switching to the CLI output
  showed them without color)
- updated dependencies
- `deew-gui`/`deew-web` 0.4.2:
  - GUI output also works for runs started with the CLI output (switching
    to it during or after such a run showed nothing)
  - GUI output keeps the previous runs like the CLI output does, scroll up to
    see them
  - narrow screens (e.g. phones): the progress bars show the status next to
    the file name and the percentage next to the bar
- `deew-gui`/`deew-web` 0.4.1:
  - the copy button copies the command in browsers opened over plain `http`
    (e.g. on a phone), and flashes red if copying fails
  - the input file summary shows the bit depths of mixed inputs
    (e.g. `16/24/32-bit`) instead of `mixed bit depth`
  - in GUI output finished progress bars show `100%` instead of `100.00%`
  - updated dependencies

# deew 3.6.4:
- `deew-gui`/`deew-web` 0.4.0:
  - new look: boxy style, blue accents and IBM Plex Sans font
  - the terminal uses IBM Plex Mono
  - `-` instead of `·` between the values in the input file summary and in
    the progress summary

# deew 3.6.3:
- `deew-gui`/`deew-web` 0.3.1:
  - the command line field is at most 3 lines high, a longer command ends
    in `...` (hover it for the full command), Run and Stop are as tall as
	the field and the copy button is centered
  - GUI output: no separate "CLI output" section anymore, the `CLI | GUI`
    toggle switches between the two and also works while a job runs
  - in GUI output the Utilities commands show their output in the terminal

# deew 3.6.2:
- reports progress, warnings and prompts to `deew-gui` and `deew-web` for their
  new GUI output (the terminal output doesn't change)
- default config: new `[gui]` section for `deew-gui` and `deew-web`
- `deew-gui`/`deew-web` 0.3.0:
  - new GUI output next to the CLI output, switched with the `CLI | GUI`
    toggle next to Run/Stop: progress bars per file, warnings and errors listed
	under them, prompts as pop-ups, the CLI output stays available below
  - the choice is saved in the new `[gui]` section of the config
    (`progress = 'cli'` or `'native'`, default `'cli'`)

# deew 3.6.1:
- fixed colors in the classic Windows console (`cmd.exe` outside
  Windows Terminal), escape codes were printed as text

# deew 3.6.0:
- new logo 1, the other logos moved one number up
- the encoding summary is at most 60 columns wide, a long temp path wraps to
  two lines and ends in `...` if it is even longer
- updated icons
  - `deew.exe` uses a sharper icon (every resolution is generated from svg)
  - Linux uses 512px icon instead of 32px
- updated dependencies
- `deew-gui` 0.2.0: the window title shows the version (`deew-gui 0.2.0`)
- `deew-web` 0.2.0: the browser tab title shows the version (`deew-web 0.2.0`)

# deew 3.5.1:
- WSL with `dee.exe`: if the deew binary is on a Windows drive (`/mnt/c` etc.),
  the default temp directory is `temp` next to it instead of
  the Windows temp directory
- updated the `temp_path` description in the default config

# deew 3.5.0:
- rewritten in Rust, single binary with no Python runtime needed
- the old Python version is available at https://github.com/pcrlnd/deew-python
- config is only read from `config.toml` next to the deew binary,
  `-gc` no longer asks for a location
- encoding summary: `Input(s)` header, new `Directory` row in Output, clearer
  Delay row (`0ms`, `Nms`, `Nms (parsed from filenames)`, 
  `mixed (parsed from filenames)`)
- inputs with different bit depths are supported (`mixed bit depth`)
- clearer `-np` description
- new optional desktop GUI (`deew-gui`) and web UI (`deew-web`),
  see the README
- config: new optional `[deew_web]` section for the web UI login

# deew 3.2.2:
- remove unnecessary thing for measure only
- try to fix temp delete

# deew 3.2.1:
- handle `-master` in dee's version string

# deew 3.2.0:
- add Windows 7 support with custom Python build:
  https://github.com/adang1345/PythonWin7

# deew 3.1.3:
- fix #37: Only show basename for already existing intermediate files

# deew 3.1.2:
- fix `&` in filename by bpoxy

# deew 3.1.1:
- use custom PyInstaller for less virus flagging

# deew 3.1.0:
- update `pyproject.toml` and `poetry.lock`

# deew 3.0.1:
- added ac4 to format error message
- fixed `allowed_bitrates` in `ac4_20`

# deew 3.0.0:
- added AC4 immersive stereo support by MartinEesmaa
  - add `dee_audio_filter_ac4_ims.dll` and `Object_0000.exe` from the AC4
    zip to your DEE installation path and add `ac4_2_0 = 320` under the
    `[default_bitrates]` section in your config file.
- mono input for TrueHD encoding error handling
- FLAC recommendation prompt for lossless mono/stereo encoding

# deew 2.9.5:
- remove `-map 0:a:index` from `ffmpeg_args` if `-filter_complex` is present
  somewhere in the command, so new ffmpeg versions can be used

# deew 2.9.4:
- allow new python version to be used

# deew 2.9.3:
- fixed spaces in paths
  (`quote` parameter was not working in `wpc()` if `is_wsl` was false)

# deew 2.9.2:
- added `-ti`/`--track-index`
- strip delay from output filenames

# deew 2.8.5:
- fix spelling in override everywhere
- use custom pyinstaller build to avoid anti-viruses going crazy

# deew 2.8.4:
- fixed unnecessary newlines in argparse

# deew 2.8.3:
- moved binary version parsing to another place to avoid error
  if `binaries` is set to false in config

# deew 2.8.2:
- removed `m`/`p` delay support and updated help to avoid confusion

# deew 2.8.1:
- fixed version in tool itself

# deew 2.8.0:
- added `summary_sections` to config
- parsing delay from filenames too

# deew 2.7.0:
- fixed first metavar removal for argparse's `--input`
- fixed exception printing in threads
- use a Windows path by default under WSL
- temp path added to the summary
- better instructions on config error

# deew 2.6.2:
- added additional instance clamping for Windows DEE (6),
  it seems like you can't do more than 6 even on a 16 thread cpu

# deew 2.6.1:
- fixed typo in help

# deew 2.6.0:
- changed default DRC to `music_light`, which is closer to the missing
  `none` preset: https://forum.doom9.org/showpost.php?p=1972689&postcount=136
- fixed DRC option not being applied for stereo TrueHD encodes
- colorized argparse's help a little bit
- renamed `-t`/`--threads` to `-in`/`--instances` to be more accurate,
  since one DEE instance can use 2 threads. see `-h` for more info

# deew 2.5.3:
- specified xml encoding as `utf-8`, this fixes errors with weird characters
- fixed xml's `os.remove`

# deew 2.5.2:
- revert clamping to `cpu_count() - 2` for Windows DEE

# deew 2.5.1:
- better thread clamping, the previous `cpu_count() - 2` has been changed to
  `cpu_count()` for Linux/macOS and `cpu_count() - 1` for Windows
  (for some reason you can only run 7 instances of DEE on a cpu with 8 threads)

# deew 2.5.0:
- added `-c`/`--changelog`: show config and config location(s)
- added `-gc`/`--generate-config`: generate a new config

# deew 2.4.1:
- fixed help of `--downmix`

# deew 2.4.0:
- updated `-cl`/`--changelog`
  - reversed order
  - only print last 10 elements
  - print `\` characters properly
- moved logo, help, bitrate and changelog printing to a more appropriate place

# deew 2.3.3:
- updated `wpc`'s path generaration (`C:` -> `C:\`)
  (drive paths without subfolders couldn't be used before)

# deew 2.3.2:
- fixed `simplens.pb`

# deew 2.3.1:
- fixed pip

# deew 2.3.0:
- turned into proper python package
- removed wsl option from config
  (autosetup with platform and binary type without additional libraries)

# deew 2.2.3:
- fixed file extension selection when `-fs`/`-fb` is used again

# deew 2.2.2:
- fixed file extension selection when `-fs`/`-fb` is used

# deew 2.2.1:
- fixed `help` of `--force-standard`

# deew 2.2.0:
- added `-fs`/`--force-standard` and `-fb`/`--force-bluray` options
  for DDP 7.1 encoding
    - `-fs` is forcing standard profile (384-1024 kbps)
    - `-fb` is forcing bluray profile (768-1664 kbps)
    - without these options deew will prefer standard profile and will pick
      that if the bitrate isn't higher than standard's max (1024kbps)
- added configurable default bitrates
- added `-lb`, `--list-bitrates`

# deew 2.1.6:
- refactor config code

# deew 2.1.5:
- fix config location search

# deew 2.1.4:
- readd local config support for standalone builds

# deew 2.1.3:
- more robust version parsing for summary based on word index

# deew 2.1.2:
- move `ffmpeg` and `ffprobe` parsing to `try except`

# deew 2.1.1:
- fix `ffprobe`'s name in summary

# deew 2.1.0:
- `title_style` for the summary to avoid padding being messed up in tmux
- added filename sanitizing for xml temp files (for some reason
  dee can't handle weird characters in the xml's name)
- added proper config location handling with ˙platformdirs˙
- autogenerate config if it's missing
- added build workflows for Github Actions
- added `DEE`, `ffmpeg`, `ffprobe` versions in the encoding summary
- implemented `DEE` version specific progress bar handling,
  this fixes the bug where the progress bar goes from 0% to 100% during
  the measuring step and stays there until the encoding step finishes
  (this happened when DEE version was 5.2.0 or higher)

# deew 2.0.3:
- proper error handling for missing keys in config

# deew 2.0.2:
- completely disabled xml validation when DEE's platform is not Windows

# deew 2.0.1:
- added `-np`/`--no-prompt`

# deew 2.0.0:
- colorized argparse
- replaced `multiprocessing` with `ThreadPoolExecutor`
    - multiple progress bars for batch encoding
    - dee's percentage remapping is removed (reverted to 1.2.8 behaviour)\
      because the TrueHD encoder works way differently
    - spinning animation for ffmpeg when length can't be parsed
    - removed `TimeRemainingColumn` because it can't be reset,\
      and it only showed the first step's remaining time
- proper error when `ffprobe` fails
- added `-d`/`--delay` option
    - supports ms, s and frame@fps
    - value has to start with `m`/`-` or `p`/`+`
    - fps can be int, float, division or `ntsc`/`pal`
    - examples: -5.1ms, +1,52s, p5s, m5@pal, +10@24000/1001
- added `mono` and `stereo` encoding with a warning prompt
- added `-dn`/`--dialnorm` option with a warning prompt
- added `-dm`/`--downmix` option
- rewrote bitrate and `encoder_mode` selection
- `bluray` `encoder_mode` (for bitrates > 1024) works on Linux/Mac now
- better encoding summary using `rich.table`
    - added `show_summary` option to config
    - shows input/output settings
    - shows if a new version is available
- added `threads` option to config
    - you can overwrite it with -t/--threads.
    - the threads number will be clamped between 1 and `cpu_count() - 2`
- dialnorm appears in the progress bar when the measuring step is done
- added `-mo`/`--measure-only` option
    - kills DEE when the dialnorm value gets written to the progress bar
    - overwrites format with `ddp` if specified
- `ffmpeg` and `DEE` arguments are printed just once with filename placeholders
  - files that already have an intermediate file will be listed
  - you can display the arguments for each file with actual filenames
    with `-la`/`--long-argument`
- standalone exe build
- icon for the project

# deew 1.2.11:
- fixed variable

# deew 1.2.10:
- switched `-af` to `-filter_complex` for resampling, so channel swapping\
  and sample rate conversion works at the same time

# deew 1.2.9:
- disabled `surround_90_degree_phase_shift`
- `DEE`'s measure and encoding step's progress has been remapped,\
  both step goes to 100% now

# deew 1.2.8:
- set `<surround_3db_attenuation>` to `false` in `thd.xml`

# deew 1.2.7:
- prior to this version deew swapped Ls Rs with Lrs Rrs, now it passes\
  `-filter_complex pan=7.1|c0=c0|c1=c1|c2=c2|c3=c3|c4=c6|c5=c7|c6=c4|c7=c5`\
  to 7.1 sources.
- moved ffmpeg args setup out of for loop

# deew 1.2.6:
- added `-cl`/`--changelog` option
- better argparse
- added types and improvements by nyuszika7h
- single input encodes shows a per encode progress bar that indicates both\
  `ffmpeg`'s temp creation and `DEE`'s measuring and encoding step
- fixed dd multithreading

# deew 1.2:
- better temp folder config:
    - empty: next to the script
    - relative path: from your current directory
    - you can also use fullpath
    - in any case will be created automatically if it doesn't exist already
- added check for intermediate file existence, now you can do:\
  `./deew.py -f dd -b 448 -i input -k`\
  `./deew.py -f dd -b 640 -i input -k`\
  `./deew.py -f ddp -i input`\
  and the intermediate file will only be created once (and removed after the\
  last encode)
- added automatic sample rate conversion with `ffmpeg`'s `soxr` resampler
    - for dd/ddp unsupported rates will be converted to 48000
    - for thd unsupported rates will be converted to 48000 if source sample\
      rate is lower than 72000, otherwise will be converted to 96000
    - command: `-af aresample=resampler=soxr -ar 48000/96000`\
      `-precision 28 -cutoff 1 -dither_scale 0`
- added alternative config path (`~/.config/deew/config.toml`) to Linux
- removed progress option, commands are printed at the same time as the\
  progress bar appears
- added drc option (`film_light`, `film_standard`, `music_light`,\
  `music_standard`, `speech`)
- you can now specify invalid bitrates and it will pick the closest\
  allowed one (`-f dd -b 635` -> 640k)
- added error for TrueHD encoding with Linux version of `DEE`
- all xml files are generated before encoding
- added back logos and logo config
- fixed intermediate file's bit depth to 32 bit if there's resampling involved

# deew 1.1:
- added sample rate, channel count and bit depth parsing for better error\
  handling and reducing temp file sizes (needs `ffprobe`)
    - spits out error for unsupported sample rates and channel counts
    - suggests using qaac or opus for mono and stereo
    - suggests using DMP or ffmpeg's `-ac 6`/`8` and `-af "pan=filter"` for\
      weird surround layouts
    - suggests using `sox` for sample rate conversion in case of unsupported\
      sample rates
    - in a run every input has to have the same channel count, sample rate\
      and bit depth (because of batch encoding)
    - grabs bits_per_sample value, if that doesn't exist or if it's 0 grabs\
      bits_per_raw_sample and if there's no bit depth 32 will be used
    - any bit depth that is not 16, 24 or 32 will be rounded up to the\
      nearest value
- changed channel option to mix
    - only works with DDP
    - DD will be encoded as 5.1 (using `DEE`'s downmix in case of a 7.1 source)
    - up/downmixing is reported at start
- removed dialnorm option
    - automatic measurement is better than relying on the input's metadata
    - `DEE` always measures the dialnorm, so there's no speed gain of setting\
      it manually
    - `deew` with Linux binaries of `DEE` should work now
- removed logo configuration and option and changed the smallest logo to an\
  even smaller one
- changed config.toml to config.toml.example so it throws an error that it\
  should be renamed and edited (to avoid user confusion)
- added temp path existence check (to avoid user confusion)
- removed magic dependency and added a wsl option to the config instead
- added `-drc_scale 0` in the `ffmpeg` arg list just in case someone encodes\
  from a DD/DDP source for whatever reason
- small XML option tweaks by cnzqy1
