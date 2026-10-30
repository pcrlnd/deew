# Golden reference outputs

These files are reference outputs captured from the Python implementation in `python/` (deew 3.2.2). The `version.txt`, `help.txt`, `no_args.txt` and `err_*` files were later bumped by hand to `deew 3.6.2` (the Rust version number), so they no longer match the Python capture in that one line. They are the behavior the Rust rewrite should match.

Commands were run from `python/` with stdout redirected to a file (so rich emits no ANSI codes) and the environment `COLUMNS=250 TERM=dumb NO_COLOR=1` (so rich does not wrap lines). `XDG_CONFIG_HOME` pointed at an empty temp dir. Nothing was written to stderr in any case.

| File | Command | Exit code | Stream |
|------|---------|-----------|--------|
| `help.txt` | `uv run python -m deew -h` | 0 | stdout |
| `no_args.txt` | `uv run python -m deew` | 1 | stdout (prints help, then exits 1) |
| `version.txt` | `uv run python -m deew -v` | 0 | stdout |
| `list_bitrates.txt` | `uv run python -m deew -lb` | 0 | stdout |
| `print_logos.txt` | `uv run python -m deew -pl` | 0 | stdout |
| `err_unknown.txt` | `uv run python -m deew -x` | 2 | stdout (usage block, then `deew: error: unrecognized arguments: -x`) |
| `err_invalid_int.txt` | `uv run python -m deew -b abc` | 2 | stdout |
| `err_missing_value.txt` | `uv run python -m deew -b` | 2 | stdout |
| `err_negative_value.txt` | `uv run python -m deew -d -5ms` | 2 | stdout |
| `err_positional.txt` | `uv run python -m deew foo` | 2 | stdout |

The five `err_*` files were captured with Python 3.12 (`uv run --python 3.12`), the version the released PyInstaller builds used. The same commands behave differently on Python 3.13+/3.14, where argparse treats any `-<digit>...` token as a negative number: `-d -5ms` is then accepted as a value instead of failing with `expected one argument`. `help.txt` etc. are identical on 3.12 and 3.14.

Full invocation example:

    cd python && COLUMNS=250 TERM=dumb NO_COLOR=1 uv run python -m deew -h > ../tests/golden/help.txt

The `-np, --no-prompt` description in `help.txt` and `no_args.txt` was intentionally changed from the Python output (now "skip confirmation questions, print them as notes and continue").

The `-c, --config` description in `help.txt` and `no_args.txt` was intentionally changed from the Python output ("show config and config location"): the Rust version only supports one config location, `config.toml` next to the executable.
