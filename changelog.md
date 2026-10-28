# deew 3.6.0:
- new logo 1, the other logos moved one number up
- the encoding summary is at most 60 columns wide, a long temp path wraps to
  two lines and ends in `...` if it is even longer
- updated icons
  - `deew.exe` uses a sharper icon (every resolution is generated from svg)
  - Linux uses 512px icon instead of 32px
- updated dependencies
- `deew-gui` 0.2.0:
  - the window title shows the version (`deew-gui 0.2.0`)
- `deew-web` 0.2.0:
  - the browser tab title shows the version (`deew-web 0.2.0`)

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
