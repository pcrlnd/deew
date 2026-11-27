//! End-to-end tests of the encoding pipeline with fake DEE tools, truehdd and
//! dee_shim (shell scripts) and real ffmpeg/ffprobe (the tests are skipped
//! when those are not installed).

#![cfg(target_os = "linux")]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

const BIN: &str = env!("CARGO_BIN_EXE_deew");

/// Spawn, retrying while the freshly written binary is still held open by a
/// concurrently forked child (ETXTBSY).
fn spawn_retry(command: &mut Command) -> std::process::Child {
    for _ in 0..50 {
        match command.spawn() {
            Err(e) if e.kind() == std::io::ErrorKind::ExecutableFileBusy => {
                std::thread::sleep(std::time::Duration::from_millis(10))
            }
            result => return result.unwrap(),
        }
    }
    command.spawn().unwrap()
}

/// Hard link the binary (same filesystem as the temp dirs), else copy it via
/// a temporary name and rename it into place.
fn install_bin(dest: &Path) {
    if std::fs::hard_link(BIN, dest).is_err() {
        let tmp = dest.with_extension("tmp");
        std::fs::copy(BIN, &tmp).unwrap();
        std::fs::rename(&tmp, dest).unwrap();
    }
}

/// The fake DEE tools. The script behaves like the tool it is named after:
/// it writes its arguments to `<tool>.args` next to itself, prints lines in
/// the style of DEE 6.7 and writes the files the real tool would write.
/// Environment variables: `FAKE_FAIL_TOOL` (name of a tool that fails with
/// `FAKE_ERROR`; with `FAKE_STDOUT_FAIL` it prints plain lines to stdout instead), `FAKE_NO_OUTPUT`, `FAKE_WARN`, `FAKE_SLOW` (inputs with `slow` in
/// their name write a partial output and hang).
const FAKE_DEE: &str = r#"#!/bin/sh
dir=$(dirname "$0")
tool=$(basename "$0")
if [ "$1" = "--help" ]; then
  echo "Fake tool."
  echo "This tool belongs to the Dolby Encoding Engine version 6.7.0+fake"
  exit 0
fi
{ echo "--- $tool"; for a in "$@"; do printf '%s\n' "$a"; done; } >> "$dir/$tool.args"
input=""; informat=""; output=""; tempdir=""; omanifest=""; imanifest=""
while [ $# -gt 0 ]; do
  case "$1" in
    -i) input="$2"; shift ;;
    --input-format) informat="$2"; shift ;;
    -o) output="$2"; shift ;;
    --temp-dir) tempdir="$2"; shift ;;
    --output-manifest) omanifest="$2"; shift ;;
    --input-manifest) imanifest="$2"; shift ;;
  esac
  shift
done
if [ -n "$FAKE_STDOUT_FAIL" ] && [ "$FAKE_FAIL_TOOL" = "$tool" ]; then
  echo "Unknown filter (encode_to_dthd)."
  echo "Could not build workflow."
  exit 1
fi
fail() {
  echo "[2026-11-01 22:11:54.300] ERROR: $1"
  echo
  echo "---- Summary of errors and warnings ----"
  echo "ERROR: $1"
  echo "----------------------------------------"
  echo "Exit code: 1"
  exit 1
}
if [ "$input" = /dev/stdin ]; then
  # the piped audio: count what ffmpeg sends, a failing ffmpeg sends nothing
  bytes=$(wc -c)
  echo "$bytes" > "$dir/$tool.stdin"
  if [ "$bytes" -eq 0 ]; then fail "The input is too short."; fi
elif [ "$input" = /dev/stdin ]; then
  # a failing tool does not read its input, ffmpeg then finds the pipe closed
  if [ "$FAKE_FAIL_TOOL" = "$tool" ]; then fail "$FAKE_ERROR"; fi
  # the piped audio: count what ffmpeg sends, a failing ffmpeg sends nothing
  # (`FAKE_NO_READ`: leave it to the pipe)
  if [ -z "$FAKE_NO_READ" ]; then
    bytes=$(wc -c)
    echo "$bytes" > "$dir/$tool.stdin"
    if [ "$bytes" -eq 0 ]; then fail "The input is too short."; fi
  fi
elif [ ! -f "$input" ]; then fail "File \"$input\" does not exist."; fi
if [ "$FAKE_FAIL_TOOL" = "$tool" ]; then fail "$FAKE_ERROR"; fi
if [ -n "$FAKE_WARN" ]; then
  echo "[2026-11-01 22:11:54.300] WARNING: $FAKE_WARN"
  echo "[2026-11-01 22:11:54.300] WARNING: $FAKE_WARN"
fi
if [ "$tool" = dee_measure_loudness ] && [ "$informat" = atmos_mezz ]; then
  # an Atmos master: the binaural block first, then the multichannel one
  echo "[2026-11-01 22:03:14.420] Step 1/1 : Loudness Measurement"
  printf '[>          ] 0 %%\r[==========>] 100 %%\r\n'
  for kind in binaural multichannel; do
    loud=-30.0; itu=-21.9; range=9.8; peak=-8.9
    if [ "$kind" = multichannel ]; then loud=${FAKE_LOUDNESS:--23.4}; itu=-24.3; range=5.6; peak=-15.3; fi
    echo "[2026-11-01 22:03:14.570] Atmos loudness type: $kind"
    echo "[2026-11-01 22:03:14.570] [Source loudness] measured_loudness=$loud"
    echo "[2026-11-01 22:03:14.570] [Source loudness] gating=dialogue"
    echo "[2026-11-01 22:03:14.570] [Source loudness] speech_percentage=16.1"
    echo "[2026-11-01 22:03:14.570] [Source loudness] dialogue_loudness=$loud"
    echo "[2026-11-01 22:03:14.570] [Source loudness] itu_loudness=$itu"
    echo "[2026-11-01 22:03:14.570] [Source loudness] loudness_range=$range"
    echo "[2026-11-01 22:03:14.570] [Source loudness] sample_peak=-15.4"
    echo "[2026-11-01 22:03:14.570] [Source loudness] true_peak=$peak"
  done
  echo "Exit code: 0"
  exit 0
fi
if [ "$tool" = dee_measure_loudness ]; then
  echo "[2026-11-01 22:03:14.420] Step 1/1 : Loudness Measurement"
  printf '[>          ] 0 %%\r[=====>     ] 50 %%\r[==========>] 100 %%\r\n'
  echo "[2026-11-01 22:03:14.570] [Source loudness] measured_loudness=${FAKE_LOUDNESS:--23.4}"
  echo "[2026-11-01 22:03:14.570] [Source loudness] gating=level"
  echo "[2026-11-01 22:03:14.570] [Source loudness] speech_percentage=measurement_disabled"
  cat > "$omanifest" <<'EOF'
{
  "loudness_manifest": {
    "version": "2.0.0",
    "tracks": [
      {
        "id": "x",
        "source_sample_rate": 48000,
        "channel_based_loudness": {
          "multichannel_5_1": {
            "integrated_loudness": -23.4,
            "integrated_speech_gated_loudness": -23.4,
            "true_peaks": [-1.0]
          }
        }
      }
    ]
  }
}
EOF
  exit 0
fi
if [ -n "$imanifest" ]; then
  dn=$(sed -n 's/.*"integrated_loudness": *\(-[0-9.]*\).*/\1/p' "$imanifest" | head -1)
  echo "[2026-11-01 22:01:24.690] [Source loudness] measured_loudness=$dn"
  echo "[2026-11-01 22:01:24.690] Step 1/1 : Encoding"
  printf '[>          ] 0 %%\r[=====>     ] 50 %%\r[==========>] 100 %%\r\n'
elif [ "$tool" = dee_aac_encoder ] || [ "$tool" = dee_dthd_encoder ]; then
  echo "[2026-11-01 22:01:24.690] Step 1/1 : Encoding"
  printf '[>          ] 0 %%\r[=====>     ] 50 %%\r[==========>] 100 %%\r\n'
else
  echo "[2026-11-01 22:01:24.690] Step 1/2 : Loudness Measurement"
  printf '[>          ] 0 %%\r[=====>     ] 12 %%\r[==========>] 50 %%\r\n'
  echo "[2026-11-01 22:01:24.838] [Source loudness] measured_loudness=${FAKE_LOUDNESS:--23.4}"
  echo "[2026-11-01 22:01:24.838] [Source loudness] gating=dialogue"
  echo "[2026-11-01 22:01:24.838] [Source loudness] speech_percentage=16.1"
  echo "[2026-11-01 22:01:24.900] Step 2/2 : Encoding"
  printf '[==========>] 50 %%\r[=======>    ] 75 %%\r[==========>] 100 %%\r\n'
fi
if [ -n "$omanifest" ]; then echo '{}' > "$omanifest"; fi
if [ "$tool" = dee_dthd_encoder ]; then echo log > "$output.log.html"; echo mll > "$output.mll"; fi
if [ -n "$tempdir" ]; then echo x > "$tempdir/dee-internal"; fi
if [ -n "$FAKE_SLOW" ]; then
  case "$input" in
    *slow*|/dev/stdin) printf partial > "$output"; sleep 30 ;;
  esac
fi
if [ -z "$FAKE_NO_OUTPUT" ]; then printf 'fake %s' "$tool" > "$output"; fi
echo "Exit code: 0"
"#;

/// A fake truehdd: `--version`, and `decode` writes the three `.atmos` files.
/// `FAKE_SHORT` makes it report far less audio than the input has, `FAKE_NO_ATMOS` leaves the Atmos presentation out of the result,
/// `FAKE_FAIL_TOOL=truehdd` makes it fail.
const FAKE_TRUEHDD: &str = r#"#!/bin/sh
dir=$(dirname "$0")
if [ "$1" = "--version" ]; then
  echo "truehdd 0.6.3 (truehd library 0.7.3) Built: fake"
  exit 0
fi
{ echo "---"; for a in "$@"; do printf '%s\n' "$a"; done; } >> "$dir/truehdd.args"
prefix=""; last=""
while [ $# -gt 0 ]; do
  case "$1" in --output-path) prefix="$2"; shift ;; esac
  last="$1"
  shift
done
# the input is stdin: take everything ffmpeg sends
if [ "$last" = "-" ]; then bytes=$(wc -c); echo "$bytes" > "$dir/truehdd.stdin"; fi
if [ "$FAKE_FAIL_TOOL" = truehdd ]; then
  echo '{"lvl":"ERROR","msg":"bad stream"}' >&2
  exit 4
fi
echo '{"lvl":"INFO","msg":"Found 100 extractable frames in 0.1s"}' >&2
echo '{"lvl":"INFO","msg":"Creating audio file: x (1 bed channel + 11 objects)"}' >&2
samples=96000
if [ -n "$FAKE_SHORT" ]; then samples=8600; fi
if [ -n "$FAKE_NO_ATMOS" ]; then
  printf pcm > "$prefix.caf"
else
  printf damf > "$prefix.atmos"
  printf meta > "$prefix.atmos.metadata"
  printf audio > "$prefix.atmos.audio"
fi
head='{"version":"0.6.2","frames":100,"samples":'$samples',"sampleRate":48000,'
if [ -n "$FAKE_NO_ATMOS" ]; then
  echo "$head"'"presentations":[]}'
else
  echo "$head"'"presentations":[{"index":3,"format":"damf","channels":12}]}'
fi
"#;

/// A fake harletty: `taxonomy` prints the version (`FAKE_HARLETTY_VERSION`),
/// `info` reports what `FAKE_SPATIAL` says (`dtsx` by default, `plain` for a
/// `--codec wav` input, `auro`,
/// `plain`, `nocodec`, or `error`, which fails), and `decode` writes the
/// three `.atmos` files, the audio a CAF of `FAKE_SAMPLES` stereo frames at
/// 48 kHz (96000 by default). `FAKE_WARN` adds a WARN record to the log,
/// `FAKE_FAIL_TOOL=harletty` makes `decode` fail.
const FAKE_HARLETTY: &str = r#"#!/bin/sh
dir=$(dirname "$0")
cmd=""
skip=""
for a in "$@"; do
  if [ -n "$skip" ]; then skip=""; continue; fi
  case "$a" in
    --loglevel|--log-format|--codec) skip=1 ;;
    taxonomy|info|decode) cmd="$a"; break ;;
  esac
done
log() { { echo "---"; for a in "$@"; do printf '%s\n' "$a"; done; } >> "$dir/harletty.$cmd.args"; }
wav=""; prev=""; codec="DTS-HD MA"
for a in "$@"; do if [ "$prev" = "--codec" ] && [ "$a" = wav ]; then wav=1; fi; prev="$a"; done
if [ -n "$wav" ]; then codec=PCM; fi
if [ "$cmd" = taxonomy ]; then
  echo '{"schema":1,"harletty":"'"${FAKE_HARLETTY_VERSION:-0.9.0}"'","build":"fake","taxonomy":4}'
  exit 0
fi
log "$@"
# a harletty that can't read PCM: a clap usage error
if [ -n "$wav" ] && [ -n "$FAKE_REJECT_WAV" ]; then
  echo "error: invalid value 'wav' for '--codec <CODEC>'" >&2
  exit 2
fi
prefix=""; last=""
while [ $# -gt 0 ]; do
  case "$1" in --output-path) prefix="$2"; shift ;; esac
  last="$1"
  shift
done
if [ "$cmd" = info ]; then
  # harletty stops reading early, ffmpeg gets a broken pipe
  if [ "$last" = "-" ]; then head -c 1000 > /dev/null; fi
  head='{"schema":1,"harletty":"0.9.0","build":"fake","taxonomy":4,'
  # plain PCM has no Auro-3D unless a test says so
  spatial="${FAKE_SPATIAL:-dtsx}"
  if [ -z "$FAKE_SPATIAL" ] && [ -n "$wav" ]; then spatial=plain; fi
  case "$spatial" in
    dtsx) echo "$head"'"codec":"DTS-HD MA","channels":8,"sample_rate":48000,"spatial":{"label":"DTS:X-7.1.4+3","kind":"dtsx","objects":3,"fixed":4,"experimental":true,"presentation":"ObjectsD0"},"frames_seen":1,"seconds_seen":0.01}' ;;
    auro) echo "$head"'"codec":"'"$codec"'","channels":8,"sample_rate":48000,"spatial":{"label":"Auro-3D-13.1","kind":"auro","objects":null,"fixed":null,"experimental":false},"frames_seen":1,"seconds_seen":0.01}' ;;
    plain) echo "$head"'"codec":"DTS","channels":6,"sample_rate":48000,"spatial":null,"frames_seen":1,"seconds_seen":1.0}' ;;
    nocodec) echo "$head"'"codec":null,"error":"no DTS frame found","channels":null,"sample_rate":null,"spatial":null,"frames_seen":0,"seconds_seen":0.0}' ;;
    error) echo "Error: Failed to open input: boom" >&2; exit 1 ;;
  esac
  exit 0
fi
# decode: the input is stdin, take everything ffmpeg sends
if [ "$last" = "-" ]; then bytes=$(wc -c); echo "$bytes" > "$dir/harletty.stdin"; fi
echo '{"ts":2026-11-13T16:55:49Z,"lvl":"INFO","msg":"Decoding DTS stream: x (strict mode: false)"}' >&2
if [ -n "$FAKE_WARN" ]; then
  echo '{"ts":2026-11-13T16:55:49Z,"lvl":"WARN","msg":"DTS core decode error (dropping frame): Err(x)"}' >&2
  echo '{"ts":2026-11-13T16:55:49Z,"lvl":"WARN","msg":"DTS:X ObjectsD0 is an experimental presentation: its feed identities rest on corpus evidence"}' >&2
fi
if [ "$FAKE_FAIL_TOOL" = harletty ]; then
  echo '{"ts":2026-11-13T16:55:49Z,"lvl":"ERROR","msg":"bad stream"}' >&2
  echo "Error: bad stream" >&2
  exit 1
fi
printf damf > "$prefix.atmos"
printf meta > "$prefix.atmos.metadata"
{
  printf 'caff\000\001\000\000'
  printf 'desc\000\000\000\000\000\000\000\040\100\347\160\000\000\000\000\000lpcm\000\000\000\004\000\000\000\006\000\000\000\001\000\000\000\002\000\000\000\030'
  printf 'data\377\377\377\377\377\377\377\377\000\000\000\000'
  head -c $(( ${FAKE_SAMPLES:-96000} * 6 )) /dev/zero
} > "$prefix.atmos.audio"
"#;

/// A fake dee_shim: runs the binary after `--` with the arguments after it
/// and glues a line of its own in front of the first line of DEE.
const FAKE_DSHIM: &str = r#"#!/bin/sh
dir=$(dirname "$0")
if [ "$1" = "--version" ]; then
  echo "dee_shim v0.4.0+g790d050"
  exit 0
fi
{ echo "---"; for a in "$@"; do printf '%s\n' "$a"; done; } >> "$dir/dee_shim.args"
binary=""
while [ $# -gt 0 ]; do
  case "$1" in
    --binary) binary="$2"; shift ;;
    --) shift; break ;;
  esac
  shift
done
printf 'dshim ok   fake patch'
exec "$binary" "$@"
"#;

/// ffprobe as deew sees it: the real one, but the TrueHD streams get the
/// Atmos profile (ffmpeg can't encode Atmos). `FAKE_NO_PROFILE` keeps the
/// real output. `FAKE_DTSX` gives the DTS streams the DTS:X profile.
const FAKE_FFPROBE: &str = r#"#!/bin/sh
if [ -n "$FAKE_NO_PROFILE" ]; then exec ffprobe "$@"; fi
# keep the exit code and stderr of ffprobe
tmp=$(mktemp)
ffprobe "$@" > "$tmp" 2> "$tmp.err"
status=$?
cat "$tmp.err" >&2
dtsx=""
if [ -n "$FAKE_DTSX" ]; then dtsx='s/"profile": "DTS",/"profile": "DTS-HD MA + DTS:X",/'; fi
sed -e 's/"codec_name": "truehd",/"codec_name": "truehd", "profile": "Dolby TrueHD + Dolby Atmos",/' ${dtsx:+-e "$dtsx"} "$tmp"
rm -f "$tmp" "$tmp.err"
exit $status
"#;

const DEE_TOOLS: [&str; 8] = [
    "dee_ddp_encoder",
    "dee_ddpjoc_encoder",
    "dee_ac4_encoder",
    "dee_ac4ajoc_encoder",
    "dee_ac4ims_encoder",
    "dee_aac_encoder",
    "dee_dthd_encoder",
    "dee_measure_loudness",
];

fn write_script(path: &Path, content: &str) {
    fs::write(path, content).unwrap();
    fs::set_permissions(path, fs::Permissions::from_mode(0o755)).unwrap();
}

/// A command that does not inherit cargo's `LD_LIBRARY_PATH`, which makes
/// every start of ffmpeg search the (slow) build directories first.
fn command(program: impl AsRef<std::ffi::OsStr>) -> Command {
    let mut command = Command::new(program);
    command.env_remove("LD_LIBRARY_PATH");
    command
}

fn tool_available(name: &str) -> bool {
    command(name)
        .arg("-version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// A test environment: fake tools, config, temp and output directories.
struct Env {
    dir: tempfile::TempDir,
}

impl Env {
    /// `None` (and a printed note) if ffmpeg or ffprobe are missing.
    fn new() -> Option<Env> {
        if !tool_available("ffmpeg") || !tool_available("ffprobe") {
            eprintln!("skipping: ffmpeg/ffprobe not found in PATH");
            return None;
        }
        let env = Env {
            dir: tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap(),
        };
        for dir in ["bin/dee", "shim", "empty", "xdg", "work"] {
            fs::create_dir_all(env.path(dir)).unwrap();
        }
        // one script, linked under the names of all the tools
        let first = env.dee().join(DEE_TOOLS[0]);
        write_script(&first, FAKE_DEE);
        for tool in &DEE_TOOLS[1..] {
            let link = env.dee().join(tool);
            if fs::hard_link(&first, &link).is_err() {
                write_script(&link, FAKE_DEE);
            }
        }
        // the tools are next to the private copy of the binary
        write_script(&env.path("bin/truehdd"), FAKE_TRUEHDD);
        write_script(&env.path("bin/harletty"), FAKE_HARLETTY);
        write_script(&env.path("bin/dee_shim"), FAKE_DSHIM);
        write_script(&env.path("bin/ffprobe"), FAKE_FFPROBE);
        // a private copy of the binary: the config lives next to it
        install_bin(&env.bin());
        env.write_config();
        Some(env)
    }

    /// Feed DEE through the pipe (the config of the tests turns it off).
    fn set_pipe(&self, on: bool) {
        let text = fs::read_to_string(self.config()).unwrap();
        fs::write(
            self.config(),
            text.replace("pipe = false", &format!("pipe = {on}"))
                .replace("pipe = true", &format!("pipe = {on}")),
        )
        .unwrap();
    }

    fn write_config(&self) {
        fs::write(
            self.config(),
            format!(
                "temp_path = '{}'\nlogo = 0\nmax_instances = 2\npipe = false\n\
                 [default_bitrates]\ndd_1_0 = 128\ndd_2_0 = 256\ndd_5_1 = 640\n\
                 ddp_1_0 = 128\nddp_2_0 = 256\nddp_5_1 = 1024\nddp_7_1_standard = 1536\nddp_7_1_bluray = 1536\n\
                 ddp_atmos_streaming = 1024\nddp_atmos_standard = 1536\nddp_atmos_bluray = 1536\nac4_2_0 = 256\nac4_5_1 = 448\nac4_atmos = 448\n\
                 aac_2_0 = 256\n\
                 [summary_sections]\ndeew_info = false\nbinaries = true\ninput_info = true\n\
                 output_info = true\nother = true\n",
                self.temp().display()
            ),
        )
        .unwrap();
    }

    /// The private copy of the binary.
    fn bin(&self) -> PathBuf {
        self.path("bin/deew")
    }

    /// The config file next to the private copy of the binary.
    fn config(&self) -> PathBuf {
        self.path("bin/config.toml")
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    /// The `dee` folder of the fake DEE tools, next to the binary.
    fn dee(&self) -> PathBuf {
        self.path("bin/dee")
    }

    /// Make dee_shim (or the DEE tools) invisible to deew, or visible again.
    fn set_shim(&self, present: bool) {
        let (from, to) = if present {
            ("shim", "bin")
        } else {
            ("bin", "shim")
        };
        fs::rename(
            self.path(from).join("dee_shim"),
            self.path(to).join("dee_shim"),
        )
        .unwrap();
    }

    fn hide_dee_tools(&self) {
        fs::rename(self.dee(), self.path("dee-hidden")).unwrap();
    }

    fn temp(&self) -> PathBuf {
        self.path("temp")
    }

    fn out(&self) -> PathBuf {
        self.path("out")
    }

    fn work(&self) -> PathBuf {
        self.path("work")
    }

    /// Generate a 2 s silent file; `layout` is an ffmpeg channel layout.
    fn media(&self, name: &str, layout: &str, rate: u32, codec: &str) -> PathBuf {
        let path = self.work().join(name);
        let status = command("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
            .arg(format!(
                "anullsrc=channel_layout={layout}:sample_rate={rate}"
            ))
            .args(["-t", "2", "-c:a", codec])
            .args(["-strict", "-2"])
            .arg(&path)
            .status()
            .unwrap();
        assert!(status.success(), "ffmpeg failed to generate {name}");
        path
    }

    /// A 5.1 Matroska audio file with language and title tags on its track.
    fn tagged(&self, name: &str, lang: &str, title: &str) -> PathBuf {
        let path = self.work().join(name);
        let status = command("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
            .arg("anullsrc=channel_layout=5.1:sample_rate=48000")
            .args(["-t", "2", "-c:a", "pcm_s24le"])
            .args(["-metadata:s:a:0", &format!("language={lang}")])
            .args(["-metadata:s:a:0", &format!("title={title}")])
            .arg(&path)
            .status()
            .unwrap();
        assert!(status.success(), "ffmpeg failed to generate {name}");
        path
    }

    /// A raw TrueHD file, the input of the Atmos formats.
    fn truehd(&self, name: &str) -> PathBuf {
        self.media(name, "7.1", 48000, "truehd")
    }

    /// A Matroska audio file with the raw TrueHD file in it, preceded by an
    /// AC-3 track if `with_ac3`.
    fn mka(&self, name: &str, with_ac3: bool) -> PathBuf {
        let raw = self.truehd("raw.thd");
        let path = self.work().join(name);
        let mut ffmpeg = command("ffmpeg");
        ffmpeg.args(["-v", "error", "-y"]);
        if with_ac3 {
            ffmpeg.args([
                "-f",
                "lavfi",
                "-i",
                "anullsrc=channel_layout=5.1:sample_rate=48000",
            ]);
        }
        ffmpeg.arg("-i").arg(&raw);
        if with_ac3 {
            ffmpeg.args([
                "-t", "2", "-map", "0:a", "-map", "1:a", "-c:0", "ac3", "-c:1", "copy",
            ]);
        } else {
            ffmpeg.args(["-c", "copy"]);
        }
        assert!(ffmpeg.arg(&path).status().unwrap().success());
        path
    }

    fn deew_with(&self, args: &[&str], envs: &[(&str, &str)]) -> Output {
        spawn_retry(
            command(self.bin())
                .args(args)
                .current_dir(self.work())
                .env("XDG_CONFIG_HOME", self.path("xdg"))
                .env_remove("NO_COLOR")
                .envs(envs.iter().copied())
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped()),
        )
        .wait_with_output()
        .unwrap()
    }

    /// Run `deew -np -o <out> <args>`.
    fn deew(&self, args: &[&str]) -> Output {
        self.deew_env(args, &[])
    }

    fn deew_env(&self, args: &[&str], envs: &[(&str, &str)]) -> Output {
        let out = self.out();
        let mut all = vec!["-np", "-o", out.to_str().unwrap()];
        all.extend_from_slice(args);
        self.deew_with(&all, envs)
    }

    /// The arguments of the last run of a fake tool (`None` if it never ran).
    fn last_run(&self, tool: &str) -> Option<Vec<String>> {
        let text = fs::read_to_string(self.dee().join(format!("{tool}.args"))).ok()?;
        let last = text.split("--- ").last()?;
        // the first line is the tool name
        Some(last.lines().skip(1).map(str::to_string).collect())
    }

    fn run(&self, tool: &str) -> Vec<String> {
        self.last_run(tool)
            .unwrap_or_else(|| panic!("{tool} was not run"))
    }

    /// The arguments of truehdd or dee_shim (its file has `---` separators).
    fn run_of(&self, file: &str, dir: &str) -> Option<Vec<String>> {
        let text = fs::read_to_string(self.path(dir).join(file)).ok()?;
        let last = text.split("---\n").last()?;
        Some(last.lines().map(str::to_string).collect())
    }

    fn truehdd_run(&self) -> Option<Vec<String>> {
        self.run_of("truehdd.args", "bin")
    }

    /// The arguments of the last `harletty decode` or `harletty info`.
    fn harletty_run(&self, command: &str) -> Option<Vec<String>> {
        self.run_of(&format!("harletty.{command}.args"), "bin")
    }

    /// A raw DTS file (DTS core, which is what ffmpeg can write).
    fn dts(&self, name: &str) -> PathBuf {
        self.media(name, "5.1", 48000, "dca")
    }

    /// A Matroska audio file with a DTS track.
    fn dts_mka(&self, name: &str) -> PathBuf {
        let raw = self.dts("raw.dts");
        let path = self.work().join(name);
        let status = command("ffmpeg")
            .args(["-v", "error", "-y", "-i"])
            .arg(&raw)
            .args(["-c", "copy"])
            .arg(&path)
            .status()
            .unwrap();
        assert!(status.success(), "ffmpeg failed to generate {name}");
        path
    }

    fn dshim_run(&self) -> Option<Vec<String>> {
        self.run_of("dee_shim.args", "bin")
    }
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn ok(o: &Output) {
    assert_eq!(o.status.code(), Some(0), "{}", stdout(o));
}

/// The value after the option `name`.
fn opt<'a>(args: &'a [String], name: &str) -> &'a str {
    let i = args
        .iter()
        .position(|a| a == name)
        .unwrap_or_else(|| panic!("no {name} in {args:?}"));
    &args[i + 1]
}

fn has(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

fn ffprobe_field(path: &Path, entry: &str) -> String {
    let out = command("ffprobe")
        .args(["-v", "error", "-select_streams", "a:0", "-show_entries"])
        .arg(format!("stream={entry}"))
        .args(["-of", "default=nw=1:nk=1"])
        .arg(path)
        .output()
        .unwrap();
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

/// The value of the summary table row `label` (first match).
fn summary_row(text: &str, label: &str) -> String {
    text.lines()
        .filter_map(|l| {
            let cells: Vec<&str> = l.trim_matches('│').split('│').map(str::trim).collect();
            (cells.len() == 2 && cells[0] == label).then(|| cells[1].to_string())
        })
        .next()
        .unwrap_or_else(|| panic!("no {label} row in\n{text}"))
}

/// The value of the last summary table row `label` (the Output section
/// comes after the Input section).
fn last_summary_row(text: &str, label: &str) -> String {
    text.lines()
        .filter_map(|l| {
            let cells: Vec<&str> = l.trim_matches('│').split('│').map(str::trim).collect();
            (cells.len() == 2 && cells[0] == label).then(|| cells[1].to_string())
        })
        .next_back()
        .unwrap_or_else(|| panic!("no {label} row in\n{text}"))
}

macro_rules! env_or_skip {
    () => {
        match Env::new() {
            Some(env) => env,
            None => return,
        }
    };
}

fn assert_error(out: &Output, message: &str) {
    assert_eq!(out.status.code(), Some(1), "{}", stdout(out));
    let text = stdout(out);
    // errors of the checks before the run, or the `failed:` line of a job
    assert!(
        text.contains(&format!("ERROR: {message}"))
            || text
                .lines()
                .any(|l| l.starts_with("failed: ") && l.contains(&format!(" - {message}"))),
        "{text}"
    );
    assert!(!text.contains("panicked"), "{text}");
}

/// DEE's per-job directory: `<temp>/dee-<pid>-<job>`.
fn assert_dee_temp(env: &Env, dir: &str) {
    let prefix = format!("{}/dee-", env.temp().display());
    let rest = dir.strip_prefix(&prefix).unwrap_or_else(|| panic!("{dir}"));
    let (pid, job) = rest.split_once('-').unwrap();
    assert!(
        pid.parse::<u32>().is_ok() && job.parse::<usize>().is_ok(),
        "{dir}"
    );
}

fn temp_is_empty(env: &Env) -> bool {
    fs::read_dir(env.temp()).is_ok_and(|mut d| d.next().is_none())
}

#[test]
fn ddp_51_default_and_cleanup() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    let out = env.deew(&["-i", "in.wav"]);
    ok(&out);
    let text = stdout(&out);
    assert!(text.contains("Running the following commands:"), "{text}");
    assert!(text.contains("1024 kbps"), "{text}");
    assert_eq!(summary_row(&text, "Format"), "DDP");
    assert_eq!(summary_row(&text, "Channels"), "5.1");
    assert_eq!(summary_row(&text, "DEE version"), "6.7.0", "{text}");
    assert_eq!(summary_row(&text, "truehdd version"), "0.6.3");
    assert_eq!(summary_row(&text, "dee_shim version"), "0.4.0+g790d050");
    assert!(!summary_row(&text, "ffmpeg version").is_empty());
    // the output directory is listed in the Output section, cropped to the
    // table's maximum width
    let dir_row = text
        .lines()
        .find(|l| l.contains("│ Directory"))
        .unwrap_or_else(|| panic!("no Directory row: {text}"));
    let shown = dir_row
        .split('│')
        .nth(2)
        .unwrap()
        .trim()
        .trim_end_matches('…');
    assert!(
        shown.len() > 10 && env.out().to_str().unwrap().starts_with(shown),
        "{dir_row}\n{text}"
    );
    // the fake tool wrote the output; the temp files are gone
    assert_eq!(
        fs::read(env.out().join("in.ec3")).unwrap(),
        b"fake dee_ddp_encoder"
    );
    assert!(temp_is_empty(&env));

    let args = env.run("dee_ddp_encoder");
    assert_eq!(args[..4], ["--progress", "1", "--overwrite", "1"]);
    assert_eq!(opt(&args, "--input-format"), "wav");
    assert_eq!(
        opt(&args, "-i"),
        env.temp().join("in.wav").to_str().unwrap()
    );
    assert_eq!(opt(&args, "-o"), env.out().join("in.ec3").to_str().unwrap());
    assert_eq!(opt(&args, "-e"), "ddp:drc_profile=none");
    assert_eq!(opt(&args, "--data-rate"), "1024");
    assert_eq!(opt(&args, "--loudness-management"), "measure_only");
    assert_dee_temp(&env, opt(&args, "--temp-dir"));
    // 5.1 stays 5.1, no dialnorm: no manifest, and the 256 sample shift
    assert!(!has(&args, "-c") && !has(&args, "--input-manifest"));
    assert_eq!(opt(&args, "--start"), "0.005333");
    assert!(env.last_run("dee_measure_loudness").is_none());
    assert!(env.truehdd_run().is_none());
}

#[test]
fn kept_files_with_k() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    ok(&env.deew(&["-i", "in.wav", "-k"]));
    // -k keeps the intermediate wav: 5.1, 24 bit, mapped as is
    let wav = env.temp().join("in.wav");
    assert_eq!(ffprobe_field(&wav, "channels"), "6");
    assert_eq!(ffprobe_field(&wav, "sample_rate"), "48000");
    assert_eq!(ffprobe_field(&wav, "bits_per_sample"), "24");
    assert!(env.out().join("in.ec3").exists());
}

#[test]
fn dd_stereo_rounds_the_bitrate() {
    let env = env_or_skip!();
    env.media("st.wav", "stereo", 48000, "pcm_s16le");
    let out = env.deew(&["-i", "st.wav", "-f", "DD", "-b", "200"]);
    ok(&out);
    assert!(stdout(&out).contains("Consider using qaac or opus for mono and stereo encoding."));
    let args = env.run("dee_ddp_encoder");
    assert_eq!(opt(&args, "--data-rate"), "192");
    assert_eq!(opt(&args, "-e"), "dd:drc_profile=none");
    assert!(env.out().join("st.ac3").exists());
}

#[test]
fn ddp_71_is_standard_through_dee_shim() {
    let env = env_or_skip!();
    env.media("sur.wav", "7.1", 48000, "pcm_s24le");
    let out = env.deew(&["-i", "sur.wav", "-b", "1792"]);
    ok(&out);
    let text = stdout(&out);
    assert_eq!(summary_row(&text, "Profile"), "Standard (dee_shim)");
    assert_eq!(summary_row(&text, "Bitrate"), "1792 kbps");
    let shim = env.dshim_run().unwrap();
    assert_eq!(shim[0], "ddp_encoder");
    assert_eq!(
        opt(&shim, "--binary"),
        env.dee().join("dee_ddp_encoder").to_str().unwrap()
    );
    assert_eq!(opt(&shim, "--layout"), "7.1/eac3");
    assert_eq!(opt(&shim, "--bitrate"), "1792");
    assert!(!has(&shim, "--drc"));
    let args = env.run("dee_ddp_encoder");
    let rest: Vec<String> = shim[shim.iter().position(|a| a == "--").unwrap() + 1..].to_vec();
    assert_eq!(rest, args);
    // the layout supplies the mode and the rate; DRC is DEE's own -e
    assert_eq!(opt(&args, "-e"), "ddp:drc_profile=none");
    assert!(!has(&args, "--data-rate") && !has(&args, "-c"));
    assert_eq!(opt(&args, "--loudness-management"), "measure_only");
    assert_dee_temp(&env, opt(&args, "--temp-dir"));
    assert!(env.out().join("sur.ec3").exists() && !env.out().join("sur.eb3").exists());

    // a rate only the standard profile has, rounded like the others
    ok(&env.deew(&["-i", "sur.wav", "-b", "1100"]));
    assert_eq!(opt(&env.dshim_run().unwrap(), "--bitrate"), "1152");
    // the default is valid for both
    ok(&env.deew(&["-i", "sur.wav"]));
    assert_eq!(opt(&env.dshim_run().unwrap(), "--bitrate"), "1536");
    // -dm 6 is a 5.1 encode, native, 5.1 rates
    ok(&env.deew(&["-i", "sur.wav", "-dm", "6", "-b", "600"]));
    let args = env.run("dee_ddp_encoder");
    assert_eq!(opt(&args, "-e"), "ddp:drc_profile=none");
    assert_eq!(opt(&args, "-c"), "5.1");
    assert_eq!(opt(&args, "--data-rate"), "576");
    assert!(env.out().join("sur.ec3").exists());
}

const PHASE_OFF: &str = ":surround_90deg_phase_shift=0";

#[test]
fn no_phase_shift_reaches_every_dd_and_ddp_encode() {
    let env = env_or_skip!();
    env.media("st.wav", "stereo", 48000, "pcm_s16le");
    env.media("fv.wav", "5.1", 48000, "pcm_s24le");
    env.media("sur.wav", "7.1", 48000, "pcm_s24le");
    let e = |env: &Env| opt(&env.run("dee_ddp_encoder"), "-e").to_string();
    // without the flag nothing is added
    ok(&env.deew(&["-i", "fv.wav", "-f", "dd"]));
    assert_eq!(e(&env), "dd:drc_profile=none");
    for flag in ["--surround-phase-shift=off", "--surround-phase-shift=OFF"] {
        // native: DD and DDP, stereo and 5.1, also with -dm and -dl
        ok(&env.deew(&["-i", "st.wav", "-f", "dd", flag]));
        assert_eq!(e(&env), format!("dd:drc_profile=none{PHASE_OFF}"));
        ok(&env.deew(&["-i", "fv.wav", "-f", "dd", flag]));
        assert_eq!(e(&env), format!("dd:drc_profile=none{PHASE_OFF}"));
        ok(&env.deew(&["-i", "fv.wav", flag]));
        assert_eq!(e(&env), format!("ddp:drc_profile=none{PHASE_OFF}"));
        ok(&env.deew(&["-i", "fv.wav", flag, "-dl"]));
        assert_eq!(e(&env), format!("ddp:drc_profile=none:lfe_on=0{PHASE_OFF}"));
        ok(&env.deew(&["-i", "fv.wav", flag, "-dm", "2"]));
        assert_eq!(e(&env), format!("ddp:drc_profile=none{PHASE_OFF}"));
        // 7.1 Blu-ray: native
        ok(&env.deew(&["-i", "sur.wav", flag, "--ddp-mode", "bluray"]));
        assert_eq!(e(&env), format!("bluray:drc_profile=none{PHASE_OFF}"));
        // the dee_shim layouts, the key is in the string after --
        for (extra, layout, mode) in [
            (&[][..], "7.1/eac3", "ddp"),
            (&["-dl"][..], "7.0/eac3", "ddp"),
            (&["-dl", "--ddp-mode", "bluray"][..], "7.0", "bluray"),
        ] {
            let mut a = vec!["-i", "sur.wav", flag];
            a.extend(extra);
            ok(&env.deew(&a));
            let shim = env.dshim_run().unwrap();
            assert_eq!(opt(&shim, "--layout"), layout);
            let rest = &shim[shim.iter().position(|a| a == "--").unwrap() + 1..];
            assert_eq!(
                opt(rest, "-e"),
                format!("{mode}:drc_profile=none{PHASE_OFF}")
            );
        }
    }
    // the shim layouts without the flag stay as they were
    ok(&env.deew(&["-i", "sur.wav"]));
    let shim = env.dshim_run().unwrap();
    let rest = &shim[shim.iter().position(|a| a == "--").unwrap() + 1..];
    assert_eq!(opt(rest, "-e"), "ddp:drc_profile=none");
}

#[test]
fn config_can_turn_the_phase_shift_off() {
    let env = env_or_skip!();
    env.media("fv.wav", "5.1", 48000, "pcm_s24le");
    env.media("sur.wav", "7.1", 48000, "pcm_s24le");
    let text = fs::read_to_string(env.config()).unwrap();
    fs::write(
        env.config(),
        format!("surround_phase_shift = false\n{text}"),
    )
    .unwrap();
    let e = |env: &Env| opt(&env.run("dee_ddp_encoder"), "-e").to_string();
    ok(&env.deew(&["-i", "fv.wav", "-f", "dd"]));
    assert_eq!(e(&env), format!("dd:drc_profile=none{PHASE_OFF}"));
    ok(&env.deew(&["-i", "sur.wav"]));
    let shim = env.dshim_run().unwrap();
    let rest = &shim[shim.iter().position(|a| a == "--").unwrap() + 1..];
    assert_eq!(opt(rest, "-e"), format!("ddp:drc_profile=none{PHASE_OFF}"));
    // other formats ignore it, without an error
    ok(&env.deew(&["-i", "fv.wav", "-f", "ac4"]));
    assert!(
        !env.run("dee_ac4_encoder")
            .iter()
            .any(|a| a.contains("surround_90deg"))
    );
    ok(&env.deew(&["-i", "fv.wav", "-f", "aac"]));
    // -mo too
    ok(&env.deew(&["-i", "fv.wav", "-mo"]));
    // the report lists it
    ok(&env.deew(&["-i", "fv.wav", "-kl"]));
    let log = fs::read_to_string(env.out().join("fv.ec3.deew-log.html"))
        .or_else(|_| fs::read_to_string(env.out().join("fv.deew-log.html")))
        .unwrap();
    assert!(log.contains("Surround phase shift"), "{log}");
    // true is the default
    let text = fs::read_to_string(env.config()).unwrap();
    fs::write(
        env.config(),
        text.replace(
            "surround_phase_shift = false",
            "surround_phase_shift = true",
        ),
    )
    .unwrap();
    ok(&env.deew(&["-i", "fv.wav", "-f", "dd"]));
    assert_eq!(e(&env), "dd:drc_profile=none");
}

#[test]
fn no_phase_shift_only_works_for_dd_ddp_and_ddp_atmos() {
    let env = env_or_skip!();
    env.media("fv.wav", "5.1", 48000, "pcm_s24le");
    for format in ["thd", "ac4", "aac", "thd-atmos", "ac4-atmos"] {
        let out = env.deew(&["-i", "fv.wav", "-f", format, "-sps", "off"]);
        assert_error(
            &out,
            "-sps/--surround-phase-shift only works for DD, DDP and DDP Atmos.",
        );
    }
    // -mo ignores the format rules
    ok(&env.deew(&["-i", "fv.wav", "-mo", "-f", "thd", "-sps", "off"]));
}

#[test]
fn sps_overrides_the_config_both_ways() {
    let env = env_or_skip!();
    env.media("fv.wav", "5.1", 48000, "pcm_s24le");
    let e = |env: &Env| opt(&env.run("dee_ddp_encoder"), "-e").to_string();
    let text = fs::read_to_string(env.config()).unwrap();
    fs::write(
        env.config(),
        format!("surround_phase_shift = false\n{text}"),
    )
    .unwrap();
    ok(&env.deew(&["-i", "fv.wav", "-f", "dd"]));
    assert_eq!(e(&env), format!("dd:drc_profile=none{PHASE_OFF}"));
    ok(&env.deew(&["-i", "fv.wav", "-f", "dd", "-sps", "on"]));
    assert_eq!(e(&env), "dd:drc_profile=none");
    ok(&env.deew(&["-i", "fv.wav", "-f", "dd", "-sps", "off"]));
    assert_eq!(e(&env), format!("dd:drc_profile=none{PHASE_OFF}"));
    let out = env.deew(&["-i", "fv.wav", "-sps", "maybe"]);
    assert_error(&out, "-sps, -lpf, -s3a and -bc can only be on or off.");
}

#[test]
fn lpf_and_s3a_reach_dd_and_ddp_encodes() {
    let env = env_or_skip!();
    env.media("st.wav", "stereo", 48000, "pcm_s16le");
    env.media("fv.wav", "5.1", 48000, "pcm_s24le");
    env.media("sur.wav", "7.1", 48000, "pcm_s24le");
    let e = |env: &Env| opt(&env.run("dee_ddp_encoder"), "-e").to_string();
    let lpf = ":lfe_filter=0";
    let s3a = ":surround_3dB_attenuation=1";
    // defaults and explicit defaults add nothing
    ok(&env.deew(&["-i", "fv.wav", "-f", "dd", "-lpf", "on", "-s3a", "off"]));
    assert_eq!(e(&env), "dd:drc_profile=none");
    ok(&env.deew(&["-i", "fv.wav", "-f", "dd", "-lpf", "off", "-s3a", "on"]));
    assert_eq!(e(&env), format!("dd:drc_profile=none{lpf}{s3a}"));
    ok(&env.deew(&["-i", "fv.wav", "--low-pass-filter", "OFF"]));
    assert_eq!(e(&env), format!("ddp:drc_profile=none{lpf}"));
    ok(&env.deew(&[
        "-i",
        "fv.wav",
        "--surround-3db-attenuation=on",
        "-dl",
        "-lpf",
        "off",
    ]));
    assert_eq!(e(&env), format!("ddp:drc_profile=none:lfe_on=0{s3a}"));
    // stereo: neither
    ok(&env.deew(&["-i", "st.wav", "-lpf", "off", "-s3a", "on"]));
    assert_eq!(e(&env), "ddp:drc_profile=none");
    // dee_shim layout
    ok(&env.deew(&["-i", "sur.wav", "-lpf", "off", "-s3a", "on"]));
    let shim = env.dshim_run().unwrap();
    let rest = &shim[shim.iter().position(|a| a == "--").unwrap() + 1..];
    assert_eq!(opt(rest, "-e"), format!("ddp:drc_profile=none{lpf}{s3a}"));
    // the config
    let text = fs::read_to_string(env.config()).unwrap();
    fs::write(
        env.config(),
        format!("low_pass_filter = false\nsurround_3db_attenuation = true\n{text}"),
    )
    .unwrap();
    ok(&env.deew(&["-i", "fv.wav", "-f", "dd"]));
    assert_eq!(e(&env), format!("dd:drc_profile=none{lpf}{s3a}"));
    ok(&env.deew(&["-i", "fv.wav", "-f", "dd", "-lpf", "on", "-s3a", "off"]));
    assert_eq!(e(&env), "dd:drc_profile=none");
    // other formats ignore the config
    ok(&env.deew(&["-i", "fv.wav", "-f", "ac4"]));
    ok(&env.deew(&["-i", "fv.wav", "-f", "aac"]));
}

#[test]
fn lpf_and_s3a_only_work_for_dd_and_ddp() {
    let env = env_or_skip!();
    env.media("fv.wav", "5.1", 48000, "pcm_s24le");
    for format in ["ddp-atmos", "thd", "ac4", "aac", "thd-atmos", "ac4-atmos"] {
        let out = env.deew(&["-i", "fv.wav", "-f", format, "-lpf", "off"]);
        assert_error(&out, "-lpf/--low-pass-filter only works for DD and DDP.");
        if format == "thd" {
            continue; // -s3a works for TrueHD
        }
        let out = env.deew(&["-i", "fv.wav", "-f", format, "-s3a", "on"]);
        assert_error(
            &out,
            "-s3a/--surround-3db-attenuation only works for DD, DDP and TrueHD.",
        );
    }
    ok(&env.deew(&[
        "-i", "fv.wav", "-mo", "-f", "thd", "-lpf", "off", "-s3a", "on",
    ]));
}

#[test]
fn thd_surround_3db_asks_first() {
    let env = env_or_skip!();
    env.media("s51.wav", "5.1", 48000, "pcm_s24le");
    let question = "won't be identical to the source. Continue?";
    // answering no aborts
    let out = env.deew_stdin(&["-i", "s51.wav", "-f", "thd", "-s3a", "on"], "n\n");
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    assert!(stdout(&out).contains(question), "{}", stdout(&out));
    assert!(!env.out().join("s51.thd").exists());
    // yes encodes, the option is in the presentation (through dee_shim: DRC none)
    let out = env.deew_stdin(&["-i", "s51.wav", "-f", "thd", "-s3a", "on"], "y\n");
    ok(&out);
    assert!(stdout(&out).contains(question));
    assert_eq!(
        opt(&env.run("dee_dthd_encoder"), "--presentation"),
        "6ch:drc_profile=film_light:surround_3dB_attenuation=1"
    );
    assert!(env.out().join("s51.thd").exists());
    // -np: a note, no question, native with a DRC profile
    let out = env.deew(&[
        "-i", "s51.wav", "-f", "thd", "-s3a", "on", "-np", "-r", "speech",
    ]);
    ok(&out);
    let text = stdout(&out);
    assert!(
        text.contains("the output won't be identical to the source."),
        "{text}"
    );
    assert!(!text.contains(question), "{text}");
    assert_eq!(
        opt(&env.run("dee_dthd_encoder"), "--presentation"),
        "6ch:drc_profile=speech:surround_3dB_attenuation=1"
    );
    // stereo: skipped without a question
    env.media("st.wav", "stereo", 48000, "pcm_s24le");
    let out = env.deew(&["-i", "st.wav", "-f", "thd", "-s3a", "on", "-np"]);
    ok(&out);
    assert!(!stdout(&out).contains("won't be identical"));
    assert_eq!(
        opt(&env.run("dee_dthd_encoder"), "--presentation"),
        "2ch:drc_profile=film_light"
    );
}

#[test]
fn ddp_mode_bluray_is_native() {
    let env = env_or_skip!();
    env.media("sur.wav", "7.1", 48000, "pcm_s24le");
    fs::remove_file(env.dee().join("dee_ddp_encoder.args")).ok();
    let out = env.deew(&[
        "-i",
        "sur.wav",
        "--ddp-mode",
        "bluray",
        "-b",
        "1100",
        "-r",
        "music_light",
    ]);
    ok(&out);
    let text = stdout(&out);
    assert_eq!(summary_row(&text, "Profile"), "Blu-ray");
    // the Blu-ray rates only, and no shim
    let args = env.run("dee_ddp_encoder");
    assert_eq!(opt(&args, "-e"), "bluray:drc_profile=music_light");
    assert_eq!(opt(&args, "--data-rate"), "1024");
    assert!(!has(&args, "-c") && !has(&args, "--layout"));
    assert!(env.dshim_run().is_none());
    assert!(env.out().join("sur.eb3").exists() && !env.out().join("sur.ec3").exists());
    ok(&env.deew(&["-i", "sur.wav", "--ddp-mode", "bluray", "-b", "100"]));
    assert_eq!(opt(&env.run("dee_ddp_encoder"), "--data-rate"), "768");
    assert!(!stdout(&out).contains("dee_shim not found"));
}

#[test]
fn ddp_71_without_dee_shim_needs_ddp_mode_bluray() {
    let env = env_or_skip!();
    env.media("sur.wav", "7.1", 48000, "pcm_s24le");
    env.set_shim(false);
    let msg = "dee_shim is required for 7.1 DDP with the standard profile, put dee_shim next to deew or in your PATH, or use --ddp-mode bluray (768-1664 kbps)";
    for extra in [&[][..], &["-b", "1152"][..]] {
        let mut args = vec!["-i", "sur.wav"];
        args.extend_from_slice(extra);
        let out = env.deew(&args);
        assert_error(&out, msg);
        assert!(
            stdout(&out).contains("dee_shim is required for 7.1 DDP with the standard profile")
        );
        assert!(!env.out().join("sur.ec3").exists() && !env.out().join("sur.eb3").exists());
    }
    // with --ddp-mode bluray it works, and the rate is rounded
    let out = env.deew(&["-i", "sur.wav", "--ddp-mode", "bluray", "-b", "1152"]);
    ok(&out);
    let text = stdout(&out);
    assert!(!text.contains("dee_shim is required"), "{text}");
    assert_eq!(summary_row(&text, "Profile"), "Blu-ray");
    assert_eq!(opt(&env.run("dee_ddp_encoder"), "--data-rate"), "1024");
    assert!(env.out().join("sur.eb3").exists() && !env.out().join("sur.ec3").exists());
    // other formats don't care
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    ok(&env.deew(&["-i", "in.wav"]));
}

#[test]
fn ddp_70_without_dee_shim_is_an_error() {
    let env = env_or_skip!();
    env.media("sur.wav", "7.1", 48000, "pcm_s24le");
    env.media("seven.wav", "7.0", 48000, "pcm_s24le");
    env.set_shim(false);
    let msg = "dee_shim is required for 7.0 DDP, put dee_shim next to deew or in your PATH (https://github.com/truehdd/dee-shim)";
    for input in [&["-i", "seven.wav"][..], &["-i", "sur.wav", "-dl"][..]] {
        for extra in [&[][..], &["--ddp-mode", "bluray"][..]] {
            let mut args = input.to_vec();
            args.extend_from_slice(extra);
            let out = env.deew(&args);
            assert_error(&out, msg);
            assert!(!env.out().join("sur.ec3").exists() && !env.out().join("sur.eb3").exists());
            assert!(!env.out().join("seven.ec3").exists() && !env.out().join("seven.eb3").exists());
        }
    }
}

#[test]
fn ddp_mode_is_ignored_for_other_outputs() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    let out = env.deew(&["-i", "in.wav", "--ddp-mode", "bluray"]);
    ok(&out);
    assert!(
        stdout(&out).contains("--ddp-mode only applies to 7.1 DDP output, it is ignored."),
        "{}",
        stdout(&out)
    );
    assert_eq!(opt(&env.run("dee_ddp_encoder"), "--data-rate"), "1024");
    assert!(env.out().join("in.ec3").exists());
    // 7.1 downmixed to 5.1 is not 7.1 DDP either
    env.media("sur.wav", "7.1", 48000, "pcm_s24le");
    let out = env.deew(&["-i", "sur.wav", "-dm", "6", "--ddp-mode", "bluray"]);
    assert!(stdout(&out).contains("it is ignored"));
}

#[test]
fn force_standard_is_an_unknown_option() {
    let env = env_or_skip!();
    env.media("sur.wav", "7.1", 48000, "pcm_s24le");
    for flag in ["-fs", "--force-standard"] {
        let out = env.deew(&["-i", "sur.wav", flag]);
        assert_eq!(out.status.code(), Some(2), "{flag}");
        let text = format!("{}{}", stdout(&out), String::from_utf8_lossy(&out.stderr));
        assert!(
            text.contains(&format!("deew: error: unrecognized arguments: {flag}")),
            "{text}"
        );
        assert!(!env.out().join("sur.ec3").exists());
    }
    let help = stdout(&env.deew_with(&["-h"], &[]));
    assert!(!help.contains("-fs") && !help.contains("force-standard"));
}

#[test]
fn seven_one_is_channel_swapped_not_mapped() {
    let env = env_or_skip!();
    env.media("sur.wav", "7.1", 48000, "pcm_s24le");
    let out = env.deew(&["-i", "sur.wav", "-k", "-la"]);
    ok(&out);
    let text = stdout(&out);
    assert!(
        text.contains("pan=7.1|c0=c0|c1=c1|c2=c2|c3=c3|c4=c6|c5=c7|c6=c4|c7=c5"),
        "{text}"
    );
    assert!(!text.contains("-map"), "{text}");
    assert_eq!(ffprobe_field(&env.temp().join("sur.wav"), "channels"), "8");
}

#[test]
fn dd_from_71_downmixes_to_51() {
    let env = env_or_skip!();
    env.media("sur.wav", "7.1", 48000, "pcm_s24le");
    ok(&env.deew(&["-i", "sur.wav", "-f", "dd"]));
    let args = env.run("dee_ddp_encoder");
    assert_eq!(opt(&args, "-c"), "5.1");
    assert_eq!(opt(&args, "-e"), "dd:drc_profile=none");
    assert_eq!(opt(&args, "--data-rate"), "640");
    assert!(env.out().join("sur.ac3").exists());
}

#[test]
fn downmix_is_done_by_dee() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    ok(&env.deew(&["-i", "in.wav", "-dm", "2"]));
    let args = env.run("dee_ddp_encoder");
    assert_eq!(opt(&args, "-c"), "stereo");
    assert_eq!(opt(&args, "--data-rate"), "256");
    ok(&env.deew(&["-i", "in.wav", "-dm", "1"]));
    assert_eq!(opt(&env.run("dee_ddp_encoder"), "-c"), "mono");
}

#[test]
fn resampling_44k_to_48k_32bit() {
    let env = env_or_skip!();
    env.media("cd.wav", "stereo", 44100, "pcm_s16le");
    let out = env.deew(&["-i", "cd.wav", "-k", "-la"]);
    ok(&out);
    assert!(stdout(&out).contains("aresample=resampler=soxr"));
    let wav = env.temp().join("cd.wav");
    assert_eq!(ffprobe_field(&wav, "sample_rate"), "48000");
    assert_eq!(ffprobe_field(&wav, "bits_per_sample"), "32");
}

#[test]
fn ac4_5_1_and_stereo() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    ok(&env.deew(&["-i", "in.wav", "-f", "ac4"]));
    let args = env.run("dee_ac4_encoder");
    assert_eq!(opt(&args, "--data-rate"), "448");
    assert_eq!(opt(&args, "-e"), "drc_profile=music_light");
    assert_eq!(opt(&args, "--loudness-management"), "measure_only");
    assert!(!has(&args, "-c"));
    // the 2048 sample delay of the AC-4 encoder is trimmed
    assert_eq!(opt(&args, "--start"), "0.042667");
    assert!(!has(&args, "--add-silence"));
    let manifest = env.temp().join("in.ac4.json");
    assert_eq!(opt(&args, "--output-manifest"), manifest.to_str().unwrap());
    assert!(env.out().join("in.ac4").exists());
    // the manifest of the encoder is a temp file
    assert!(!manifest.exists());
    ok(&env.deew(&["-i", "in.wav", "-f", "ac4", "-k"]));
    assert!(manifest.exists());

    ok(&env.deew(&["-i", "in.wav", "-f", "ac4", "-dm", "2"]));
    let args = env.run("dee_ac4_encoder");
    assert_eq!(opt(&args, "-c"), "stereo");
    assert_eq!(opt(&args, "--data-rate"), "256");
}

#[test]
fn ac4_input_rules() {
    let env = env_or_skip!();
    env.media("mono.wav", "mono", 48000, "pcm_s16le");
    assert_error(
        &env.deew(&["-i", "mono.wav", "-f", "ac4"]),
        "input channels for AC4 encoding can only be 2/6/8.",
    );
    env.media("sur.wav", "7.1", 48000, "pcm_s24le");
    // 7.1 becomes 5.1 automatically
    ok(&env.deew(&["-i", "sur.wav", "-f", "ac4"]));
    let args = env.run("dee_ac4_encoder");
    assert_eq!(opt(&args, "-c"), "5.1");
    assert_eq!(opt(&args, "--data-rate"), "448");
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    assert_error(
        &env.deew(&["-i", "in.wav", "-f", "ac4", "-r", "none"]),
        "DRC none is not supported for AC4 encoding (AC4 Atmos supports it).",
    );
    assert_error(
        &env.deew(&["-i", "in.wav", "-f", "ac4", "-dm", "1"]),
        "-dm/--downmix can only be 2 or 6 for AC4 and 2 for AAC.",
    );
}

#[test]
fn aac_has_no_drc_or_dialnorm_and_trims_the_priming() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    let out = env.deew(&["-i", "in.wav", "-f", "aac"]);
    ok(&out);
    assert!(
        stdout(&out).contains("Consider using qaac for AAC encoding."),
        "{}",
        stdout(&out)
    );
    let args = env.run("dee_aac_encoder");
    assert_eq!(opt(&args, "-e"), "aaclc");
    assert_eq!(opt(&args, "--data-rate"), "256");
    assert_eq!(opt(&args, "--loudness-management"), "skip");
    // DEE downmixes by itself
    assert!(!has(&args, "-c"));
    // the 1600 sample priming of the AAC encoder (+32 for the downmix) is trimmed
    assert_eq!(opt(&args, "--start"), "0.034");
    assert!(!has(&args, "--add-silence") && !has(&args, "-d"));
    assert!(env.out().join("in.aac").exists());

    // always 2.0: the bitrate table is the stereo one, 5.1 and 7.1 are fine
    ok(&env.deew(&["-i", "in.wav", "-f", "aac", "-dm", "2", "-b", "100"]));
    assert_eq!(opt(&env.run("dee_aac_encoder"), "--data-rate"), "96");
    env.media("sur.wav", "7.1", 48000, "pcm_s24le");
    ok(&env.deew(&["-i", "sur.wav", "-f", "aac"]));
    // no mono: neither as input nor as -dm
    env.media("mono.wav", "mono", 48000, "pcm_s16le");
    let message = "AAC needs a 2.0, 5.1 or 7.1 input (DEE's AAC encoder always outputs 2.0)";
    assert_error(&env.deew(&["-i", "mono.wav", "-f", "aac"]), message);
    assert_error(
        &env.deew(&["-i", "mono.wav", "-f", "aac", "-dm", "1"]),
        message,
    );
    for (file, dm) in [("in.wav", "1"), ("sur.wav", "6"), ("sur.wav", "1")] {
        assert_error(
            &env.deew(&["-i", file, "-f", "aac", "-dm", dm]),
            "-dm/--downmix can only be 2 or 6 for AC4 and 2 for AAC.",
        );
    }
    assert_error(
        &env.deew(&["-i", "in.wav", "-f", "aac", "-dn", "-27"]),
        "custom dialnorm is only supported for DD, DDP, DDP Atmos, TrueHD, TrueHD Atmos and AC4 encoding.",
    );
}

#[test]
fn encoder_delay_depends_on_bitrate_and_dee_downmix() {
    let env = env_or_skip!();
    env.media("st.wav", "stereo", 48000, "pcm_s24le");
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    let start = |file: &str, extra: &[&str], tool: &str| {
        let mut args = vec!["-i", file];
        args.extend(extra);
        ok(&env.deew(&args));
        opt(&env.run(tool), "--start").to_string()
    };
    assert_eq!(
        start("st.wav", &["-f", "aac", "-b", "64"], "dee_aac_encoder"),
        "0.035625"
    );
    assert_eq!(
        start("st.wav", &["-f", "aac", "-b", "80"], "dee_aac_encoder"),
        "0.035458"
    );
    assert_eq!(
        start("in.wav", &["-f", "aac", "-b", "256"], "dee_aac_encoder"),
        "0.034"
    );
    assert_eq!(
        start("in.wav", &["-f", "aac", "-b", "64"], "dee_aac_encoder"),
        "0.036292"
    );
    assert_eq!(
        start("in.wav", &["-f", "ddp", "-dm", "2"], "dee_ddp_encoder"),
        "0.006"
    );
    assert_eq!(
        start("in.wav", &["-f", "ddp"], "dee_ddp_encoder"),
        "0.005333"
    );
}

#[test]
fn thd_presentation_and_leftovers() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    ok(&env.deew(&["-i", "in.wav", "-f", "thd", "-r", "music_light"]));
    let args = env.run("dee_dthd_encoder");
    assert_eq!(opt(&args, "--presentation"), "6ch:drc_profile=music_light");
    assert!(env.dshim_run().is_none());
    assert_eq!(opt(&args, "--input-format"), "wav");
    assert!(!has(&args, "--data-rate") && !has(&args, "--start"));
    assert_eq!(
        summary_row(
            &stdout(&env.deew(&["-i", "in.wav", "-f", "thd", "-r", "speech"])),
            "Bitrate"
        ),
        "N/A"
    );
    let thd = env.out().join("in.thd");
    assert!(thd.exists());
    // the log and mll files DEE leaves next to the output are removed
    assert!(!env.out().join("in.thd.log.html").exists());
    assert!(!env.out().join("in.thd.mll").exists());
    // the leftovers of a named output follow its name
    ok(&env.deew(&["-i", "in.wav", "-f", "thd", "-r", "speech", "-on", "Named"]));
    assert!(env.out().join("Named.thd").exists());
    assert!(!env.out().join("Named.thd.log.html").exists());
    assert!(!env.out().join("Named.thd.mll").exists());

    env.media("sur.wav", "7.1", 48000, "pcm_s24le");
    ok(&env.deew(&["-i", "sur.wav", "-f", "thd", "-r", "speech"]));
    let args = env.run("dee_dthd_encoder");
    assert_eq!(opt(&args, "--presentation"), "8ch:drc_profile=speech");
    assert!(env.dshim_run().is_none());
    assert_error(
        &env.deew(&["-i", "in.wav", "-f", "thd-atmos", "-dm", "2"]),
        "-dm/--downmix only works for DD/DDP/AC4/AAC encoding and for TrueHD (2 or 6), not for the Atmos formats.",
    );
    // TrueHD takes the dialnorm through skip:dialnorm
    ok(&env.deew(&["-i", "in.wav", "-f", "thd", "-dn", "-3"]));
    let args = env.run("dee_dthd_encoder");
    let at = args
        .iter()
        .position(|a| a == "--loudness-management")
        .unwrap();
    assert_eq!(args[at + 1], "skip:dialnorm=-3");
    assert!(env.last_run("dee_measure_loudness").is_none());
}

#[test]
fn thd_downmix_is_done_by_ffmpeg() {
    let env = env_or_skip!();
    env.media("sur.wav", "7.1", 48000, "pcm_s24le");
    let wav = env.temp().join("sur.dm6.wav");

    let out = env.deew(&[
        "-i", "sur.wav", "-f", "thd", "-r", "speech", "-dm", "6", "-k", "-la",
    ]);
    ok(&out);
    let text = stdout(&out);
    assert!(
        text.contains("pan=5.1|c0=c0|c1=c1|c2=c2|c3=c3|c4=0.5*c4+0.5*c6|c5=0.5*c5+0.5*c7'"),
        "{text}"
    );
    assert!(text.contains("aformat=sample_fmts=dbl"), "{text}");
    assert!(!text.contains("alimiter"), "{text}");
    assert_eq!(ffprobe_field(&wav, "channels"), "6");
    let args = env.run("dee_dthd_encoder");
    assert_eq!(opt(&args, "--presentation"), "6ch:drc_profile=speech");
    assert!(!has(&args, "-c"));
    assert_eq!(last_summary_row(&text, "Channels"), "5.1");
    assert!(env.out().join("sur.thd").exists());

    // -dm 2 and the FLAC suggestion; without DRC through dee_shim
    env.media("sur2.wav", "7.1", 48000, "pcm_s24le");
    let out = env.deew(&[
        "-i", "sur2.wav", "-f", "thd", "-dm", "2", "-np", "-k", "-la",
    ]);
    ok(&out);
    let text = stdout(&out);
    assert!(text.contains("pan=stereo|c0=c0+0.7071*c2+0.3536*c4+0.3536*c6|c1=c1+0.7071*c2+0.3536*c5+0.3536*c7,channelsplit="), "{text}");
    assert_eq!(
        ffprobe_field(&env.temp().join("sur2.dm2.wav"), "channels"),
        "2"
    );
    assert!(
        text.contains("[l]alimiter=limit=0.98855:level=0:latency=1[ll]"),
        "{text}"
    );
    assert!(
        text.contains("Consider using FLAC for lossless stereo encoding."),
        "{text}"
    );
    assert_eq!(last_summary_row(&text, "Channels"), "stereo");
    let shim = env.dshim_run().unwrap();
    assert_eq!(opt(&shim, "--drc"), "none");
    let args = env.run("dee_dthd_encoder");
    assert_eq!(opt(&args, "--presentation"), "2ch:drc_profile=film_light");

    // 5.1 to stereo
    env.media("five.wav", "5.1", 48000, "pcm_s24le");
    let out = env.deew(&[
        "-i", "five.wav", "-f", "thd", "-dm", "2", "-np", "-k", "-la",
    ]);
    ok(&out);
    assert!(
        stdout(&out).contains(
            "pan=stereo|c0=c0+0.7071*c2+0.7071*c4|c1=c1+0.7071*c2+0.7071*c5,channelsplit="
        )
    );
    assert_eq!(
        ffprobe_field(&env.temp().join("five.dm2.wav"), "channels"),
        "2"
    );

    // mono, a downmix that is not one and the Atmos formats are errors
    assert_error(
        &env.deew(&["-i", "sur.wav", "-f", "thd", "-dm", "1"]),
        "-dm 1 doesn't work for TrueHD, it has no mono (2.0, 5.1 and 7.1 only).",
    );
    assert_error(
        &env.deew(&["-i", "five.wav", "-f", "thd", "-dm", "6"]),
        "downmix value has to be lower than the number of input channels.",
    );
}

#[test]
fn kept_intermediates_are_not_reused_for_other_settings() {
    let env = env_or_skip!();
    env.media("sur.wav", "7.1", 48000, "pcm_s24le");
    let channels = |name: &str| ffprobe_field(&env.temp().join(name), "channels");
    let thd = |extra: &[&str]| {
        let mut args = vec!["-i", "sur.wav", "-f", "thd", "-r", "speech", "-np", "-k"];
        args.extend(extra);
        let out = env.deew(&args);
        ok(&out);
        stdout(&out)
    };
    thd(&["-dm", "2"]);
    assert_eq!(channels("sur.dm2.wav"), "2");
    let text = thd(&["-dm", "6"]);
    assert!(!text.contains("Intermediate already exists"), "{text}");
    assert_eq!(channels("sur.dm6.wav"), "6");
    assert_eq!(
        opt(&env.run("dee_dthd_encoder"), "--presentation"),
        "6ch:drc_profile=speech"
    );
    // the same settings again reuse the wav
    let text = thd(&["-dm", "6"]);
    assert!(text.contains("Intermediate already exists"), "{text}");
    // a plain run and a -dl run have wavs of their own
    thd(&[]);
    assert_eq!(channels("sur.wav"), "8");
    let text = thd(&["-dl"]);
    assert!(!text.contains("Intermediate already exists"), "{text}");
    assert!(env.temp().join("sur.dl.wav").exists());
    thd(&["-dm", "6", "-dl"]);
    assert_eq!(channels("sur.dm6.dl.wav"), "6");
    // DD takes the plain one: DEE leaves the LFE out, ffmpeg does not
    let out = env.deew(&["-i", "sur.wav", "-f", "dd", "-dl", "-k"]);
    ok(&out);
    assert!(stdout(&out).contains("Intermediate already exists"));
}

#[test]
fn thd_downmix_with_a_silent_lfe() {
    let env = env_or_skip!();
    env.media("sur.wav", "7.1", 48000, "pcm_s24le");
    let out = env.deew(&[
        "-i", "sur.wav", "-f", "thd", "-r", "speech", "-dm", "6", "-dl", "-k", "-la",
    ]);
    ok(&out);
    let text = stdout(&out);
    assert!(
        text.contains("pan=5.1|c0=c0|c1=c1|c2=c2|c3=0*c0|c4=0.5*c4+0.5*c6"),
        "{text}"
    );
    assert!(
        text.contains("TrueHD can't leave out the LFE channel, it is kept silent."),
        "{text}"
    );
    assert_eq!(last_summary_row(&text, "Channels"), "5.1 (silent LFE)");
    assert_eq!(
        opt(&env.run("dee_dthd_encoder"), "--presentation"),
        "6ch:drc_profile=speech"
    );
}

#[test]
fn thd_without_drc_runs_through_dee_shim() {
    let env = env_or_skip!();
    env.media("sur.wav", "7.1", 48000, "pcm_s24le");
    // none is the default
    let out = env.deew(&["-i", "sur.wav", "-f", "thd"]);
    ok(&out);
    let shim = env.dshim_run().unwrap();
    assert_eq!(shim[0], "dthd_encoder");
    assert_eq!(
        opt(&shim, "--binary"),
        env.dee().join("dee_dthd_encoder").to_str().unwrap()
    );
    assert!(!shim.iter().any(|a| a == "--shim"));
    assert_eq!(opt(&shim, "--drc"), "none");
    // the native command keeps a valid profile and gets the same options
    let args = env.run("dee_dthd_encoder");
    let rest: Vec<String> = shim[shim.iter().position(|a| a == "--").unwrap() + 1..].to_vec();
    assert_eq!(rest, args);
    assert_eq!(opt(&args, "--presentation"), "8ch:drc_profile=film_light");
    assert_dee_temp(&env, opt(&args, "--temp-dir"));
    // the dshim line in front of DEE's output does not disturb the parsing
    assert!(stdout(&out).contains("finished: sur.wav"));
    assert!(env.out().join("sur.thd").exists());
    assert!(!env.out().join("sur.thd.log.html").exists());
    assert!(temp_is_empty(&env));
}

#[test]
fn thd_without_drc_needs_dee_shim() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    env.set_shim(false);
    for args in [
        vec!["-i", "in.wav", "-f", "thd"],
        vec!["-i", "in.wav", "-f", "thd", "-r", "none"],
    ] {
        let out = env.deew(&args);
        assert_error(
            &out,
            "dee_shim is required for TrueHD with DRC none (the default), put dee_shim next to deew or in your PATH, or use -r with another profile",
        );
    }
    // refused before any work started
    assert!(env.last_run("dee_dthd_encoder").is_none());
    assert!(!env.temp().join("in.wav").exists());
    // the other profiles and formats and measuring do not need it
    ok(&env.deew(&["-i", "in.wav", "-f", "thd", "-r", "speech"]));
    ok(&env.deew(&["-i", "in.wav", "-f", "dd"]));
    ok(&env.deew(&["-i", "in.wav", "-f", "thd", "-mo"]));
    assert!(env.dshim_run().is_none());
}

#[test]
fn missing_thd_encoder_has_a_hint() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    fs::remove_file(env.dee().join("dee_dthd_encoder")).unwrap();
    let out = env.deew(&["-i", "in.wav", "-f", "thd"]);
    assert_error(&out, "dee_dthd_encoder does not exist.");
    assert!(
        stdout(&out).contains("https://github.com/truehdd/dthd-mac2linux"),
        "{}",
        stdout(&out)
    );
    // other formats do not need it
    ok(&env.deew(&["-i", "in.wav"]));
}

#[test]
fn missing_dee_tool_is_named() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    env.hide_dee_tools();
    assert_error(
        &env.deew(&["-i", "in.wav"]),
        "dee_ddp_encoder does not exist.",
    );
    assert_error(
        &env.deew(&["-i", "in.wav", "-f", "ac4"]),
        "dee_ac4_encoder does not exist.",
    );
}

#[test]
fn measure_only_of_an_atmos_source_measures_the_master() {
    let env = env_or_skip!();
    env.truehd("atmos.thd");
    // the format and the downmix do not matter
    for (index, extra) in [
        vec![],
        vec!["-f", "thd"],
        vec!["-f", "ddp-atmos", "-dm", "2"],
    ]
    .into_iter()
    .enumerate()
    {
        let mut args = vec!["-i", "atmos.thd", "-mo", "-la"];
        args.extend(extra);
        let out = env.deew(&args);
        ok(&out);
        let text = stdout(&out);
        assert!(
            text.contains("finished: atmos.thd (-23 dB, dialogue-gated, 16% speech, program -24.3 LKFS, LRA 5.6 LU, true peak -15.3 dBTP) - took "),
            "{index}: {text}"
        );
        let truehdd = env.truehdd_run().unwrap();
        assert_eq!(
            opt(&truehdd, "--output-path"),
            env.temp().join("atmos").to_str().unwrap()
        );
        let args = env.run("dee_measure_loudness");
        assert_eq!(opt(&args, "--input-format"), "atmos_mezz");
        assert_eq!(
            opt(&args, "-i"),
            env.temp().join("atmos.atmos").to_str().unwrap()
        );
        for flag in [
            "-o",
            "-e",
            "--data-rate",
            "--loudness-management",
            "--temp-dir",
            "--output-manifest",
        ] {
            assert!(!has(&args, flag), "{flag}");
        }
        assert!(env.dshim_run().is_none());
        assert!(env.last_run("dee_ddpjoc_encoder").is_none());
        assert!(env.last_run("dee_ddp_encoder").is_none());
        assert!(!env.out().join("atmos.ec3").exists());
        assert!(temp_is_empty(&env));
    }
}

#[test]
fn ddp_atmos_with_dee_shim() {
    let env = env_or_skip!();
    env.truehd("atmos.thd");
    let out = env.deew(&["-i", "atmos.thd", "-f", "ddp-atmos"]);
    ok(&out);
    let text = stdout(&out);
    assert_eq!(summary_row(&text, "Format"), "DDP Atmos");
    assert_eq!(last_summary_row(&text, "Channels"), "Atmos");
    assert_eq!(summary_row(&text, "Bitrate"), "1024 kbps");
    assert_eq!(summary_row(&text, "dee_shim version"), "0.4.0+g790d050");
    assert!(!text.contains("dee_shim wasn't found"), "{text}");

    // truehdd decodes into the temp directory
    let truehdd = env.truehdd_run().unwrap();
    assert_eq!(truehdd[0], "decode");
    assert_eq!(opt(&truehdd, "--presentation"), "3");
    assert!(has(&truehdd, "--json"));
    assert_eq!(
        opt(&truehdd, "--output-path"),
        env.temp().join("atmos").to_str().unwrap()
    );
    assert_eq!(truehdd.last().unwrap(), "atmos.thd");

    // dee_shim runs the encoder
    let shim = env.dshim_run().unwrap();
    assert_eq!(shim[0], "ddpjoc_encoder");
    assert_eq!(
        opt(&shim, "--binary"),
        env.dee().join("dee_ddpjoc_encoder").to_str().unwrap()
    );
    assert!(!shim.iter().any(|a| a == "--shim"));
    assert_eq!(opt(&shim, "--oamd-as-is"), "on");
    assert_eq!(opt(&shim, "--bitrate"), "1024");
    assert_eq!(opt(&shim, "--layout"), "5.1");
    assert!(!has(&shim, "--drc"));
    let rest: Vec<String> = shim[shim.iter().position(|a| a == "--").unwrap() + 1..].to_vec();
    assert_eq!(rest, env.run("dee_ddpjoc_encoder"));
    assert_eq!(opt(&rest, "-e"), "ddpjoc:drc_profile=none");
    assert_eq!(opt(&rest, "--input-format"), "atmos_mezz");
    assert_eq!(
        opt(&rest, "-i"),
        env.temp().join("atmos.atmos").to_str().unwrap()
    );
    assert_eq!(
        opt(&rest, "-o"),
        env.out().join("atmos.ec3").to_str().unwrap()
    );
    // DEE-6.7 joc encoders have no --temp-dir
    assert!(!has(&rest, "--temp-dir") && !has(&rest, "--data-rate"));
    assert!(env.out().join("atmos.ec3").exists());
    // all three .atmos files are gone
    assert!(temp_is_empty(&env));
}

#[test]
fn warp_mode_and_bed_conform_only_work_for_the_truehdd_formats() {
    let env = env_or_skip!();
    env.media("fv.wav", "5.1", 48000, "pcm_s24le");
    for format in ["dd", "ddp", "thd", "ac4", "aac"] {
        let out = env.deew(&["-i", "fv.wav", "-f", format, "-wm", "normal"]);
        assert_error(
            &out,
            "-wm/--warp-mode only works for DDP Atmos, TrueHD Atmos, AC4 Atmos, AC4 IMS and -mo.",
        );
        let out = env.deew(&["-i", "fv.wav", "-f", format, "-bc", "on"]);
        assert_error(
            &out,
            "-bc/--bed-conform only works for DDP Atmos, TrueHD Atmos, AC4 Atmos, AC4 IMS and -mo.",
        );
    }
    let out = env.deew(&["-i", "fv.wav", "-f", "ddp-atmos", "-wm", "stereo"]);
    assert_error(
        &out,
        "-wm/--warp-mode can only be normal, warping, prologiciix or loro.",
    );
    let out = env.deew(&["-i", "fv.wav", "-f", "ddp-atmos", "-bc", "maybe"]);
    assert_error(&out, "-sps, -lpf, -s3a and -bc can only be on or off.");
    // -mo accepts them for any format
    ok(&env.deew(&[
        "-i", "fv.wav", "-mo", "-f", "thd", "-wm", "loro", "-bc", "on",
    ]));
    // the options of an allowed run are ignored for files that don't go through truehdd
    let out = env.deew(&["-i", "fv.wav", "-f", "ac4-ims", "-wm", "loro", "-bc", "on"]);
    ok(&out);
    assert!(env.truehdd_run().is_none());
}

#[test]
fn warp_mode_and_bed_conform_reach_truehdd() {
    let env = env_or_skip!();
    env.truehd("atmos.thd");
    let out = env.deew(&["-i", "atmos.thd", "-f", "ddp-atmos", "-k"]);
    ok(&out);
    let truehdd = env.truehdd_run().unwrap();
    assert!(!has(&truehdd, "--warp-mode") && !has(&truehdd, "--bed-conform"));
    assert!(env.temp().join("atmos.atmos").exists());
    assert!(!stdout(&out).contains("warp mode"), "{}", stdout(&out));

    // other options: another name, the kept decode is not reused
    fs::remove_file(env.path("bin").join("truehdd.args")).unwrap();
    let out = env.deew(&[
        "-i",
        "atmos.thd",
        "-f",
        "ddp-atmos",
        "-k",
        "-wm",
        "LoRo",
        "-bc",
        "on",
    ]);
    ok(&out);
    let text = stdout(&out);
    assert_eq!(
        summary_row(&text, "truehdd"),
        "warp mode loro, bed conform on"
    );
    let truehdd = env.truehdd_run().unwrap();
    assert_eq!(opt(&truehdd, "--warp-mode"), "loro");
    assert!(has(&truehdd, "--bed-conform"));
    assert_eq!(
        opt(&truehdd, "--output-path"),
        env.temp().join("atmos.wm-loro.bc").to_str().unwrap()
    );
    for ext in ["atmos", "atmos.metadata", "atmos.audio"] {
        assert!(
            env.temp().join(format!("atmos.wm-loro.bc.{ext}")).exists(),
            "{ext}"
        );
    }
    // the same options reuse it
    fs::remove_file(env.path("bin").join("truehdd.args")).unwrap();
    ok(&env.deew(&[
        "-i",
        "atmos.thd",
        "-f",
        "ddp-atmos",
        "-k",
        "-wm",
        "loro",
        "-bc",
        "on",
    ]));
    assert!(env.truehdd_run().is_none());

    // the config sets them, the flags override it
    let config = fs::read_to_string(env.config()).unwrap();
    fs::write(
        env.config(),
        format!("truehdd_warp_mode = 'warping'\ntruehdd_bed_conform = true\n{config}"),
    )
    .unwrap();
    let out = env.deew(&["-i", "atmos.thd", "-f", "thd-atmos", "-k"]);
    ok(&out);
    assert_eq!(
        summary_row(&stdout(&out), "truehdd"),
        "warp mode warping, bed conform on"
    );
    let truehdd = env.truehdd_run().unwrap();
    assert_eq!(opt(&truehdd, "--warp-mode"), "warping");
    assert!(has(&truehdd, "--bed-conform"));
    assert_eq!(
        opt(&truehdd, "--output-path"),
        env.temp().join("atmos.wm-warping.bc").to_str().unwrap()
    );
    fs::remove_file(env.path("bin").join("truehdd.args")).unwrap();
    let out = env.deew(&[
        "-i",
        "atmos.thd",
        "-f",
        "ddp-atmos",
        "-k",
        "-wm",
        "normal",
        "-bc",
        "off",
    ]);
    ok(&out);
    assert_eq!(summary_row(&stdout(&out), "truehdd"), "warp mode normal");
    let truehdd = env.truehdd_run().unwrap();
    assert_eq!(opt(&truehdd, "--warp-mode"), "normal");
    assert!(!has(&truehdd, "--bed-conform"));
    assert_eq!(
        opt(&truehdd, "--output-path"),
        env.temp().join("atmos.wm-normal").to_str().unwrap()
    );
}

#[test]
fn atmos_files_are_kept_with_k() {
    let env = env_or_skip!();
    env.truehd("atmos.thd");
    ok(&env.deew(&["-i", "atmos.thd", "-f", "ddp-atmos", "-k", "-b", "1000"]));
    for ext in ["atmos", "atmos.metadata", "atmos.audio"] {
        assert!(env.temp().join(format!("atmos.{ext}")).exists(), "{ext}");
    }
    assert_eq!(opt(&env.dshim_run().unwrap(), "--bitrate"), "1024");
    // the next run reuses the decode
    fs::remove_file(env.path("bin").join("truehdd.args")).unwrap();
    let out = env.deew(&["-i", "atmos.thd", "-f", "ddp-atmos", "-k"]);
    ok(&out);
    assert!(env.truehdd_run().is_none());
    assert!(
        stdout(&out).contains("Intermediate already exists for the following file(s): atmos.thd"),
        "{}",
        stdout(&out)
    );
}

#[test]
fn old_underscore_format_names_are_still_accepted() {
    let env = env_or_skip!();
    env.truehd("atmos.thd");
    let out = env.deew(&["-i", "atmos.thd", "-f", "ddp_atmos"]);
    ok(&out);
    let text = stdout(&out);
    assert_eq!(summary_row(&text, "Format"), "DDP Atmos");
    assert!(!text.contains("ddp_atmos"), "{text}");
}

#[test]
fn ddp_atmos_modes() {
    let env = env_or_skip!();
    env.truehd("atmos.thd");
    for (mode, layout, native, ext, rate) in [
        ("streaming", "5.1", "ddpjoc:drc_profile=none", "ec3", "1024"),
        (
            "standard",
            "7.1/eac3",
            "bluray:drc_profile=none",
            "ec3",
            "1536",
        ),
        ("bluray", "7.1", "bluray:drc_profile=none", "eb3", "1536"),
    ] {
        let out = env.deew(&[
            "-i",
            "atmos.thd",
            "-f",
            "ddp-atmos",
            "--ddp-atmos-mode",
            mode,
        ]);
        ok(&out);
        let text = stdout(&out);
        assert_eq!(
            summary_row(&text, "Bitrate"),
            format!("{rate} kbps"),
            "{mode}"
        );
        let shim = env.dshim_run().unwrap();
        assert_eq!(opt(&shim, "--layout"), layout, "{mode}");
        assert_eq!(opt(&shim, "--bitrate"), rate, "{mode}");
        assert_eq!(opt(&shim, "--oamd-as-is"), "on");
        let rest: Vec<String> = shim[shim.iter().position(|a| a == "--").unwrap() + 1..].to_vec();
        assert_eq!(opt(&rest, "-e"), native, "{mode}");
        assert!(env.out().join(format!("atmos.{ext}")).exists(), "{mode}");
        // rates of the mode only
        let out = env.deew(&[
            "-i",
            "atmos.thd",
            "-f",
            "ddp-atmos",
            "--ddp-atmos-mode",
            mode,
            "-b",
            "5000",
        ]);
        ok(&out);
        let top = match mode {
            "streaming" => "1024",
            "standard" => "2048",
            _ => "1664",
        };
        assert_eq!(opt(&env.dshim_run().unwrap(), "--bitrate"), top, "{mode}");
    }
    assert!(!env.out().join("atmos.eb3").metadata().unwrap().is_dir());
    let out = env.deew(&[
        "-i",
        "atmos.thd",
        "-f",
        "ddp-atmos",
        "--ddp-atmos-mode",
        "x",
    ]);
    assert_error(
        &out,
        "--ddp-atmos-mode can only be streaming, standard or bluray.",
    );
}

#[test]
fn output_to_input_and_config_output_path() {
    let env = env_or_skip!();
    let sub = env.work().join("sub");
    fs::create_dir_all(&sub).unwrap();
    let input = env.media("sub/in.wav", "5.1", 48000, "pcm_s24le");
    // -oi beats -o
    let out = env.deew(&["-i", input.to_str().unwrap(), "-oi"]);
    ok(&out);
    assert!(sub.join("in.ec3").exists());
    assert!(!env.out().join("in.ec3").exists());
    fs::remove_file(sub.join("in.ec3")).unwrap();

    // output.path of the config, created if missing
    let cfg = env.config();
    let base = fs::read_to_string(&cfg).unwrap();
    let cfg_out = env.path("cfgout/deep");
    fs::write(
        &cfg,
        format!("{base}[output]\npath = '{}'\n", cfg_out.display()),
    )
    .unwrap();
    ok(&env.deew_with(&["-np", "-i", input.to_str().unwrap()], &[]));
    assert!(cfg_out.join("in.ec3").exists());
    // -o overrides it, -oi overrides both
    ok(&env.deew(&["-i", input.to_str().unwrap()]));
    assert!(env.out().join("in.ec3").exists());
    ok(&env.deew_with(&["-np", "-oi", "-i", input.to_str().unwrap()], &[]));
    assert!(sub.join("in.ec3").exists());
    fs::remove_file(sub.join("in.ec3")).unwrap();

    // output.to_input = true
    fs::write(&cfg, format!("{base}[output]\nto_input = true\n")).unwrap();
    ok(&env.deew_with(&["-np", "-i", input.to_str().unwrap()], &[]));
    assert!(sub.join("in.ec3").exists());
}

#[test]
fn ddp_atmos_needs_dee_shim() {
    let env = env_or_skip!();
    env.truehd("atmos.thd");
    env.set_shim(false);
    let out = env.deew(&["-i", "atmos.thd", "-f", "ddp-atmos"]);
    assert_error(
        &out,
        "dee_shim is required for ddp-atmos, put dee_shim next to deew or in your PATH (https://github.com/truehdd/dee-shim)",
    );
    // refused before the decode
    assert!(env.truehdd_run().is_none());
    assert!(env.last_run("dee_ddpjoc_encoder").is_none());
    // AC4 Atmos does not use it
    ok(&env.deew(&["-i", "atmos.thd", "-f", "ac4-atmos"]));
    assert!(env.dshim_run().is_none());
    assert_eq!(
        summary_row(
            &stdout(&env.deew(&["-i", "atmos.thd", "-f", "ac4-atmos"])),
            "dee_shim version"
        ),
        "not found"
    );
}

#[test]
fn ac4_atmos() {
    let env = env_or_skip!();
    env.truehd("atmos.thd");
    let out = env.deew(&["-i", "atmos.thd", "-f", "ac4-atmos", "-d", "+100ms"]);
    ok(&out);
    assert_eq!(summary_row(&stdout(&out), "Format"), "AC4 Atmos");
    assert_eq!(last_summary_row(&stdout(&out), "Channels"), "Atmos");
    let args = env.run("dee_ac4ajoc_encoder");
    assert_eq!(opt(&args, "--level"), "3");
    assert_eq!(opt(&args, "--data-rate"), "448");
    assert_eq!(opt(&args, "-e"), "drc_profile=none");
    // 100 ms minus the 2048 sample delay of the AC-4 encoder
    assert_eq!(opt(&args, "--add-silence"), "0.057333:0");
    assert!(!has(&args, "--temp-dir") && !has(&args, "--input-format"));
    assert!(env.out().join("atmos.ac4").exists());
    assert!(temp_is_empty(&env));
    // no dee_shim for AC4
    assert!(env.dshim_run().is_none());
}

#[test]
fn atmos_input_rules() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    env.truehd("atmos.thd");
    for format in ["ddp-atmos", "thd-atmos", "ac4-atmos"] {
        assert_error(
            &env.deew(&["-i", "in.wav", "-f", format]),
            "ddp-atmos, thd-atmos and ac4-atmos need a TrueHD Atmos, DTS:X or Auro-3D (24-bit DTS-HD MA, FLAC, PCM or TrueHD with a harletty that reads PCM) input or an Atmos master, in.wav is none of them.",
        );
        assert_error(
            &env.deew(&["-i", "atmos.thd", "-f", format, "-dm", "2"]),
            "-dm/--downmix only works for DD/DDP/AC4/AAC encoding and for TrueHD (2 or 6), not for the Atmos formats.",
        );
        if format == "ac4-atmos" {
            assert_error(
                &env.deew(&["-i", "atmos.thd", "-f", format, "-dn", "-27"]),
                "custom dialnorm is only supported for DD, DDP, DDP Atmos, TrueHD, TrueHD Atmos and AC4 encoding.",
            );
        }
    }
    assert!(env.truehdd_run().is_none());
    // truehd is no input for the other formats' downmix checks: a missing
    // Atmos presentation is found by truehdd
    let out = env.deew_env(
        &["-i", "atmos.thd", "-f", "ddp-atmos"],
        &[("FAKE_NO_ATMOS", "1")],
    );
    assert_error(&out, "the input has no Atmos presentation.");
    assert!(env.last_run("dee_ddpjoc_encoder").is_none());
    // the decoded PCM truehdd leaves for such inputs is removed
    assert!(temp_is_empty(&env));
}

#[test]
fn truehd_without_atmos_is_rejected_early() {
    let env = env_or_skip!();
    env.truehd("plain.thd");
    // the real ffprobe: no Atmos profile on a TrueHD stream ffmpeg wrote
    for format in ["ddp-atmos", "thd-atmos", "ac4-atmos"] {
        let out = env.deew_env(
            &["-i", "plain.thd", "-f", format],
            &[("FAKE_NO_PROFILE", "1")],
        );
        assert_error(
            &out,
            "ddp-atmos, thd-atmos and ac4-atmos need a TrueHD Atmos, DTS:X or Auro-3D (24-bit DTS-HD MA, FLAC, PCM or TrueHD with a harletty that reads PCM) input or an Atmos master, plain.thd has no Atmos, DTS:X or Auro-3D.",
        );
    }
    // rejected before truehdd
    assert!(env.truehdd_run().is_none());
    assert!(temp_is_empty(&env));
    // with the profile it passes
    ok(&env.deew(&["-i", "plain.thd", "-f", "ddp-atmos"]));
}

#[test]
fn truehdd_failure_is_a_job_error() {
    let env = env_or_skip!();
    env.truehd("atmos.thd");
    let out = env.deew_env(
        &["-i", "atmos.thd", "-f", "ddp-atmos"],
        &[("FAKE_FAIL_TOOL", "truehdd")],
    );
    assert_error(&out, "truehdd failed (exit code 4): bad stream");
    assert!(env.last_run("dee_ddpjoc_encoder").is_none());
}

#[test]
fn missing_truehdd_is_only_needed_for_atmos() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    env.truehd("atmos.thd");
    fs::remove_file(env.path("bin").join("truehdd")).unwrap();
    let out = env.deew(&["-i", "in.wav"]);
    ok(&out);
    assert_eq!(summary_row(&stdout(&out), "truehdd version"), "not found");
    assert_error(
        &env.deew(&["-i", "atmos.thd", "-f", "ddp-atmos"]),
        "truehdd does not exist.",
    );
}

#[test]
fn dd_and_ddp_dialnorm_is_skipped_to_dee() {
    let env = env_or_skip!();
    env.media("a.wav", "5.1", 48000, "pcm_s24le");
    for format in ["dd", "ddp"] {
        let out = env.deew(&["-i", "a.wav", "-f", format, "-dn", "-27", "-k"]);
        ok(&out);
        assert_eq!(summary_row(&stdout(&out), "Dialnorm"), "-27 dB");
        let args = env.run("dee_ddp_encoder");
        let at = args
            .iter()
            .position(|a| a == "--loudness-management")
            .unwrap();
        assert_eq!(args[at + 1], "skip:dialnorm=-27");
        assert!(!has(&args, "--input-manifest"));
        assert!(!args.contains(&"measure_only".to_string()));
        assert!(!env.temp().join("a.loudness.json").exists());
    }
    assert!(env.last_run("dee_measure_loudness").is_none());
}

#[test]
fn custom_dialnorm_goes_through_a_loudness_manifest() {
    let env = env_or_skip!();
    env.media("a.wav", "5.1", 48000, "pcm_s24le");
    let out = env.deew(&["-i", "a.wav", "-f", "ac4", "-dn", "-27", "-k"]);
    ok(&out);
    assert!(stdout(&out).contains(
        "Consider leaving the dialnorm value at 0 (auto), setting it manually can be dangerous."
    ));
    assert_eq!(summary_row(&stdout(&out), "Dialnorm"), "-27 dB");
    let manifest = env.temp().join("a.loudness.json");
    let measure = env.run("dee_measure_loudness");
    assert_eq!(
        opt(&measure, "-i"),
        env.temp().join("a.wav").to_str().unwrap()
    );
    assert_eq!(
        opt(&measure, "--output-manifest"),
        manifest.to_str().unwrap()
    );
    assert!(!has(&measure, "--temp-dir"));
    let args = env.run("dee_ac4_encoder");
    assert_eq!(opt(&args, "--input-manifest"), manifest.to_str().unwrap());
    // the dialnorm replaced the measured loudness
    let value: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&manifest).unwrap()).unwrap();
    let entry =
        &value["loudness_manifest"]["tracks"][0]["channel_based_loudness"]["multichannel_5_1"];
    assert_eq!(entry["integrated_loudness"], -27.0);
    assert_eq!(entry["integrated_speech_gated_loudness"], -27.0);
    assert_eq!(entry["true_peaks"][0], -1.0);

    // without -k the manifest is removed
    ok(&env.deew(&["-i", "a.wav", "-dn", "-20", "-f", "ac4"]));
    assert!(
        env.run("dee_ac4_encoder")
            .contains(&"--input-manifest".to_string())
    );
    assert!(env.out().join("a.ac4").exists());
}

#[test]
fn dialnorm_shows_in_the_events() {
    let env = env_or_skip!();
    env.media("a.wav", "5.1", 48000, "pcm_s24le");
    let file = env.path("events.jsonl");
    let out = env.deew_env(
        &["-i", "a.wav", "-dn", "-27"],
        &[("DEEW_EVENTS_FILE", file.to_str().unwrap())],
    );
    ok(&out);
    let all = events(&file);
    let progress: Vec<_> = all.iter().filter(|e| e["type"] == "progress").collect();
    assert!(progress.iter().any(|e| e["stage"] == "measure"));
    assert!(
        progress
            .iter()
            .any(|e| e["stage"] == "encode" && e["db"] == -27),
        "{progress:?}"
    );
    // the measurement and the encode are steps of their own, 0 to 100
    let max_of = |stage: &str| {
        progress
            .iter()
            .filter(|e| e["stage"] == stage)
            .filter_map(|e| e["percent"].as_f64())
            .fold(0.0, f64::max)
    };
    assert_eq!(max_of("measure"), 100.0);
    assert_eq!(max_of("encode"), 100.0);
    // the encode starts again at 0
    let first_encode = progress.iter().find(|e| e["stage"] == "encode").unwrap();
    assert_eq!(first_encode["percent"], 0.0, "{first_encode:?}");
}

#[test]
fn ac4_dialnorm_needs_at_most_5_1() {
    let env = env_or_skip!();
    env.media("sur.wav", "7.1", 48000, "pcm_s24le");
    // AC4 still measures for -dn with dee_measure_loudness
    assert_error(
        &env.deew(&["-i", "sur.wav", "-f", "ac4", "-dn", "-27"]),
        "custom dialnorm (-dn) doesn't work for AC4 with 7.1 input, DEE's AC4 encoder downmixes it to 5.1 itself.",
    );
    // -mo measures 7.1 with dee_shim
    ok(&env.deew(&["-i", "sur.wav", "-mo"]));
    fs::remove_file(env.path("bin").join("dee_shim.args")).ok();
    // DD and DDP give the dialnorm to DEE, which needs no measurement
    for format in ["dd", "ddp"] {
        ok(&env.deew(&["-i", "sur.wav", "-f", format, "-dn", "-27"]));
        let encoder = env
            .dshim_run()
            .unwrap_or_else(|| env.run("dee_ddp_encoder"));
        assert!(
            encoder.contains(&"skip:dialnorm=-27".to_string()),
            "{encoder:?}"
        );
    }
}

#[test]
fn measure_only_runs_the_measurement_tool() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    let file = env.path("events.jsonl");
    // the encoders are not even needed
    for tool in DEE_TOOLS.iter().filter(|t| **t != "dee_measure_loudness") {
        fs::remove_file(env.dee().join(tool)).unwrap();
    }
    let out = env.deew_env(
        &["-i", "in.wav", "-mo", "-f", "thd"],
        &[("DEEW_EVENTS_FILE", file.to_str().unwrap())],
    );
    ok(&out);
    let text = stdout(&out);
    let args = env.run("dee_measure_loudness");
    assert_eq!(
        opt(&args, "-i"),
        env.temp().join("in.wav").to_str().unwrap()
    );
    // no output
    assert!(!env.out().join("in.thd").exists() && !env.out().join("in.ec3").exists());
    // the output section is not shown
    assert!(!text.contains("Dialnorm"), "{text}");
    assert!(!text.contains("Bitrate"), "{text}");
    assert!(temp_is_empty(&env));
    let all = events(&file);
    assert!(
        all.iter()
            .any(|e| e["type"] == "progress" && e["stage"] == "measure" && e["db"] == -23)
    );
    assert!(!all.iter().any(|e| e["stage"] == "encode"));
    assert!(
        text.contains("finished: in.wav (-23 dB, level-gated) - took "),
        "{text}"
    );
}

#[test]
fn measure_only_shows_the_real_loudness_but_encodes_keep_the_dialnorm_range() {
    let env = env_or_skip!();
    env.media("sur.wav", "7.1", 48000, "pcm_s24le");
    let loud = [("FAKE_LOUDNESS", "-33.0")];
    let out = env.deew_env(&["-i", "sur.wav", "-mo"], &loud);
    ok(&out);
    assert!(
        stdout(&out).contains("finished: sur.wav (-33 dB, level-gated) - took "),
        "{}",
        stdout(&out)
    );
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    let out = env.deew_env(&["-i", "in.wav"], &loud);
    ok(&out);
    assert!(stdout(&out).contains("finished: in.wav (-31 dB) - took "));
}

#[test]
fn measure_only_of_7_1_goes_through_dee_shim() {
    let env = env_or_skip!();
    env.media("sur.wav", "7.1", 48000, "pcm_s24le");
    let out = env.deew(&["-i", "sur.wav", "-mo"]);
    ok(&out);
    let text = stdout(&out);
    // dee_shim measure_loudness --layout 7.1 runs dee_measure_loudness, which exits by itself
    let shim = env.dshim_run().unwrap();
    assert_eq!(shim[0], "measure_loudness");
    assert_eq!(opt(&shim, "--layout"), "7.1");
    assert!(opt(&shim, "--binary").ends_with("dee_measure_loudness"));
    assert!(!has(&shim, "--unpadded-wav"));
    let args = env.run("dee_measure_loudness");
    assert_eq!(opt(&args, "--input-format"), "wav");
    assert!(opt(&args, "--output-manifest").ends_with("sur.loudness.json"));
    assert!(env.last_run("dee_ddp_encoder").is_none());
    assert!(
        text.contains("dee_shim measure_loudness --binary"),
        "{text}"
    );
    assert!(
        text.contains("finished: sur.wav (-23 dB, level-gated) - took "),
        "{text}"
    );
    assert!(
        text.contains("a 7.1 or 7.0 DDP encode sets its dialnorm from its 5.1 downmix"),
        "{text}"
    );
    assert!(!env.out().join("sur.ec3").exists());
    assert!(temp_is_empty(&env));
    // 5.1 does not need dee_shim
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    fs::remove_file(env.path("bin").join("dee_shim")).unwrap();
    let out51 = env.deew(&["-i", "in.wav", "-mo"]);
    ok(&out51);
    assert!(
        !stdout(&out51).contains("5.1 downmix"),
        "{}",
        stdout(&out51)
    );
    // 7.1 does
    assert_error(
        &env.deew(&["-i", "sur.wav", "-mo"]),
        "dee_shim is required for -mo of 7.1 and 7.0 input (DEE's own loudness meter refuses 8 channels), put dee_shim next to deew or in your PATH (https://github.com/truehdd/dee-shim)",
    );
}

#[test]
fn old_tools_are_refused_when_the_job_uses_them() {
    let env = env_or_skip!();
    env.media("sur.wav", "7.1", 48000, "pcm_s24le");
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    let shim = |version: &str| {
        write_script(
            &env.path("bin/dee_shim"),
            &FAKE_DSHIM.replace("v0.4.0+g790d050", version),
        );
    };
    // 7.1 DDP with the standard profile, and -mo of 7.1, run dee_shim
    shim("v0.3.0+gabcdef");
    for args in [&["-i", "sur.wav"][..], &["-i", "sur.wav", "-mo"][..]] {
        assert_error(
            &env.deew(args),
            "dee_shim 0.3.0+gabcdef is too old, deew needs 0.4.0 or newer: update it with -gt",
        );
    }
    // so does DD and DDP of a 5.0 source, on the pipe too
    env.media("five.wav", "5.0", 48000, "pcm_s24le");
    for pipe in [false, true] {
        env.set_pipe(pipe);
        assert_error(
            &env.deew(&["-i", "five.wav"]),
            "dee_shim 0.3.0+gabcdef is too old, deew needs 0.4.0 or newer: update it with -gt",
        );
    }
    env.set_pipe(false);
    ok(&env.deew(&["-i", "five.wav", "-dm", "2"]));
    // a job without dee_shim is not affected
    ok(&env.deew(&["-i", "in.wav"]));
    ok(&env.deew(&["-i", "sur.wav", "--ddp-mode", "bluray"]));
    // a version that can't be read
    write_script(&env.path("bin/dee_shim"), "#!/bin/sh\necho nothing\n");
    assert_error(
        &env.deew(&["-i", "sur.wav"]),
        "can't read the version of dee_shim, deew needs 0.4.0 or newer: update it with -gt",
    );
    // newer is fine, also with more digits
    shim("v0.10.1");
    ok(&env.deew(&["-i", "sur.wav"]));
    // truehdd: Atmos decoding runs it
    let truehdd = |version: &str| {
        write_script(
            &env.path("bin/truehdd"),
            &FAKE_TRUEHDD.replace("truehdd 0.6.3", &format!("truehdd {version}")),
        );
    };
    shim("v0.4.0+g790d050");
    env.truehd("atmos.thd");
    truehdd("0.6.2");
    assert_error(
        &env.deew(&["-i", "atmos.thd", "-f", "ddp-atmos"]),
        "truehdd 0.6.2 is too old, deew needs 0.6.3 or newer: update it with -gt",
    );
    write_script(&env.path("bin/truehdd"), "#!/bin/sh\nexit 0\n");
    assert_error(
        &env.deew(&["-i", "atmos.thd", "-f", "ddp-atmos"]),
        "can't read the version of truehdd, deew needs 0.6.3 or newer: update it with -gt",
    );
}

#[test]
fn delay_in_the_file_name() {
    let env = env_or_skip!();
    env.media("x DELAY -24ms.wav", "5.1", 48000, "pcm_s24le");
    ok(&env.deew(&["-i", "x DELAY -24ms.wav"]));
    let args = env.run("dee_ddp_encoder");
    // trimmed, 24 ms plus the 256 sample shift of the encoder; the output
    // name has no DELAY part
    assert_eq!(opt(&args, "--start"), "0.029333");
    assert!(!has(&args, "--add-silence"));
    assert!(env.out().join("x.ec3").exists());
}

#[test]
fn delay_argument_overrides() {
    let env = env_or_skip!();
    env.media("d DELAY -24ms.wav", "5.1", 48000, "pcm_s24le");
    let out = env.deew(&["-i", "d DELAY -24ms.wav", "-d=+10@pal"]);
    ok(&out);
    assert_eq!(summary_row(&stdout(&out), "Delay"), "400ms");
    let args = env.run("dee_ddp_encoder");
    assert_eq!(opt(&args, "--add-silence"), "0.394667:0");
    assert!(!has(&args, "--start"));
}

#[test]
fn delay_options_per_format() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    // no compensation for TrueHD
    ok(&env.deew(&["-i", "in.wav", "-f", "thd", "-d=-24ms"]));
    assert_eq!(opt(&env.run("dee_dthd_encoder"), "--start"), "0.024");
    ok(&env.deew(&["-i", "in.wav", "-f", "thd", "-d", "+0ms"]));
    let args = env.run("dee_dthd_encoder");
    assert!(!has(&args, "--start") && !has(&args, "--add-silence"));
    // AAC: 1600 samples (+32 for DEE's downmix of the 5.1), AC4: 2048 samples
    ok(&env.deew(&["-i", "in.wav", "-f", "aac", "-d", "+1s"]));
    assert_eq!(opt(&env.run("dee_aac_encoder"), "--add-silence"), "0.966:0");
    ok(&env.deew(&["-i", "in.wav", "-f", "aac", "-d", "+0ms"]));
    let args = env.run("dee_aac_encoder");
    assert_eq!(opt(&args, "--start"), "0.034");
    assert!(!has(&args, "--add-silence"));
    ok(&env.deew(&["-i", "in.wav", "-f", "ac4", "-d", "+0ms"]));
    let args = env.run("dee_ac4_encoder");
    assert_eq!(opt(&args, "--start"), "0.042667");
    assert!(!has(&args, "--add-silence"));
}

#[test]
fn failing_tool_is_a_job_error() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    let out = env.deew_env(
        &["-i", "in.wav"],
        &[
            ("FAKE_FAIL_TOOL", "dee_ddp_encoder"),
            ("FAKE_ERROR", "[boom] failed"),
        ],
    );
    assert_error(&out, "dee_ddp_encoder failed (exit code 1): [boom] failed");
    // the error is only reported once and the temp files are removed
    let text = stdout(&out);
    assert_eq!(text.matches("[boom] failed").count(), 1, "{text}");
    assert!(!env.out().join("in.ec3").exists());
    assert!(temp_is_empty(&env));
}

#[test]
fn failing_measurement_stops_before_the_encode() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    let out = env.deew_env(
        &["-i", "in.wav", "-f", "ac4", "-dn", "-27"],
        &[
            ("FAKE_FAIL_TOOL", "dee_measure_loudness"),
            ("FAKE_ERROR", "cannot measure"),
        ],
    );
    assert_error(
        &out,
        "dee_measure_loudness failed (exit code 1): cannot measure",
    );
    assert!(env.last_run("dee_ac4_encoder").is_none());
}

#[test]
fn one_failing_job_does_not_stop_the_others() {
    let env = env_or_skip!();
    env.media("a.wav", "5.1", 48000, "pcm_s24le");
    env.media("b.wav", "5.1", 48000, "pcm_s24le");
    // a tool that fails on the first input only
    let tool = env.dee().join("dee_ddp_encoder");
    fs::remove_file(&tool).unwrap();
    write_script(
        &tool,
        &FAKE_DEE.replace(
            "if [ \"$FAKE_FAIL_TOOL\" = \"$tool\" ]",
            "case \"$input\" in *a.wav) fail \"a is bad\" ;; esac\nif [ \"$FAKE_FAIL_TOOL\" = \"$tool\" ]",
        ),
    );
    let out = env.deew(&["-i", "a.wav", "b.wav", "-in", "1"]);
    assert_error(&out, "dee_ddp_encoder failed (exit code 1): a is bad");
    assert!(env.out().join("b.ec3").exists());
    assert!(!env.out().join("a.ec3").exists());
    // each job reports once, and the error is not repeated at the end
    let text = stdout(&out);
    assert_eq!(text.matches("a is bad").count(), 1, "{text}");
    assert!(
        text.contains("failed: a.wav - dee_ddp_encoder failed"),
        "{text}"
    );
    assert!(text.contains("finished: b.wav (-23 dB) - took "), "{text}");
    assert!(!text.contains("ERROR:"), "{text}");
    assert!(!text.contains("in queue"), "{text}");
}

#[test]
fn missing_output_is_an_error() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    let out = env.deew_env(&["-i", "in.wav"], &[("FAKE_NO_OUTPUT", "1")]);
    let output = env.out().join("in.ec3");
    assert_error(
        &out,
        &format!(
            "dee_ddp_encoder failed: the output file {} was not written.",
            output.display()
        ),
    );
}

#[test]
fn warnings_are_printed_once() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    let file = env.path("events.jsonl");
    let out = env.deew_env(
        &["-i", "in.wav"],
        &[
            ("FAKE_WARN", "clipping detected"),
            ("DEEW_EVENTS_FILE", file.to_str().unwrap()),
        ],
    );
    ok(&out);
    let text = stdout(&out);
    assert_eq!(
        text.matches("WARNING: clipping detected").count(),
        1,
        "{text}"
    );
    let all = events(&file);
    assert_eq!(
        all.iter()
            .filter(|e| e["type"] == "warning" && e["text"] == "clipping detected")
            .count(),
        1
    );
}

#[test]
fn existing_intermediate_is_reused() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    fs::create_dir_all(env.temp()).unwrap();
    fs::write(env.temp().join("in.wav"), b"already converted").unwrap();
    let out = env.deew(&["-i", "in.wav", "-k"]);
    ok(&out);
    let text = stdout(&out);
    assert!(
        text.contains("Intermediate already exists for the following file(s): in.wav"),
        "{text}"
    );
    assert_eq!(
        fs::read(env.temp().join("in.wav")).unwrap(),
        b"already converted"
    );
    assert!(env.out().join("in.ec3").exists());
}

#[test]
fn directory_input_is_sorted_and_filtered() {
    let env = env_or_skip!();
    fs::create_dir_all(env.work().join("album/sub")).unwrap();
    for name in ["album/b.wav", "album/a.wav", "album/.hidden.wav"] {
        let target = env.media("tmp.wav", "5.1", 48000, "pcm_s24le");
        fs::rename(target, env.work().join(name)).unwrap();
    }
    let out = env.deew(&["-i", "album", "-in", "2"]);
    ok(&out);
    let text = stdout(&out);
    assert!(text.contains("Files"), "{text}");
    assert!(env.out().join("a.ec3").exists() && env.out().join("b.ec3").exists());
    assert!(!env.out().join(".hidden.ec3").exists());
    let table_files = text.lines().find(|l| l.contains("Files")).unwrap();
    assert!(table_files.contains('2'), "{table_files}");
    assert!(temp_is_empty(&env));
}

#[test]
fn long_argument_prints_every_command() {
    let env = env_or_skip!();
    env.media("a.wav", "5.1", 48000, "pcm_s24le");
    env.media("b.wav", "5.1", 48000, "pcm_s24le");
    let out = env.deew(&["-i", "a.wav", "b.wav", "-la", "-f", "ac4", "-dn", "-27"]);
    ok(&out);
    let text = stdout(&out);
    let commands: Vec<&str> = text
        .lines()
        .filter(|l| l.starts_with("ffmpeg -y"))
        .collect();
    assert_eq!(commands.len(), 2, "{text}");
    assert!(
        commands[0].contains("-i a.wav -map 0:a:0 -c pcm_s24le"),
        "{}",
        commands[0]
    );
    assert!(commands[1].contains("-i b.wav "), "{}", commands[1]);
    // ffmpeg && measurement && encode
    assert_eq!(commands[0].matches(" && ").count(), 2, "{}", commands[0]);
    assert!(
        commands[0].contains(" && dee_measure_loudness --progress 1 "),
        "{}",
        commands[0]
    );
    assert!(
        commands[0].contains(" && dee_ac4_encoder --progress 1 "),
        "{}",
        commands[0]
    );
    assert!(commands[0].contains("--input-manifest "), "{}", commands[0]);
}

#[test]
fn short_print_has_placeholders() {
    let env = env_or_skip!();
    env.media("a.wav", "5.1", 48000, "pcm_s24le");
    let out = env.deew(&["-i", "a.wav"]);
    ok(&out);
    let text = stdout(&out);
    assert!(text.contains("ffmpeg -y -drc_scale 0 -i [input]"), "{text}");
    assert!(text.contains(" && dee_ddp_encoder --progress 1 --overwrite 1 --input-format wav -i [input] -o [output] -e ddp:drc_profile=none"), "{text}");
    env.truehd("atmos.thd");
    let out = env.deew(&["-i", "atmos.thd", "-f", "ac4-atmos"]);
    let text = stdout(&out);
    assert!(
        text.contains("truehdd decode --presentation 3 --json --progress --output-path [output] [input] && dee_ac4ajoc_encoder"),
        "{text}"
    );
}

#[test]
fn channel_mismatch_between_inputs() {
    let env = env_or_skip!();
    env.media("a.wav", "5.1", 48000, "pcm_s24le");
    env.media("b.wav", "stereo", 48000, "pcm_s24le");
    assert_error(
        &env.deew(&["-i", "a.wav", "b.wav"]),
        "each input has to have the same channel count:\n[6, 2]",
    );
}

#[test]
fn mixed_bit_depths_use_their_own_depth() {
    let env = env_or_skip!();
    env.media("a16.wav", "5.1", 48000, "pcm_s16le");
    env.media("b24.wav", "5.1", 48000, "pcm_s24le");
    let out = env.deew(&["-i", "a16.wav", "b24.wav", "-la"]);
    ok(&out);
    let text = stdout(&out);
    let commands: Vec<&str> = text
        .lines()
        .filter(|l| l.starts_with("ffmpeg -y"))
        .collect();
    assert_eq!(commands.len(), 2, "{text}");
    assert!(
        commands[0].contains("-i a16.wav -map 0:a:0 -c pcm_s16le "),
        "{}",
        commands[0]
    );
    assert!(
        commands[1].contains("-i b24.wav -map 0:a:0 -c pcm_s24le "),
        "{}",
        commands[1]
    );
    assert_eq!(summary_row(&text, "Bit depth"), "16/24");
    assert!(text.contains("Input(s)"), "{text}");

    // the short print lists the distinct depths
    let out = env.deew(&["-i", "a16.wav", "b24.wav"]);
    ok(&out);
    assert!(
        stdout(&out).contains("-c pcm_s16le/pcm_s24le "),
        "{}",
        stdout(&out)
    );

    // the same depth is shown as a number
    let out = env.deew(&["-i", "b24.wav"]);
    ok(&out);
    assert_eq!(summary_row(&stdout(&out), "Bit depth"), "24");
}

#[test]
fn delay_row_variants() {
    let env = env_or_skip!();
    for name in [
        "p.wav",
        "q.wav",
        "s DELAY -24ms.wav",
        "t DELAY -24ms.wav",
        "u DELAY +10ms.wav",
    ] {
        env.media(name, "5.1", 48000, "pcm_s24le");
    }
    let delay = |args: &[&str]| {
        let mut all = vec!["-i"];
        all.extend_from_slice(args);
        let out = env.deew(&all);
        ok(&out);
        summary_row(&stdout(&out), "Delay")
    };
    assert_eq!(delay(&["p.wav", "q.wav"]), "0ms");
    assert_eq!(
        delay(&["s DELAY -24ms.wav"]),
        "-24ms (parsed from filenames)"
    );
    assert_eq!(
        delay(&["s DELAY -24ms.wav", "t DELAY -24ms.wav"]),
        "-24ms (parsed from filenames)"
    );
    assert_eq!(
        delay(&["s DELAY -24ms.wav", "u DELAY +10ms.wav"]),
        "mixed (parsed from filenames)"
    );
    assert_eq!(
        delay(&["s DELAY -24ms.wav", "p.wav"]),
        "mixed (parsed from filenames)"
    );
    assert_eq!(delay(&["s DELAY -24ms.wav", "-d=-5.1ms"]), "-5.1ms");
    assert_eq!(delay(&["p.wav", "-d=+1,52s"]), "1520ms");
}

#[test]
fn bad_delay() {
    let env = env_or_skip!();
    env.media("a.wav", "5.1", 48000, "pcm_s24le");
    assert_error(
        &env.deew(&["-i", "a.wav", "-d", "10ms"]),
        "unsupported delay value.",
    );
    assert_error(
        &env.deew(&["-i", "a.wav", "-d=+10"]),
        "unsupported delay value.",
    );
}

#[test]
fn missing_input() {
    let env = env_or_skip!();
    assert_error(
        &env.deew(&["-i", "nope.wav"]),
        "path nope.wav does not exist.",
    );
}

#[test]
fn unusable_input_file() {
    let env = env_or_skip!();
    fs::write(env.work().join("junk.wav"), b"this is not audio").unwrap();
    assert_error(
        &env.deew(&["-i", "junk.wav"]),
        "junk.wav: ffprobe: Invalid data found when processing input",
    );
}

impl Env {
    /// Run deew (prompts on) with `input` on stdin.
    fn deew_stdin(&self, args: &[&str], input: &str) -> Output {
        use std::io::Write;
        let out = self.out();
        let mut all = vec!["-o", out.to_str().unwrap()];
        all.extend_from_slice(args);
        let mut child = spawn_retry(
            command(self.bin())
                .args(&all)
                .current_dir(self.work())
                .env("XDG_CONFIG_HOME", self.path("xdg"))
                .env_remove("NO_COLOR")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped()),
        );
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        child.wait_with_output().unwrap()
    }

    /// A folder `name` in the work directory with an `info.nfo` and a stereo wav.
    fn folder_with_nfo(&self, name: &str) {
        let dir = self.work().join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("info.nfo"), b"release notes\n").unwrap();
        let wav = self.media("s20.wav", "stereo", 48000, "pcm_s16le");
        fs::rename(wav, dir.join("s20.wav")).unwrap();
    }
}

#[test]
fn folder_with_a_non_audio_file_skips_it_with_np() {
    let env = env_or_skip!();
    env.folder_with_nfo("dir");
    let out = env.deew(&["-i", "dir"]);
    ok(&out);
    let text = stdout(&out);
    assert!(text.contains("1 file in dir can't be encoded:"), "{text}");
    assert!(text.contains("  info.nfo  no audio track"), "{text}");
    assert!(
        text.contains("Skipping it, encoding the other file."),
        "{text}"
    );
    assert!(env.out().join("s20.ec3").exists(), "{text}");
    assert!(!text.contains("ERROR"), "{text}");
}

#[test]
fn skipping_asks_and_n_stops() {
    let env = env_or_skip!();
    env.folder_with_nfo("dir");
    let out = env.deew_stdin(&["-i", "dir"], "n\n");
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    let text = stdout(&out);
    assert!(
        text.contains("Skip it and encode the other file? [y/n]:"),
        "{text}"
    );
    assert!(!env.out().join("s20.ec3").exists());
}

#[test]
fn skipping_asks_and_y_encodes() {
    let env = env_or_skip!();
    env.folder_with_nfo("dir");
    let out = env.deew_stdin(&["-i", "dir"], "y\ny\ny\n");
    ok(&out);
    assert!(env.out().join("s20.ec3").exists(), "{}", stdout(&out));
}

#[test]
fn folder_without_audio_files_is_an_error() {
    let env = env_or_skip!();
    let dir = env.work().join("dir");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join("info.nfo"), b"notes\n").unwrap();
    fs::write(dir.join("extras.zip"), b"PK\x03\x04 not really a zip").unwrap();
    let out = env.deew(&["-i", "dir"]);
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    let text = stdout(&out);
    assert!(
        text.contains("ERROR: no file in dir can be encoded:"),
        "{text}"
    );
    assert!(text.contains("  info.nfo  "), "{text}");
    assert!(text.contains("  extras.zip  "), "{text}");
}

#[test]
fn direct_non_audio_file_is_an_error() {
    let env = env_or_skip!();
    fs::write(env.work().join("info.nfo"), b"notes\n").unwrap();
    assert_error(
        &env.deew(&["-i", "info.nfo"]),
        "info.nfo has no audio track.",
    );
}

#[test]
fn missing_track_index_is_named() {
    let env = env_or_skip!();
    env.media("a.wav", "stereo", 48000, "pcm_s16le");
    env.media("b.wav", "stereo", 48000, "pcm_s16le");
    assert_error(
        &env.deew(&["-i", "a.wav", "-ti", "3"]),
        "a.wav has no audio track 3 (-ti), it has 1 (0).",
    );
    let out = env.deew(&["-i", "a.wav", "b.wav", "-ti", "3"]);
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    let text = stdout(&out);
    assert!(
        text.contains(
            "ERROR: 2 files have no audio track 3 (-ti 3):\n  a.wav  1 audio track (0)\n  b.wav  1 audio track (0)"
        ),
        "{text}"
    );
}

#[test]
fn missing_track_in_folders_shows_the_folder() {
    let env = env_or_skip!();
    env.folder_with_nfo("dir/");
    let out = env.deew(&["-i", "dir/", "-ti", "1"]);
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    let text = stdout(&out);
    assert!(
        text.contains("ERROR: dir/s20.wav has no audio track 1 (-ti), it has 1 (0)."),
        "{text}"
    );
}

#[test]
fn unreadable_file_names_the_reason() {
    use std::os::unix::fs::PermissionsExt;
    let env = env_or_skip!();
    let wav = env.media("noperm.wav", "stereo", 48000, "pcm_s16le");
    fs::set_permissions(&wav, fs::Permissions::from_mode(0o000)).unwrap();
    if fs::File::open(&wav).is_ok() {
        eprintln!("skipping: running as root");
        return;
    }
    assert_error(
        &env.deew(&["-i", "noperm.wav"]),
        "noperm.wav: ffprobe: Permission denied",
    );
}

#[test]
fn argument_errors() {
    let env = env_or_skip!();
    env.media("a.wav", "5.1", 48000, "pcm_s24le");
    assert_error(
        &env.deew(&["-i", "a.wav", "-f", "mp3"]),
        "-f/--format can only be dd, ddp, ddp-atmos, thd, thd-atmos, ac4, ac4-atmos, ac4-ims or aac.",
    );
    assert_error(
        &env.deew(&["-i", "a.wav", "-dm", "3"]),
        "-dm/--downmix can only be 1, 2 or 6.",
    );
    assert_error(
        &env.deew(&["-i", "a.wav", "-dm", "6"]),
        "downmix value has to be lower than the number of input channels.",
    );
    assert_error(
        &env.deew(&["-i", "a.wav", "-in", "many"]),
        "invalid number of instances: many",
    );
    assert_error(
        &env.deew(&["-i", "a.wav", "-p", "maybe"]),
        "-p/--pipe can only be auto, always or never.",
    );
}

#[test]
fn missing_binary() {
    let env = env_or_skip!();
    // ffmpeg is not next to deew, and not in the PATH
    let nowhere = env.path("empty");
    assert_error(
        &env.deew_env(&["-i", "a.wav"], &[("PATH", nowhere.to_str().unwrap())]),
        "ffmpeg does not exist.",
    );
}

#[test]
fn incomplete_config_is_rejected() {
    let env = env_or_skip!();
    fs::write(
        env.config(),
        "temp_path = ''\nlogo = 0\nmax_instances = 2\n",
    )
    .unwrap();
    assert_error(
        &env.deew(&["-i", "a.wav"]),
        "the following keys are missing from your config file: default_bitrates, summary_sections.\nUpdate your config file.",
    );
}

#[test]
fn removed_tool_keys_are_ignored() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    let config = fs::read_to_string(env.config()).unwrap();
    fs::write(
        env.config(),
        format!("ffmpeg_path = '/nonexistent/ffmpeg'\ndee_path = '/x'\n{config}"),
    )
    .unwrap();
    let out = env.deew(&["-i", "in.wav"]);
    ok(&out);
    assert!(!stdout(&out).contains("not supported anymore"));
    assert!(!stdout(&out).contains("WARNING"), "{}", stdout(&out));
    assert!(env.out().join("in.ec3").exists());
}

#[test]
fn unknown_duration_still_encodes() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    // an ffprobe that does not report durations
    let wrapper = env.path("bin/ffprobe");
    fs::write(
        &wrapper,
        "#!/bin/sh\nexec ffprobe \"$@\" | sed 's/\"duration\": \"[^\"]*\",//'\n",
    )
    .unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755)).unwrap();
    ok(&env.deew(&["-i", "in.wav"]));
    assert!(env.out().join("in.ec3").exists());
}

#[test]
fn declining_the_prompt_exits() {
    let env = env_or_skip!();
    env.media("a.wav", "stereo", 48000, "pcm_s16le");
    let mut child = spawn_retry(
        command(env.bin())
            .args(["-i", "a.wav", "-o", env.out().to_str().unwrap()])
            .current_dir(env.work())
            .env("XDG_CONFIG_HOME", env.path("xdg"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped()),
    );
    use std::io::Write;
    child.stdin.take().unwrap().write_all(b"n\n").unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    assert!(stdout(&out).contains("are you sure you want to use DEE?"));
    assert!(!env.out().join("a.ec3").exists());
}

/// Parse the lines of an events file.
fn events(path: &Path) -> Vec<serde_json::Value> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("{l}: {e}")))
        .collect()
}

#[test]
fn events_file_describes_an_encode() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    let file = env.path("events.jsonl");
    let out = env.deew_env(
        &["-i", "in.wav"],
        &[("DEEW_EVENTS_FILE", file.to_str().unwrap())],
    );
    ok(&out);
    let all = events(&file);
    assert_eq!(all[0]["type"], "hello");
    assert_eq!(all[0]["version"], env!("CARGO_PKG_VERSION"));
    assert_eq!(all[1]["type"], "jobs");
    assert_eq!(all[1]["files"], serde_json::json!(["in.wav"]));
    let progress: Vec<_> = all.iter().filter(|e| e["type"] == "progress").collect();
    assert!(progress.iter().all(|e| e["job"] == 0));
    assert_eq!(progress[0]["stage"], "starting");
    assert!(progress.iter().any(|e| e["stage"] == "ffmpeg"));
    assert!(progress.iter().any(|e| e["stage"] == "measure"));
    assert!(
        progress
            .iter()
            .any(|e| e["stage"] == "encode" && e["db"] == -23)
    );
    assert!(!progress.iter().any(|e| e["stage"] == "decode"));
    assert!(progress.iter().any(|e| e["percent"] == 100.0));
    // DEE's one percentage over two steps becomes 0 to 100 per step (the
    // events are throttled, so the values in between may be missing)
    let percents = |stage: &str| -> Vec<f64> {
        progress
            .iter()
            .filter(|e| e["stage"] == stage)
            .filter_map(|e| e["percent"].as_f64())
            .collect()
    };
    let measure = percents("measure");
    assert!(measure.contains(&100.0), "{measure:?}");
    let encode = percents("encode");
    assert_eq!(encode[0], 0.0, "{encode:?}");
    assert!(encode.contains(&100.0), "{encode:?}");
    let last = all.last().unwrap();
    assert_eq!(last["type"], "done");
    assert_eq!(last["job"], 0);
    assert_eq!(last["result"], "finished");
    assert!(last["took"].is_u64(), "{last}");
    assert!(last.get("error").is_none());
    // the result line reaches the terminal log
    assert!(
        stdout(&out).contains("finished: in.wav (-23 dB) - took "),
        "{}",
        stdout(&out)
    );
}

#[test]
fn events_file_has_a_decode_stage_for_atmos() {
    let env = env_or_skip!();
    env.truehd("atmos.thd");
    let file = env.path("events.jsonl");
    let out = env.deew_env(
        &["-i", "atmos.thd", "-f", "ddp-atmos"],
        &[("DEEW_EVENTS_FILE", file.to_str().unwrap())],
    );
    ok(&out);
    let all = events(&file);
    let stages: Vec<&str> = all
        .iter()
        .filter(|e| e["type"] == "progress")
        .filter_map(|e| e["stage"].as_str())
        .collect();
    let first = |stage: &str| stages.iter().position(|s| *s == stage).unwrap();
    assert!(!stages.contains(&"ffmpeg"));
    assert!(
        first("starting") < first("decode")
            && first("decode") < first("measure")
            && first("measure") < first("encode"),
        "{stages:?}"
    );
}

#[test]
fn events_file_records_errors() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    let file = env.path("events.jsonl");
    let out = env.deew_env(
        &["-i", "in.wav"],
        &[
            ("DEEW_EVENTS_FILE", file.to_str().unwrap()),
            ("FAKE_FAIL_TOOL", "dee_ddp_encoder"),
            ("FAKE_ERROR", "boom"),
        ],
    );
    assert_eq!(out.status.code(), Some(1));
    let all = events(&file);
    assert!(
        all.iter()
            .any(|e| e["type"] == "error"
                && e["text"] == "dee_ddp_encoder failed (exit code 1): boom")
    );
    assert_eq!(
        all.last().unwrap(),
        &serde_json::json!({"type": "error", "text": "dee_ddp_encoder failed (exit code 1): boom"})
    );
}

#[test]
fn events_file_records_prompt_and_answer() {
    let env = env_or_skip!();
    env.media("a.wav", "stereo", 48000, "pcm_s16le");
    let file = env.path("events.jsonl");
    let mut child = spawn_retry(
        command(env.bin())
            .args(["-o", env.out().to_str().unwrap(), "-i", "a.wav"])
            .current_dir(env.work())
            .env("XDG_CONFIG_HOME", env.path("xdg"))
            .env("DEEW_EVENTS_FILE", &file)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()),
    );
    std::io::Write::write_all(&mut child.stdin.take().unwrap(), b"n\n").unwrap();
    let out = child.wait_with_output().unwrap();
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    let all = events(&file);
    assert_eq!(all[1]["type"], "prompt");
    assert!(
        all[1]["text"]
            .as_str()
            .unwrap()
            .starts_with("Consider using qaac or opus for mono and stereo encoding,")
    );
    assert_eq!(all[2], serde_json::json!({"type": "answered"}));
}

#[test]
fn container_truehd_is_piped_from_ffmpeg_into_truehdd() {
    let env = env_or_skip!();
    env.mka("atmos.mka", false);
    let out = env.deew(&["-i", "atmos.mka", "-f", "ddp-atmos", "-la"]);
    ok(&out);
    let text = stdout(&out);
    assert!(
        text.contains(
            "ffmpeg -v error -nostdin -i atmos.mka -map 0:a:0 -c copy -f truehd - | truehdd decode "
        ) && text.contains("--output-path "),
        "{text}"
    );
    // truehdd reads stdin, which got the stream
    assert_eq!(env.truehdd_run().unwrap().last().unwrap(), "-");
    let bytes: u64 = fs::read_to_string(env.path("bin").join("truehdd.stdin"))
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert!(bytes > 100, "{bytes}");
    assert!(env.out().join("atmos.ec3").exists());
    assert!(temp_is_empty(&env));
    let out = env.deew(&["-i", "atmos.mka", "-f", "ddp-atmos"]);
    assert!(stdout(&out).contains("-i [input] -map 0:a:0 -c copy -f truehd - | truehdd decode --presentation 3 --json --output-path [output] -"));
}

#[test]
fn raw_truehd_goes_to_truehdd_directly() {
    let env = env_or_skip!();
    env.truehd("atmos.thd");
    let out = env.deew(&["-i", "atmos.thd", "-f", "ddp-atmos", "-la"]);
    ok(&out);
    let text = stdout(&out);
    assert!(
        !text.contains("-f truehd") && !text.contains(" - | "),
        "{text}"
    );
    assert!(
        text.contains("truehdd decode --presentation 3 --json --progress --output-path "),
        "{text}"
    );
    assert_eq!(env.truehdd_run().unwrap().last().unwrap(), "atmos.thd");
    assert!(!env.path("bin").join("truehdd.stdin").exists());
    assert!(env.out().join("atmos.ec3").exists());
    let out = env.deew(&["-i", "atmos.thd", "-f", "ac4-atmos"]);
    assert!(stdout(&out).contains("truehdd decode --presentation 3 --json --progress --output-path [output] [input] && dee_ac4ajoc_encoder"));
}

#[test]
fn dee_failure_on_stdout_without_error_prefix_is_reported() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    let out = env.deew_env(
        &["-i", "in.wav"],
        &[
            ("FAKE_FAIL_TOOL", "dee_ddp_encoder"),
            ("FAKE_STDOUT_FAIL", "1"),
        ],
    );
    assert_error(
        &out,
        "dee_ddp_encoder failed (exit code 1): Could not build workflow.",
    );
}

#[test]
fn ffmpeg_failure_names_the_error_and_the_full_temp_disk() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    let wrapper = env.path("bin/ffmpeg");
    write_script(
        &wrapper,
        "#!/bin/sh\ncase \"$*\" in *-rf64*) echo '[out#0/wav @ 0x5da79dccbe40] Error closing file: No space left on device' >&2; echo 'Conversion failed!' >&2; exit 228 ;; esac\nexec ffmpeg \"$@\"\n",
    );
    let out = env.deew(&["-i", "in.wav"]);
    let text = stdout(&out);
    assert!(
        text.contains("ffmpeg failed (exit code 228): Error closing file: No space left on device (temp path: "),
        "{text}"
    );
}

#[test]
fn failing_ffmpeg_in_the_pipe_is_a_job_error() {
    let env = env_or_skip!();
    env.mka("atmos.mka", false);
    // a file that ffprobe accepts but ffmpeg cannot read as the selected track
    let wrapper = env.path("bin/ffmpeg");
    write_script(
        &wrapper,
        "#!/bin/sh\ncase \"$*\" in *truehd*) echo 'broken input' >&2; exit 1 ;; esac\nexec ffmpeg \"$@\"\n",
    );
    let out = env.deew(&["-i", "atmos.mka", "-f", "ddp-atmos"]);
    assert_error(&out, "ffmpeg failed (exit code 1): broken input");
    assert!(env.last_run("dee_ddpjoc_encoder").is_none());
    assert!(temp_is_empty(&env));
}

#[test]
fn track_index_selects_the_truehd_track() {
    let env = env_or_skip!();
    env.mka("two.mka", true);
    // the first track is AC-3
    assert_error(
        &env.deew(&["-i", "two.mka", "-f", "ddp-atmos"]),
        "ddp-atmos, thd-atmos and ac4-atmos need a TrueHD Atmos, DTS:X or Auro-3D (24-bit DTS-HD MA, FLAC, PCM or TrueHD with a harletty that reads PCM) input or an Atmos master, two.mka is none of them.",
    );
    ok(&env.deew(&["-i", "two.mka", "-f", "ddp-atmos", "-ti", "1", "-k"]));
    let kept = fs::read_to_string(env.path("bin").join("truehdd.stdin")).unwrap();
    assert!(kept.trim().parse::<u64>().unwrap() > 100);
    assert!(env.out().join("two.ec3").exists());
}

#[test]
fn dtsx_file_goes_through_harletty_into_ddp_atmos() {
    let env = env_or_skip!();
    env.dts("x.dts");
    let out = env.deew(&["-i", "x.dts", "-f", "ddp-atmos"]);
    ok(&out);
    let text = stdout(&out);
    assert_eq!(summary_row(&text, "harletty version"), "0.9.0");
    assert_eq!(summary_row(&text, "harletty"), "bed conform on");
    // the experimental presentation is a warning
    assert!(
        text.contains("WARNING: x.dts: DTS:X-7.1.4+3 (ObjectsD0) is decoded experimentally by harletty, check the result."),
        "{text}"
    );
    // a raw file is detected and decoded directly
    let info = env.harletty_run("info").unwrap();
    assert_eq!(info, ["--loglevel", "off", "info", "--json", "x.dts"]);
    let decode = env.harletty_run("decode").unwrap();
    assert_eq!(
        decode[..9],
        [
            "--log-format",
            "json",
            "--loglevel",
            "info",
            "decode",
            "--codec",
            "dts",
            "--bed-conform",
            "--output-path"
        ]
    );
    assert_eq!(
        opt(&decode, "--output-path"),
        env.temp().join("x.bc").to_str().unwrap()
    );
    assert_eq!(decode.last().unwrap(), "x.dts");
    assert!(!has(&decode, "--warp-mode") && !has(&decode, "--progress"));
    assert!(env.truehdd_run().is_none());
    // from the .atmos on it is a TrueHD Atmos source
    let shim = env.dshim_run().unwrap();
    assert_eq!(shim[0], "ddpjoc_encoder");
    let rest: Vec<String> = shim[shim.iter().position(|a| a == "--").unwrap() + 1..].to_vec();
    assert_eq!(opt(&rest, "--input-format"), "atmos_mezz");
    assert_eq!(
        opt(&rest, "-i"),
        env.temp().join("x.bc.atmos").to_str().unwrap()
    );
    assert!(env.out().join("x.ec3").exists());
    // all three .atmos files are gone
    assert!(temp_is_empty(&env));
}

#[test]
fn dts_track_of_a_container_is_piped_into_harletty() {
    let env = env_or_skip!();
    env.dts_mka("x.mka");
    for format in ["ddp-atmos", "thd-atmos", "ac4-atmos"] {
        let out = env.deew_env(
            &["-i", "x.mka", "-f", format, "-wm", "loro", "-bc", "off"],
            &[("FAKE_SPATIAL", "auro")],
        );
        ok(&out);
        let text = stdout(&out);
        assert_eq!(
            summary_row(&text, "harletty"),
            "warp mode loro, bed conform on",
            "{format}"
        );
        // Auro-3D is not experimental
        assert!(!text.contains("experimentally"), "{text}");
        // a track is copied into harletty, for the detection and the decode
        let info = env.harletty_run("info").unwrap();
        assert_eq!(info.last().unwrap(), "-");
        let decode = env.harletty_run("decode").unwrap();
        assert_eq!(decode.last().unwrap(), "-");
        assert_eq!(opt(&decode, "--warp-mode"), "loro");
        assert!(has(&decode, "--bed-conform"));
        assert_eq!(
            opt(&decode, "--output-path"),
            env.temp().join("x.wm-loro.bc").to_str().unwrap()
        );
        let bytes: u64 = fs::read_to_string(env.path("bin/harletty.stdin"))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert!(bytes > 1000, "{bytes}");
        assert!(env.truehdd_run().is_none());
        assert!(temp_is_empty(&env), "{format}");
    }
}

#[test]
fn dts_without_dtsx_is_no_atmos_source() {
    let env = env_or_skip!();
    env.dts("x.dts");
    for format in ["ddp-atmos", "thd-atmos", "ac4-atmos"] {
        let out = env.deew_env(&["-i", "x.dts", "-f", format], &[("FAKE_SPATIAL", "plain")]);
        assert_error(
            &out,
            "ddp-atmos, thd-atmos and ac4-atmos need a TrueHD Atmos, DTS:X or Auro-3D (24-bit DTS-HD MA, FLAC, PCM or TrueHD with a harletty that reads PCM) input or an Atmos master, x.dts has no Atmos, DTS:X or Auro-3D.",
        );
    }
    assert!(env.harletty_run("decode").is_none());
    assert!(env.truehdd_run().is_none());
    assert!(temp_is_empty(&env));
}

#[test]
fn harletty_is_only_used_for_the_atmos_formats() {
    let env = env_or_skip!();
    env.dts("x.dts");
    for args in [
        vec!["-f", "ddp"],
        vec!["-f", "dd"],
        vec!["-f", "thd"],
        vec!["-f", "ac4"],
        vec!["-f", "aac"],
        vec!["-mo"],
    ] {
        let mut all = vec!["-i", "x.dts"];
        all.extend(args.iter());
        let out = env.deew(&all);
        ok(&out);
        assert!(env.harletty_run("info").is_none(), "{args:?}");
        assert!(env.harletty_run("decode").is_none(), "{args:?}");
        assert!(env.truehdd_run().is_none(), "{args:?}");
    }
    // not even without harletty
    fs::remove_file(env.path("bin/harletty")).unwrap();
    ok(&env.deew(&["-i", "x.dts", "-f", "ddp"]));
}

#[test]
fn harletty_is_required_for_dts_of_the_atmos_formats() {
    let env = env_or_skip!();
    env.dts("x.dts");
    fs::remove_file(env.path("bin/harletty")).unwrap();
    let out = env.deew(&["-i", "x.dts", "-f", "ddp-atmos"]);
    assert_error(
        &out,
        "harletty is required for DTS:X and Auro-3D sources of the Atmos formats and AC-4 IMS (to read and decode them), put harletty next to deew or in your PATH, or get it with -gt (https://github.com/harletty/harletty-bridge)",
    );
    // a TrueHD source needs no harletty
    env.truehd("atmos.thd");
    ok(&env.deew(&["-i", "atmos.thd", "-f", "ddp-atmos"]));
}

#[test]
fn old_harletty_is_rejected() {
    let env = env_or_skip!();
    env.dts("x.dts");
    let out = env.deew_env(
        &["-i", "x.dts", "-f", "ddp-atmos"],
        &[("FAKE_HARLETTY_VERSION", "0.8.2")],
    );
    assert_error(
        &out,
        "harletty 0.8.2 is too old, deew needs 0.9.0 or newer: update it with -gt",
    );
    assert!(env.harletty_run("info").is_none());
    let out = env.deew_env(
        &["-i", "x.dts", "-f", "ddp-atmos"],
        &[("FAKE_HARLETTY_VERSION", "junk")],
    );
    assert_error(
        &out,
        "can't read the version of harletty, deew needs 0.9.0 or newer: update it with -gt",
    );
}

#[test]
fn harletty_info_failures_name_the_file() {
    let env = env_or_skip!();
    env.dts("x.dts");
    let out = env.deew_env(
        &["-i", "x.dts", "-f", "ddp-atmos"],
        &[("FAKE_SPATIAL", "error")],
    );
    assert_error(
        &out,
        "harletty couldn't read x.dts: Failed to open input: boom",
    );
    let out = env.deew_env(
        &["-i", "x.dts", "-f", "ddp-atmos"],
        &[("FAKE_SPATIAL", "nocodec")],
    );
    assert_error(&out, "harletty couldn't read x.dts: no DTS frame found");
    assert!(env.harletty_run("decode").is_none());
}

#[test]
fn harletty_failure_is_reported() {
    let env = env_or_skip!();
    env.dts("x.dts");
    let out = env.deew_env(
        &["-i", "x.dts", "-f", "ddp-atmos"],
        &[("FAKE_FAIL_TOOL", "harletty")],
    );
    assert_error(&out, "harletty failed (exit code 1): bad stream");
    assert!(env.last_run("dee_ddpjoc_encoder").is_none());
    assert!(temp_is_empty(&env));
}

#[test]
fn harletty_warnings_and_short_decodes() {
    let env = env_or_skip!();
    env.dts_mka("x.mka");
    let out = env.deew_env(&["-i", "x.mka", "-f", "ddp-atmos"], &[("FAKE_WARN", "1")]);
    ok(&out);
    let text = stdout(&out);
    assert!(
        text.contains("WARNING: harletty reported problems in x.mka: DTS core decode error (dropping frame): Err(x). The output may have a gap"),
        "{text}"
    );
    // the experimental notice of the log is not repeated
    assert_eq!(text.matches("experimental").count(), 1, "{text}");
    // far less audio than the input has
    fs::remove_file(env.out().join("x.ec3")).unwrap();
    let out = env.deew_env(
        &["-i", "x.mka", "-f", "ddp-atmos"],
        &[("FAKE_SAMPLES", "9600")],
    );
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    assert!(
        stdout(&out).contains(" - harletty decoded only 0.2 s of 2."),
        "{}",
        stdout(&out)
    );
    assert!(!env.out().join("x.ec3").exists());
    assert!(temp_is_empty(&env));
}

#[test]
fn dtsx_and_truehd_sources_mix() {
    let env = env_or_skip!();
    env.dts("x.dts");
    env.truehd("atmos.thd");
    let out = env.deew(&["-i", "x.dts", "atmos.thd", "-f", "thd-atmos"]);
    ok(&out);
    assert!(env.harletty_run("decode").is_some());
    assert!(env.truehdd_run().is_some());
    assert!(temp_is_empty(&env));
}

#[test]
fn decode_that_is_too_short_is_an_error() {
    let env = env_or_skip!();
    env.mka("atmos.mka", false);
    let out = env.deew_env(
        &["-i", "atmos.mka", "-f", "ddp-atmos"],
        &[("FAKE_SHORT", "1")],
    );
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    assert!(
        stdout(&out).contains("failed: ")
            && stdout(&out).contains(" - truehdd decoded only 0.2 s of 2."),
        "{}",
        stdout(&out)
    );
    assert!(env.last_run("dee_ddpjoc_encoder").is_none());
    assert!(!env.out().join("atmos.ec3").exists());
    assert!(temp_is_empty(&env));
}

fn read_log(env: &Env, name: &str) -> String {
    fs::read_to_string(env.out().join(name)).unwrap_or_else(|e| panic!("no {name}: {e}"))
}

#[test]
fn keep_log_writes_a_report() {
    let env = env_or_skip!();
    env.media("a & <b>.wav", "5.1", 48000, "pcm_s24le");
    ok(&env.deew(&["-i", "a & <b>.wav", "-kl", "-f", "ac4", "-dn", "-27"]));
    let html = read_log(&env, "a & <b>.deew-log.html");
    // the status and the file name, escaped
    assert!(html.contains("a &amp; &lt;b&gt;.wav"), "{html}");
    assert!(!html.contains("<b>"));
    assert!(html.contains("badge ok"));
    // every stage with its command and the output of the tool
    for stage in ["ffmpeg", "measure", "encode"] {
        assert!(
            html.contains(&format!("<span class=\"stage-name\">{stage}</span>")),
            "{stage}"
        );
    }
    assert!(html.contains("dee_measure_loudness --progress 1"));
    assert!(html.contains("dee_ac4_encoder --progress 1"));
    assert!(html.contains("--input-manifest"));
    assert!(html.contains("measured_loudness=-23.4"));
    assert!(html.contains("measured loudness -23 dB") || html.contains("-23 dB"));
    // the progress lines are collapsed to the last state
    assert!(html.contains("[==========&gt;] 100 %"));
    assert!(!html.contains("] 12 %"));
    // versions, settings
    assert!(html.contains("6.7.0") && html.contains("0.6.3"));
    assert!(html.contains("<td>DRC</td><td class=\"mono\">music_light</td>"));
    assert!(html.contains("-27 dB"));
    // output
    assert!(
        html.contains("fake dee_ddp_encoder".len().to_string().as_str()) || html.contains("Size")
    );
    // no external references
    for needle in ["<link", "src=\"http", "@import", "url(http"] {
        assert!(!html.contains(needle), "{needle}");
    }
    assert!(!env.out().join("a & <b>.thd.log.html").exists());
}

#[test]
fn without_keep_log_there_is_no_report() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    ok(&env.deew(&["-i", "in.wav"]));
    ok(&env.deew(&["-i", "in.wav", "-f", "thd", "-r", "speech"]));
    let names: Vec<String> = fs::read_dir(env.out())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert!(!names.iter().any(|n| n.contains("log")), "{names:?}");
}

#[test]
fn keep_log_keeps_the_html_log_of_truehd() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    ok(&env.deew(&["-i", "in.wav", "-f", "thd", "-r", "speech", "-kl"]));
    assert!(env.out().join("in.thd.log.html").exists());
    assert!(!env.out().join("in.thd.mll").exists());
    assert!(env.out().join("in.deew-log.html").exists());
    let html = read_log(&env, "in.deew-log.html");
    assert!(html.contains("dee_dthd_encoder --progress 1"));
    assert!(html.contains("<td>dee_shim used</td><td class=\"mono\">no</td>"));
    // through dee_shim: the shim and its glued output are in the report
    ok(&env.deew(&["-i", "in.wav", "-f", "thd", "-kl"]));
    let html = read_log(&env, "in.deew-log.html");
    assert!(html.contains("dee_shim dthd_encoder --binary"));
    assert!(html.contains("<td>dee_shim used</td><td class=\"mono\">yes</td>"));
    assert!(html.contains("dshim ok   fake patch"));
}

#[test]
fn failing_job_gets_a_report_with_the_error() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    let out = env.deew_env(
        &["-i", "in.wav", "-kl"],
        &[
            ("FAKE_FAIL_TOOL", "dee_ddp_encoder"),
            ("FAKE_ERROR", "boom <1>"),
        ],
    );
    assert_eq!(out.status.code(), Some(1));
    let html = read_log(&env, "in.deew-log.html");
    assert!(html.contains("badge fail"));
    assert!(
        html.contains("dee_ddp_encoder failed (exit code 1): boom &lt;1&gt;"),
        "{html}"
    );
    assert!(html.contains("exit 1"));
    assert!(html.contains("Summary of errors and warnings"));
    assert!(temp_is_empty(&env));
}

#[test]
fn measure_only_report_is_named_after_the_input() {
    let env = env_or_skip!();
    env.media("in DELAY -5ms.wav", "5.1", 48000, "pcm_s24le");
    ok(&env.deew(&["-i", "in DELAY -5ms.wav", "-mo", "-kl"]));
    let html = read_log(&env, "in.deew-log.html");
    assert!(html.contains("dee_measure_loudness --progress 1"));
    assert!(!html.contains("dee_ddp_encoder"));
    assert!(html.contains("Measured loudness"));
    assert!(html.contains("-23 dB, level-gated"), "{html}");
    // the settings are about the measurement, not an encode
    assert!(html.contains("Measure only"));
    assert!(!html.contains("DDP</td>") && !html.contains("Bitrate"));
}

#[test]
fn every_job_gets_its_own_report() {
    let env = env_or_skip!();
    for name in ["a.wav", "b.wav", "c.wav"] {
        env.media(name, "5.1", 48000, "pcm_s24le");
    }
    ok(&env.deew(&["-i", "a.wav", "b.wav", "c.wav", "-kl", "-in", "3"]));
    for name in ["a", "b", "c"] {
        let html = read_log(&env, &format!("{name}.deew-log.html"));
        assert!(html.contains(&format!("<h1>{name}.wav</h1>")), "{name}");
        assert!(html.contains(&format!("{name}.wav")));
    }
}

#[test]
fn atmos_report_has_the_decode_stage() {
    let env = env_or_skip!();
    env.mka("atmos.mka", false);
    ok(&env.deew(&["-i", "atmos.mka", "-f", "ddp-atmos", "-kl"]));
    let html = read_log(&env, "atmos.deew-log.html");
    assert!(html.contains("<span class=\"stage-name\">decode</span>"));
    assert!(html.contains("ffmpeg -v error -nostdin -i atmos.mka"));
    assert!(html.contains(" - | ") && html.contains("/truehdd decode "));
    assert!(html.contains("truehdd stdout"));
    assert!(html.contains("dee_shim ddpjoc_encoder"));
}

#[test]
fn thd_atmos_runs_dthd_through_dee_shim() {
    let env = env_or_skip!();
    env.mka("atmos.mka", false);
    let out = env.deew(&["-i", "atmos.mka", "-f", "thd-atmos", "-r", "speech", "-la"]);
    ok(&out);
    let text = stdout(&out);
    assert_eq!(summary_row(&text, "Format"), "TrueHD Atmos");
    assert_eq!(summary_row(&text, "Bitrate"), "N/A");
    assert_eq!(last_summary_row(&text, "Channels"), "Atmos");
    // decode like the other Atmos formats, then dthd through the shim
    assert!(text.contains(" - | truehdd decode "), "{text}");
    let shim = env.dshim_run().unwrap();
    assert_eq!(shim[0], "dthd_encoder");
    assert_eq!(
        opt(&shim, "--binary"),
        env.dee().join("dee_dthd_encoder").to_str().unwrap()
    );
    assert_eq!(opt(&shim, "--oamd-as-is"), "on");
    assert_eq!(opt(&shim, "--as-is"), "on");
    assert_eq!(opt(&shim, "--drc"), "speech");
    let args = env.run("dee_dthd_encoder");
    let rest: Vec<String> = shim[shim.iter().position(|a| a == "--").unwrap() + 1..].to_vec();
    assert_eq!(rest, args);
    assert_eq!(
        opt(&args, "--presentation"),
        "atmos:number_of_elements=16:legacy_authoring=0"
    );
    assert_eq!(opt(&args, "--input-format"), "atmos_mezz");
    assert_eq!(
        opt(&args, "-i"),
        env.temp().join("atmos.atmos").to_str().unwrap()
    );
    assert_eq!(
        opt(&args, "-o"),
        env.out().join("atmos.thd").to_str().unwrap()
    );
    // the TrueHD encoder takes a temp dir; no delay compensation
    assert_dee_temp(&env, opt(&args, "--temp-dir"));
    assert!(!has(&args, "--start") && !has(&args, "--add-silence"));
    assert!(env.out().join("atmos.thd").exists());
    // DEE's log and mll are cleaned up, the temp dir is empty
    assert!(!env.out().join("atmos.thd.log.html").exists());
    assert!(!env.out().join("atmos.thd.mll").exists());
    assert!(temp_is_empty(&env));
}

#[test]
fn thd_atmos_defaults_to_no_drc_and_keeps_the_log() {
    let env = env_or_skip!();
    env.truehd("atmos.thd");
    ok(&env.deew(&["-i", "atmos.thd", "-f", "thd-atmos", "-kl", "-d", "+1s"]));
    let shim = env.dshim_run().unwrap();
    assert_eq!(opt(&shim, "--drc"), "none");
    assert_eq!(opt(&env.run("dee_dthd_encoder"), "--add-silence"), "1:0");
    assert!(env.out().join("atmos.thd.log.html").exists());
    assert!(!env.out().join("atmos.thd.mll").exists());
    assert!(env.out().join("atmos.deew-log.html").exists());
}

#[test]
fn thd_atmos_needs_its_inputs_and_tools() {
    let env = env_or_skip!();
    env.truehd("atmos.thd");
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    assert_error(
        &env.deew(&["-i", "atmos.thd", "-f", "thd-atmos", "-dm", "2"]),
        "-dm/--downmix only works for DD/DDP/AC4/AAC encoding and for TrueHD (2 or 6), not for the Atmos formats.",
    );
    ok(&env.deew(&["-i", "atmos.thd", "-f", "thd-atmos", "-dn", "-3"]));
    let shim = env.dshim_run().unwrap();
    let encoder = env.run("dee_dthd_encoder");
    let dash = shim.iter().position(|a| a == "--").unwrap();
    let at = shim.iter().position(|a| a == "skip:dialnorm=-3").unwrap();
    assert!(at > dash && shim[at - 1] == "--loudness-management");
    assert!(env.last_run("dee_measure_loudness").is_none());
    drop(encoder);
    let out = env.deew_env(
        &["-i", "atmos.thd", "-f", "thd-atmos"],
        &[("FAKE_NO_ATMOS", "1")],
    );
    assert_error(&out, "the input has no Atmos presentation.");
    assert!(temp_is_empty(&env));
    // dee_shim is required, no fallback
    env.set_shim(false);
    assert_error(
        &env.deew(&["-i", "atmos.thd", "-f", "thd-atmos"]),
        "dee_shim is required for thd-atmos, put dee_shim next to deew or in your PATH (https://github.com/truehdd/dee-shim)",
    );
    // the encoder is the TrueHD one
    fs::remove_file(env.dee().join("dee_dthd_encoder")).unwrap();
    env.set_shim(true);
    let out = env.deew(&["-i", "atmos.thd", "-f", "thd-atmos"]);
    assert_error(&out, "dee_dthd_encoder does not exist.");
    assert!(stdout(&out).contains("dthd-mac2linux"));
}

#[test]
fn programs_next_to_deew_are_used_first() {
    let env = env_or_skip!();
    env.truehd("atmos.thd");
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    // the programs are next to the deew executable
    let bin = env.path("bin");
    // an ffmpeg next to deew that leaves a mark before it runs the real one
    let marker = env.path("ffmpeg-ran");
    write_script(
        &bin.join("ffmpeg"),
        &format!(
            "#!/bin/sh\necho ran >> '{}'\nexec ffmpeg \"$@\"\n",
            marker.display()
        ),
    );
    let out = env.deew(&["-i", "in.wav", "-la"]);
    ok(&out);
    assert!(marker.exists(), "{}", stdout(&out));
    assert!(env.last_run("dee_ddp_encoder").is_some());
    let out = env.deew(&["-i", "atmos.thd", "-f", "ddp-atmos"]);
    ok(&out);
    // the scripts write next to the (linked) path they were started by
    assert!(bin.join("dee_shim.args").exists() && bin.join("truehdd.args").exists());
    assert_eq!(
        summary_row(&stdout(&out), "dee_shim version"),
        "0.4.0+g790d050"
    );
}

#[test]
fn five_and_seven_channel_inputs_get_a_silent_lfe_slot() {
    let env = env_or_skip!();
    // 5.0: a real 5.0 DDP by the 5.0 layout of dee_shim, the wav has the 5 channels
    env.media("five.wav", "5.0", 48000, "pcm_s24le");
    let out = env.deew(&["-i", "five.wav", "-k", "-la"]);
    ok(&out);
    let text = stdout(&out);
    assert!(
        text.contains("pan=5.0|c0=c0|c1=c1|c2=c2|c3=c3|c4=c4"),
        "{text}"
    );
    assert_eq!(
        ffprobe_field(&env.temp().join("five.5ch.wav"), "channels"),
        "5"
    );
    assert_eq!(last_summary_row(&text, "Channels"), "5.0");
    let shim = env.dshim_run().unwrap();
    assert_eq!(opt(&shim, "--layout"), "5.0");
    assert!(has(&shim, "--unpadded-wav"));
    assert!(!has(&shim, "--mode"));
    assert_eq!(opt(&shim, "--bitrate"), "1024");
    // the native options are the plain ones, the data rate is the shim's
    assert_eq!(opt(&shim, "-e"), "ddp:drc_profile=none:lfe_on=0");
    assert!(!has(&shim, "--data-rate") && !has(&shim, "-c"));
    assert!(opt(&shim, "-i").ends_with("five.5ch.wav"));
    assert!(env.last_run("dee_ddp_encoder").is_some());
    // DD as well, at another bitrate
    ok(&env.deew(&["-i", "five.wav", "-f", "dd", "-b", "448"]));
    let shim = env.dshim_run().unwrap();
    assert_eq!(opt(&shim, "--mode"), "dd");
    assert_eq!(opt(&shim, "--layout"), "5.0");
    assert_eq!(opt(&shim, "--bitrate"), "448");
    assert_eq!(opt(&shim, "-e"), "dd:drc_profile=none:lfe_on=0");
    // the pipe: raw PCM as DEE takes it (6 channels, silent LFE), no --unpadded-wav
    env.set_pipe(true);
    ok(&env.deew(&["-i", "five.wav"]));
    let shim = env.dshim_run().unwrap();
    assert_eq!(opt(&shim, "--layout"), "5.0");
    assert!(!has(&shim, "--unpadded-wav"));
    assert!(
        opt(&shim, "--input-format").contains("channels=6"),
        "{shim:?}"
    );
    assert_eq!(opt(&shim, "-e"), "ddp:drc_profile=none:lfe_on=0");
    env.set_pipe(false);
    // a stereo downmix has no LFE to remove: DEE alone
    fs::remove_file(env.path("bin").join("dee_shim.args")).ok();
    ok(&env.deew(&["-i", "five.wav", "-dm", "2"]));
    let args = env.run("dee_ddp_encoder");
    assert_eq!(opt(&args, "-e"), "ddp:drc_profile=none");
    assert_eq!(opt(&args, "-c"), "stereo");
    assert!(env.dshim_run().is_none());
    // 5.1 with -dl is not a 5-channel source: plain DEE
    env.media("fivone.wav", "5.1", 48000, "pcm_s24le");
    ok(&env.deew(&["-i", "fivone.wav", "-dl"]));
    assert!(env.dshim_run().is_none());
    assert_eq!(
        opt(&env.run("dee_ddp_encoder"), "-e"),
        "ddp:drc_profile=none:lfe_on=0"
    );

    // 7.0 (FL FR FC BL BR SL SR): side pair to Ls/Rs, back pair to Lrs/Rrs
    env.media("seven.wav", "7.0", 48000, "pcm_s24le");
    let out = env.deew(&["-i", "seven.wav", "-k", "-la"]);
    ok(&out);
    let text = stdout(&out);
    assert!(
        text.contains("pan=7.0|c0=c0|c1=c1|c2=c2|c3=c5|c4=c6|c5=c3|c6=c4"),
        "{text}"
    );
    // no LFE slot: the wav has the 7 channels of the layout, read with --unpadded-wav
    let wav = env.temp().join("seven.7ch.wav");
    assert_eq!(ffprobe_field(&wav, "channels"), "7");
    assert_eq!(last_summary_row(&text, "Channels"), "7.0");
    assert_eq!(summary_row(&text, "Profile"), "Standard (dee_shim)");
    let shim = env.dshim_run().unwrap();
    assert_eq!(opt(&shim, "--layout"), "7.0/eac3");
    assert!(has(&shim, "--unpadded-wav"));
    assert!(env.out().join("seven.ec3").exists());
    // --ddp-mode bluray: the Blu-ray 7.0 layout of dee_shim (AC-3 core)
    fs::remove_file(env.path("bin").join("dee_shim.args")).ok();
    let out = env.deew(&["-i", "seven.wav", "--ddp-mode", "bluray", "-b", "1100"]);
    ok(&out);
    assert_eq!(summary_row(&stdout(&out), "Profile"), "Blu-ray");
    let shim = env.dshim_run().unwrap();
    assert_eq!(opt(&shim, "--layout"), "7.0");
    assert_eq!(opt(&shim, "--bitrate"), "1024");
    assert!(env.out().join("seven.eb3").exists());
    // 7.1 with -dl is the same layout, the LFE slot is left out of the wav too
    env.media("eight.wav", "7.1", 48000, "pcm_s24le");
    let out = env.deew(&["-i", "eight.wav", "-dl", "-k"]);
    ok(&out);
    let text = stdout(&out);
    assert!(
        text.contains("pan=7.0|c0=c0|c1=c1|c2=c2|c3=c6|c4=c7|c5=c4|c6=c5"),
        "{text}"
    );
    assert_eq!(
        ffprobe_field(&env.temp().join("eight.7ch.wav"), "channels"),
        "7"
    );
    assert_eq!(opt(&env.dshim_run().unwrap(), "--layout"), "7.0/eac3");
    // the pipe: DEE takes no raw PCM of 7 channels, the LFE slot stays (silent)
    env.set_pipe(true);
    ok(&env.deew(&["-i", "seven.wav"]));
    let shim = env.dshim_run().unwrap();
    assert!(
        opt(&shim, "--input-format").contains("channels=8"),
        "{shim:?}"
    );
    assert!(!has(&shim, "--unpadded-wav"));
    env.set_pipe(false);
    // downmix from 7.0 (not a 5-channel source: DEE alone)
    ok(&env.deew(&["-i", "seven.wav", "-dm", "6"]));
    let args = env.run("dee_ddp_encoder");
    assert_eq!(opt(&args, "-c"), "5.1");
    assert_eq!(opt(&args, "-e"), "ddp:drc_profile=none:lfe_on=0");
    ok(&env.deew(&["-i", "seven.wav", "-f", "dd"]));
    assert_eq!(
        opt(&env.run("dee_ddp_encoder"), "-e"),
        "dd:drc_profile=none:lfe_on=0"
    );
    // TrueHD can't leave the LFE out
    let out = env.deew(&["-i", "seven.wav", "-f", "thd", "-r", "speech"]);
    ok(&out);
    let text = stdout(&out);
    assert!(
        text.contains("TrueHD can't leave out the LFE channel, it is kept silent."),
        "{text}"
    );
    assert_eq!(last_summary_row(&text, "Channels"), "7.1 (silent LFE)");
    assert_eq!(
        opt(&env.run("dee_dthd_encoder"), "--presentation"),
        "8ch:drc_profile=speech"
    );
}

#[test]
fn five_zero_dd_and_ddp_need_dee_shim() {
    let env = env_or_skip!();
    env.media("five.wav", "5.0", 48000, "pcm_s24le");
    fs::remove_file(env.path("bin").join("dee_shim")).unwrap();
    let msg = "dee_shim is required for 5.0 DD and DDP, put dee_shim next to deew or in your PATH, or get it with -gt (https://github.com/truehdd/dee-shim)";
    assert_error(&env.deew(&["-i", "five.wav"]), msg);
    assert_error(&env.deew(&["-i", "five.wav", "-f", "dd"]), msg);
    assert!(env.last_run("dee_ddp_encoder").is_none());
    // the other jobs of a 5.0 file don't
    ok(&env.deew(&["-i", "five.wav", "-dm", "2"]));
    ok(&env.deew(&["-i", "five.wav", "-f", "thd", "-r", "speech"]));
    ok(&env.deew(&["-i", "five.wav", "-mo"]));
}

#[test]
fn five_and_seven_channel_inputs_need_their_layout() {
    let env = env_or_skip!();
    for (name, layout) in [("a.wav", "4.1"), ("b.wav", "7.0(front)"), ("c.wav", "6.1")] {
        env.media(name, layout, 48000, "pcm_s24le");
        let out = env.deew(&["-i", name]);
        assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
        let text = stdout(&out);
        assert!(
            text.contains(&format!(
                "the channel layout {layout} ({name}) is not supported"
            )),
            "{text}"
        );
        assert!(
            text.contains("5 channels have to be 5.0 and 7 channels 7.0"),
            "{text}"
        );
    }
    env.media("d.wav", "5.0(side)", 48000, "pcm_s24le");
    ok(&env.deew(&["-i", "d.wav"]));
}

#[test]
fn disable_lfe_per_format() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    // DDP 5.1: lfe_on=0, DEE drops the slot, no pan needed
    let out = env.deew(&["-i", "in.wav", "-dl", "-la"]);
    ok(&out);
    let text = stdout(&out);
    assert!(!text.contains("pan="), "{text}");
    assert_eq!(last_summary_row(&text, "Channels"), "5.0");
    assert_eq!(
        opt(&env.run("dee_ddp_encoder"), "-e"),
        "ddp:drc_profile=none:lfe_on=0"
    );
    // without -dl the LFE is kept
    let text = stdout(&env.deew(&["-i", "in.wav"]));
    assert_eq!(last_summary_row(&text, "Channels"), "5.1");
    assert_eq!(
        opt(&env.run("dee_ddp_encoder"), "-e"),
        "ddp:drc_profile=none"
    );
    // TrueHD: the LFE is emptied in the pan step
    let out = env.deew(&[
        "-i",
        "in.wav",
        "--disable-lfe",
        "-f",
        "thd",
        "-r",
        "speech",
        "-la",
    ]);
    ok(&out);
    let text = stdout(&out);
    assert!(
        text.contains("pan=5.1|c0=c0|c1=c1|c2=c2|c3=0*c0|c4=c4|c5=c5"),
        "{text}"
    );
    assert!(text.contains("TrueHD can't leave out the LFE channel, it is kept silent."));
    assert_eq!(last_summary_row(&text, "Channels"), "5.1 (silent LFE)");
    // a stereo downmix: nothing to remove
    let out = env.deew(&["-i", "in.wav", "-dl", "-dm", "2"]);
    ok(&out);
    assert!(stdout(&out).contains("-dl is ignored, the output has no LFE channel."));
    assert!(
        !env.run("dee_ddp_encoder")
            .iter()
            .any(|a| a.contains("lfe_on"))
    );

    env.media("sur.wav", "7.1", 48000, "pcm_s24le");
    // DDP 7.1: the 7.0 layouts of dee_shim
    ok(&env.deew(&["-i", "sur.wav", "-dl"]));
    assert_eq!(opt(&env.dshim_run().unwrap(), "--layout"), "7.0/eac3");
    ok(&env.deew(&["-i", "sur.wav", "-dl", "--ddp-mode", "bluray"]));
    assert_eq!(opt(&env.dshim_run().unwrap(), "--layout"), "7.0");
    // 7.1 -dl -dm 6: a real 5.0
    ok(&env.deew(&["-i", "sur.wav", "-dl", "-dm", "6"]));
    let args = env.run("dee_ddp_encoder");
    assert_eq!(opt(&args, "-c"), "5.1");
    assert_eq!(opt(&args, "-e"), "ddp:drc_profile=none:lfe_on=0");
    // AC4 (7.1 becomes 5.1) keeps a silent LFE
    let out = env.deew(&["-i", "sur.wav", "-dl", "-f", "ac4", "-la"]);
    ok(&out);
    let text = stdout(&out);
    assert!(
        text.contains("AC4 can't leave out the LFE channel, it is kept silent."),
        "{text}"
    );
    assert!(text.contains("c3=0*c0"), "{text}");
    // AAC is always 2.0
    let out = env.deew(&["-i", "sur.wav", "-dl", "-f", "aac"]);
    ok(&out);
    assert!(stdout(&out).contains("-dl is ignored, the output has no LFE channel."));

    // inputs without an LFE channel
    env.media("st.wav", "stereo", 48000, "pcm_s24le");
    let out = env.deew(&["-i", "st.wav", "-dl"]);
    ok(&out);
    assert!(stdout(&out).contains("-dl only applies to 5.1 and 7.1 inputs, it is ignored."));
    env.media("five.wav", "5.0", 48000, "pcm_s24le");
    let out = env.deew(&["-i", "five.wav", "-dl"]);
    ok(&out);
    assert!(stdout(&out).contains("-dl is not needed, a 5.0 or 7.0 input has no LFE channel."));
}

#[test]
fn disable_lfe_is_ignored_for_atmos() {
    let env = env_or_skip!();
    env.truehd("in.thd");
    let out = env.deew(&["-i", "in.thd", "-f", "ac4-atmos", "-dl"]);
    ok(&out);
    assert!(
        stdout(&out).contains("-dl is ignored for the Atmos formats."),
        "{}",
        stdout(&out)
    );
}

/// Start deew like [`Env::deew_env`] without waiting for it.
fn spawn_deew(env: &Env, args: &[&str]) -> std::process::Child {
    let out = env.out();
    spawn_retry(
        command(env.bin())
            .args(["-np", "-o", out.to_str().unwrap()])
            .args(args)
            .current_dir(env.work())
            .env("XDG_CONFIG_HOME", env.path("xdg"))
            .env("FAKE_SLOW", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()),
    )
}

fn wait_for(path: &Path) {
    for _ in 0..400 {
        if path.exists() {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    panic!("{} did not appear", path.display());
}

fn interrupt(child: &std::process::Child, signal: i32) {
    // SAFETY: kill(2) on the PID of our own child process.
    assert_eq!(unsafe { libc::kill(child.id() as i32, signal) }, 0);
}

fn temp_files(env: &Env) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(env.temp())
        .map(|d| {
            d.map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

#[test]
fn interrupt_removes_partial_output_and_temp_files() {
    let env = env_or_skip!();
    for name in ["a_slow.wav", "b.wav"] {
        let wav = env.media(name, "5.1", 48000, "pcm_s24le");
        assert!(wav.exists());
    }
    let mut child = spawn_deew(&env, &["-i", "a_slow.wav", "b.wav", "-in", "1", "-kl"]);
    wait_for(&env.out().join("a_slow.ec3"));
    interrupt(&child, libc::SIGINT);
    let out = {
        let status = child.wait().unwrap();
        let mut text = String::new();
        std::io::Read::read_to_string(child.stdout.as_mut().unwrap(), &mut text).unwrap();
        (status, text)
    };
    assert_eq!(out.0.code(), Some(130), "{}", out.1);
    assert!(out.1.contains("interrupted, stopping"), "{}", out.1);
    assert!(
        out.1
            .contains("interrupted: 0 jobs finished, 1 stopped, 1 not started"),
        "{}",
        out.1
    );
    // nothing of the running job is left, the other one never started
    assert!(!env.out().join("a_slow.ec3").exists());
    assert!(!env.out().join("b.ec3").exists());
    assert!(temp_files(&env).is_empty(), "{:?}", temp_files(&env));
    // the report of the stopped job says so, the other job has none
    let log = fs::read_to_string(env.out().join("a_slow.deew-log.html")).unwrap();
    assert!(log.contains("interrupted"));
    assert!(!env.out().join("b.deew-log.html").exists());
}

#[test]
fn interrupt_keeps_finished_jobs_and_their_temp_files_with_k() {
    let env = env_or_skip!();
    for name in ["a.wav", "b_slow.wav", "c.wav"] {
        env.media(name, "5.1", 48000, "pcm_s24le");
    }
    let mut child = spawn_deew(
        &env,
        &["-i", "a.wav", "b_slow.wav", "c.wav", "-in", "1", "-k"],
    );
    wait_for(&env.out().join("b_slow.ec3"));
    // SIGTERM stops deew the same way
    interrupt(&child, libc::SIGTERM);
    let status = child.wait().unwrap();
    let mut text = String::new();
    std::io::Read::read_to_string(child.stdout.as_mut().unwrap(), &mut text).unwrap();
    assert_eq!(status.code(), Some(130), "{text}");
    assert!(
        text.contains("interrupted: 1 job finished, 1 stopped, 1 not started"),
        "{text}"
    );
    assert!(env.out().join("a.ec3").exists());
    assert!(!env.out().join("b_slow.ec3").exists());
    assert!(!env.out().join("c.ec3").exists());
    // -k keeps the finished job's wav, the stopped job's partial one goes
    assert_eq!(temp_files(&env), ["a.wav"]);
}

#[test]
fn second_signal_exits_at_once() {
    let env = env_or_skip!();
    env.media("a_slow.wav", "5.1", 48000, "pcm_s24le");
    let mut child = spawn_deew(&env, &["-i", "a_slow.wav"]);
    wait_for(&env.out().join("a_slow.ec3"));
    interrupt(&child, libc::SIGINT);
    std::thread::sleep(std::time::Duration::from_millis(300));
    interrupt(&child, libc::SIGINT);
    assert_eq!(child.wait().unwrap().code(), Some(130));
}

// ---------------------------------------------------------------------------
// ffmpeg piped straight into DEE (`pipe = true`)

/// What a fake tool read from its stdin, in bytes.
fn piped_bytes(env: &Env, tool: &str) -> u64 {
    fs::read_to_string(env.dee().join(format!("{tool}.stdin")))
        .unwrap_or_else(|e| panic!("{tool} read no pipe: {e}"))
        .trim()
        .parse()
        .unwrap()
}

#[test]
fn piped_ddp_has_no_wav() {
    let env = env_or_skip!();
    env.set_pipe(true);
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    let out = env.deew(&["-i", "in.wav", "-k"]);
    ok(&out);
    let text = stdout(&out);
    // one command: ffmpeg into the encoder
    assert!(
        text.contains("ffmpeg -y -drc_scale 0 -i [input] -map 0:a:0 -c pcm_s24le -f s24le - | dee_ddp_encoder --progress 1 --overwrite 1 --input-format pcm_s24le:sample_rate=48000:channels=6:streaming=1:sample_count=144000 -i /dev/stdin -o [output]"),
        "{text}"
    );
    let args = env.run("dee_ddp_encoder");
    assert_eq!(
        opt(&args, "--input-format"),
        "pcm_s24le:sample_rate=48000:channels=6:streaming=1:sample_count=144000"
    );
    assert_eq!(opt(&args, "-i"), "/dev/stdin");
    // 2 s of 24 bit 5.1 came through the pipe
    assert_eq!(piped_bytes(&env, "dee_ddp_encoder"), 2 * 48000 * 6 * 3);
    assert_eq!(
        fs::read(env.out().join("in.ec3")).unwrap(),
        b"fake dee_ddp_encoder"
    );
    // not even -k leaves anything: no wav was written
    assert!(temp_is_empty(&env), "{:?}", temp_files(&env));
}

#[test]
fn piping_option_overrides_the_config() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    // the config says never, -p always pipes
    let out = env.deew(&["-i", "in.wav", "-p", "always"]);
    ok(&out);
    assert_eq!(piped_bytes(&env, "dee_ddp_encoder"), 2 * 48000 * 6 * 3);
    assert!(stdout(&out).contains("on (always)"), "{}", stdout(&out));
    // the config says auto, -p never uses the temp wav
    env.set_pipe(true);
    let out = env.deew(&["-i", "in.wav", "-p", "never", "-k"]);
    ok(&out);
    let text = stdout(&out);
    assert!(text.contains("off"), "{text}");
    assert!(!text.contains("/dev/stdin"), "{text}");
    assert!(opt(&env.run("dee_ddp_encoder"), "-i").ends_with(".wav"));
}

#[test]
fn piped_jobs_have_no_ffmpeg_stage() {
    let env = env_or_skip!();
    env.set_pipe(true);
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    let file = env.path("events.jsonl");
    let out = env.deew_env(
        &["-i", "in.wav"],
        &[("DEEW_EVENTS_FILE", file.to_str().unwrap())],
    );
    ok(&out);
    let all = events(&file);
    let stages: Vec<&str> = all
        .iter()
        .filter(|e| e["type"] == "progress")
        .filter_map(|e| e["stage"].as_str())
        .collect();
    assert!(
        stages.contains(&"measure") && stages.contains(&"encode"),
        "{stages:?}"
    );
    assert!(!stages.contains(&"ffmpeg"), "{stages:?}");
    assert!(
        all.iter()
            .any(|e| e["type"] == "progress" && e["stage"] == "encode" && e["percent"] == 100.0)
    );
}

#[test]
fn piped_long_print_and_report() {
    let env = env_or_skip!();
    env.set_pipe(true);
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    let out = env.deew(&["-i", "in.wav", "-la", "-kl"]);
    ok(&out);
    let text = stdout(&out);
    assert!(text.contains(" -f s24le - | dee_ddp_encoder"), "{text}");
    assert!(text.contains("-i /dev/stdin -o "), "{text}");
    assert!(!text.contains("-rf64"), "{text}");
    let log = fs::read_to_string(env.out().join("in.deew-log.html")).unwrap();
    assert!(log.contains("-f s24le -"), "{log}");
    assert!(log.contains("/dev/stdin"), "{log}");
    assert!(!log.contains("-rf64"));
    assert!(temp_is_empty(&env));
}

#[test]
fn piped_resampling_counts_samples_at_the_output_rate() {
    let env = env_or_skip!();
    env.set_pipe(true);
    env.media("in.wav", "stereo", 44100, "pcm_s24le");
    ok(&env.deew(&["-i", "in.wav"]));
    let args = env.run("dee_ddp_encoder");
    assert_eq!(
        opt(&args, "--input-format"),
        "pcm_f32le:sample_rate=48000:channels=2:streaming=1:sample_count=144000"
    );
    // 2 s resampled to 48 kHz, 32 bit float stereo
    assert_eq!(piped_bytes(&env, "dee_ddp_encoder"), 2 * 48000 * 2 * 4);
}

#[test]
fn piped_encoders_and_measuring() {
    let env = env_or_skip!();
    env.set_pipe(true);
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    for (format, tool) in [
        ("dd", "dee_ddp_encoder"),
        ("ac4", "dee_ac4_encoder"),
        ("aac", "dee_aac_encoder"),
    ] {
        let out = env.deew(&["-i", "in.wav", "-f", format]);
        ok(&out);
        let args = env.run(tool);
        assert_eq!(opt(&args, "-i"), "/dev/stdin", "{format}");
        assert!(
            opt(&args, "--input-format").starts_with("pcm_s24le:"),
            "{format}"
        );
        assert!(piped_bytes(&env, tool) > 0, "{format}");
        assert!(temp_is_empty(&env), "{format}: {:?}", temp_files(&env));
    }
    // -mo: dee_measure_loudness reads the pipe too
    let out = env.deew(&["-i", "in.wav", "-mo"]);
    ok(&out);
    let args = env.run("dee_measure_loudness");
    assert_eq!(opt(&args, "-i"), "/dev/stdin");
    assert_eq!(piped_bytes(&env, "dee_measure_loudness"), 2 * 48000 * 6 * 3);
    assert!(temp_is_empty(&env), "{:?}", temp_files(&env));
    // 7.1 is measured by dee_shim measure_loudness
    env.media("in8.wav", "7.1", 48000, "pcm_s24le");
    let out = env.deew(&["-i", "in8.wav", "-mo"]);
    ok(&out);
    assert!(stdout(&out).contains("-23"), "{}", stdout(&out));
    let args = env.run("dee_measure_loudness");
    assert_eq!(opt(&args, "-i"), "/dev/stdin");
    assert!(opt(&args, "--input-format").contains("channels=8"));
    assert_eq!(opt(&env.dshim_run().unwrap(), "--layout"), "7.1");
    assert_eq!(piped_bytes(&env, "dee_measure_loudness"), 2 * 48000 * 8 * 3);
    assert!(temp_is_empty(&env), "{:?}", temp_files(&env));
}

#[test]
fn truehd_and_atmos_keep_the_wav_and_ac4_dialnorm_pipes() {
    let env = env_or_skip!();
    env.set_pipe(true);
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    // TrueHD reads wav only
    ok(&env.deew(&["-i", "in.wav", "-f", "thd", "-k"]));
    let args = env.run("dee_dthd_encoder");
    assert_eq!(opt(&args, "--input-format"), "wav");
    assert!(env.temp().join("in.wav").exists());
    fs::remove_file(env.temp().join("in.wav")).unwrap();
    // AC4 with a custom dialnorm: ffmpeg feeds the measurement and the
    // encode, no wav
    ok(&env.deew(&["-i", "in.wav", "-f", "ac4", "-dn", "-20", "-k"]));
    for tool in ["dee_measure_loudness", "dee_ac4_encoder"] {
        let args = env.run(tool);
        assert!(opt(&args, "--input-format").starts_with("pcm_"), "{tool}");
        assert_eq!(opt(&args, "-i"), "/dev/stdin", "{tool}");
    }
    assert!(!env.temp().join("in.wav").exists());
    // pipe = false: both read the wav
    let config = fs::read_to_string(env.config()).unwrap();
    fs::write(env.config(), config.replace("pipe = true", "pipe = false")).unwrap();
    ok(&env.deew(&["-i", "in.wav", "-f", "ac4", "-dn", "-20", "-k"]));
    for tool in ["dee_measure_loudness", "dee_ac4_encoder"] {
        let args = env.run(tool);
        assert_eq!(opt(&args, "--input-format"), "wav", "{tool}");
        assert_eq!(
            opt(&args, "-i"),
            env.temp().join("in.wav").to_str().unwrap(),
            "{tool}"
        );
    }
    assert!(env.temp().join("in.wav").exists());
    fs::write(env.config(), config).unwrap();
    // the Atmos formats are unchanged
    env.truehd("atmos.thd");
    ok(&env.deew(&["-i", "atmos.thd", "-f", "ac4-atmos"]));
    assert!(env.truehdd_run().is_some());
    let args = env.run("dee_ac4ajoc_encoder");
    assert_ne!(opt(&args, "-i"), "/dev/stdin");
}

/// A stand-in for ffmpeg that fails on demand and else runs the real one.
fn fake_ffmpeg(env: &Env) {
    write_script(
        &env.path("bin/ffmpeg"),
        r#"#!/bin/sh
case "$FAKE_FFMPEG" in
  fail)
    echo "[aist#0:0/pcm_s24le @ 0x55d5c1a2b340] Error: Invalid data found when processing input" >&2
    exit 1 ;;
  partial)
    head -c 3000 /dev/zero
    echo "Error while decoding stream #0:0: Invalid data found" >&2
    exit 1 ;;
esac
exec ffmpeg "$@"
"#,
    );
}

#[test]
fn a_failing_ffmpeg_is_reported_as_ffmpeg() {
    let env = env_or_skip!();
    env.set_pipe(true);
    fake_ffmpeg(&env);
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    // no audio at all: DEE only sees a short input, but ffmpeg is the reason
    let out = env.deew_env(&["-i", "in.wav"], &[("FAKE_FFMPEG", "fail")]);
    assert_error(
        &out,
        "ffmpeg failed (exit code 1): Error: Invalid data found when processing input",
    );
    assert!(!stdout(&out).contains("too short"), "{}", stdout(&out));
    assert!(!env.out().join("in.ec3").exists());
    assert!(temp_is_empty(&env));
    // some audio, then the failure: DEE ends fine, the job still fails and
    // leaves no output
    let out = env.deew_env(&["-i", "in.wav"], &[("FAKE_FFMPEG", "partial")]);
    assert_error(
        &out,
        "ffmpeg failed (exit code 1): Error while decoding stream #0:0: Invalid data found",
    );
    assert!(!env.out().join("in.ec3").exists());
    assert!(temp_is_empty(&env));
    // the real ffmpeg works as before
    ok(&env.deew(&["-i", "in.wav"]));
}

#[test]
fn a_failing_dee_stops_ffmpeg_and_is_reported() {
    let env = env_or_skip!();
    env.set_pipe(true);
    fake_ffmpeg(&env);
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    // DEE quits without reading: ffmpeg finds the pipe closed ("Broken pipe"),
    // which is no error of its own
    let out = env.deew_env(
        &["-i", "in.wav", "-kl"],
        &[
            ("FAKE_FAIL_TOOL", "dee_ddp_encoder"),
            ("FAKE_ERROR", "[boom] failed"),
        ],
    );
    assert_error(&out, "dee_ddp_encoder failed (exit code 1): [boom] failed");
    let text = stdout(&out);
    assert_eq!(text.matches("[boom] failed").count(), 1, "{text}");
    assert!(!text.contains("ffmpeg failed"), "{text}");
    assert!(!env.out().join("in.ec3").exists());
    assert!(temp_is_empty(&env));
    let log = fs::read_to_string(env.out().join("in.deew-log.html")).unwrap();
    assert!(log.contains("ffmpeg stderr"), "{log}");
}

fn alive(pattern: &str) -> bool {
    command("pgrep")
        .args(["-f", pattern])
        .stdout(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[test]
fn interrupt_stops_ffmpeg_and_dee_of_a_piped_job() {
    let env = env_or_skip!();
    env.set_pipe(true);
    // a name no other test's ffmpeg has, for pgrep
    let unique = env
        .dir
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let name = format!("a_slow{unique}.wav");
    env.media(&name, "5.1", 48000, "pcm_s24le");
    // DEE does not read the pipe, ffmpeg hangs on a full pipe
    let mut child = {
        let out = env.out();
        spawn_retry(
            command(env.bin())
                .args(["-np", "-o", out.to_str().unwrap(), "-i", &name])
                .current_dir(env.work())
                .env("XDG_CONFIG_HOME", env.path("xdg"))
                .env("FAKE_SLOW", "1")
                .env("FAKE_NO_READ", "1")
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped()),
        )
    };
    let output = env.out().join(name.replace(".wav", ".ec3"));
    wait_for(&output);
    assert!(alive(&name), "ffmpeg should be running");
    interrupt(&child, libc::SIGINT);
    let status = child.wait().unwrap();
    let mut text = String::new();
    std::io::Read::read_to_string(child.stdout.as_mut().unwrap(), &mut text).unwrap();
    assert_eq!(status.code(), Some(130), "{text}");
    assert!(
        text.contains("interrupted: 0 jobs finished, 1 stopped, 0 not started"),
        "{text}"
    );
    assert!(!output.exists());
    assert!(temp_files(&env).is_empty(), "{:?}", temp_files(&env));
    assert!(!alive(&name), "ffmpeg is still running");
    assert!(!alive(env.dee().to_str().unwrap()), "DEE is still running");
}

// ---------------------------------------------------------------------------
// AC4 IMS and Atmos masters (ADM BWF, DAMF)

impl Env {
    /// A minimal ADM BWF: the headers are all deew looks at. `axml` comes
    /// after the data like in real files.
    fn adm(&self, name: &str) -> PathBuf {
        fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
            let mut v = id.to_vec();
            v.extend((body.len() as u32).to_le_bytes());
            v.extend(body);
            if body.len() % 2 == 1 {
                v.push(0);
            }
            v
        }
        let mut fmt = Vec::new();
        fmt.extend(1u16.to_le_bytes());
        fmt.extend(2u16.to_le_bytes());
        fmt.extend(48000u32.to_le_bytes());
        fmt.extend((48000u32 * 6).to_le_bytes());
        fmt.extend(6u16.to_le_bytes());
        fmt.extend(24u16.to_le_bytes());
        let mut body = b"WAVE".to_vec();
        for c in [
            chunk(b"fmt ", &fmt),
            chunk(b"chna", &[0; 4]),
            chunk(b"data", &[0; 6000]),
            chunk(b"axml", b"<ebuCoreMain/>"),
        ] {
            body.extend(c);
        }
        let mut file = b"RIFF".to_vec();
        file.extend((body.len() as u32).to_le_bytes());
        file.extend(body);
        let path = self.work().join(name);
        fs::write(&path, file).unwrap();
        path
    }

    /// A DAMF set (`.atmos`, `.atmos.metadata`, `.atmos.audio`) in `dir`.
    fn damf(&self, dir: &Path, name: &str, parts: bool) -> PathBuf {
        fs::create_dir_all(dir).unwrap();
        let path = dir.join(format!("{name}.atmos"));
        fs::write(
            &path,
            format!(
                "version: 0.5.1\npresentations:\n  - type: home\n    metadata: {name}.atmos.metadata\n    audio: {name}.atmos.audio\n"
            ),
        )
        .unwrap();
        if parts {
            fs::write(dir.join(format!("{name}.atmos.metadata")), "m").unwrap();
            fs::write(dir.join(format!("{name}.atmos.audio")), "a").unwrap();
        }
        path
    }
}

#[test]
fn ac4_ims_from_51() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    let out = env.deew(&["-i", "in.wav", "-f", "ac4-ims", "-d", "+100ms"]);
    ok(&out);
    let text = stdout(&out);
    assert_eq!(summary_row(&text, "Format"), "AC4 IMS");
    assert_eq!(last_summary_row(&text, "Channels"), "immersive stereo");
    assert_eq!(last_summary_row(&text, "Bitrate"), "256 kbps");
    assert!(!text.contains("│ Mode"), "{text}");
    let args = env.run("dee_ac4ims_encoder");
    assert_eq!(opt(&args, "--input-format"), "wav");
    assert_eq!(
        opt(&args, "-i"),
        env.temp().join("in.wav").to_str().unwrap()
    );
    assert_eq!(opt(&args, "--data-rate"), "256");
    assert_eq!(opt(&args, "-e"), "drc_profile=none");
    assert_eq!(opt(&args, "--loudness-management"), "measure_only");
    assert_eq!(opt(&args, "--add-silence"), "0.057333:0");
    for unknown in ["--temp-dir", "--output-manifest", "-c"] {
        assert!(!has(&args, unknown), "{unknown}");
    }
    assert!(env.out().join("in.ac4").exists());
    assert!(temp_is_empty(&env));
    // no dee_shim, no truehdd
    assert!(env.dshim_run().is_none() && env.truehdd_run().is_none());
    // bitrate and DRC, the underscore spelling and the case
    let out = env.deew(&["-i", "in.wav", "-f", "AC4_IMS", "-b", "96", "-r", "speech"]);
    ok(&out);
    let args = env.run("dee_ac4ims_encoder");
    assert_eq!(opt(&args, "--data-rate"), "96");
    assert_eq!(opt(&args, "-e"), "drc_profile=speech");
}

#[test]
fn ac4_ims_other_channel_counts_and_rates() {
    let env = env_or_skip!();
    // 7.1 is downmixed to the 5.1 DEE takes, 5.0 gets a silent LFE
    for (layout, rate) in [("7.1", 48000), ("5.0", 48000), ("5.1", 44100)] {
        let name = format!("c{layout}_{rate}.wav");
        env.media(&name, layout, rate, "pcm_s24le");
        let out = env.deew(&["-i", &name, "-f", "ac4-ims", "-la"]);
        ok(&out);
        let text = stdout(&out);
        assert!(text.contains("ffmpeg"), "{layout}: {text}");
        if layout == "7.1" {
            assert!(text.contains("pan=5.1"), "{text}");
        }
        let args = env.run("dee_ac4ims_encoder");
        assert_eq!(opt(&args, "--input-format"), "wav", "{layout}");
        assert!(!has(&args, "-c"), "{layout}");
        assert!(temp_is_empty(&env), "{layout}");
    }
    // the file DEE got is 5.1 at 48 kHz: look at what ffmpeg made, with -k
    env.media("seven.wav", "7.1", 44100, "pcm_s24le");
    ok(&env.deew(&["-i", "seven.wav", "-f", "ac4-ims", "-k"]));
    let kept = fs::read_dir(env.temp())
        .unwrap()
        .flatten()
        .map(|e| e.path())
        .find(|p| p.extension().is_some_and(|e| e == "wav"))
        .expect("kept wav");
    assert_eq!(ffprobe_field(&kept, "channels"), "6");
    assert_eq!(ffprobe_field(&kept, "sample_rate"), "48000");
}

#[test]
fn ac4_ims_music_mode() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    for mode in ["music", "Music"] {
        let out = env.deew(&["-i", "in.wav", "-f", "ac4-ims", "--ac4-ims-mode", mode]);
        ok(&out);
        assert_eq!(summary_row(&stdout(&out), "Mode"), "music");
        let args = env.run("dee_ac4ims_encoder");
        assert_eq!(opt(&args, "-e"), "mode=music:drc_profile=music_light");
        assert_eq!(
            opt(&args, "--loudness-management"),
            "measure_only:dialogue_intelligence=0"
        );
    }
    // the same profile is no conflict
    ok(&env.deew(&[
        "-i",
        "in.wav",
        "-f",
        "ac4-ims",
        "--ac4-ims-mode",
        "music",
        "-r",
        "music_light",
    ]));
    assert_error(
        &env.deew(&[
            "-i",
            "in.wav",
            "-f",
            "ac4-ims",
            "--ac4-ims-mode",
            "music",
            "-r",
            "speech",
        ]),
        "--ac4-ims-mode music (or ac4_ims = 'music' in the config) needs the DRC profile music_light (DEE's music mode takes no other), leave out -r or use -r music_light.",
    );
    assert_error(
        &env.deew(&["-i", "in.wav", "-f", "ac4-ims", "--ac4-ims-mode", "x"]),
        "--ac4-ims-mode can only be general or music.",
    );
    // general is the default
    ok(&env.deew(&["-i", "in.wav", "-f", "ac4-ims", "--ac4-ims-mode", "GENERAL"]));
    assert_eq!(
        opt(&env.run("dee_ac4ims_encoder"), "-e"),
        "drc_profile=none"
    );
    // other formats ignore it with a note
    let out = env.deew(&["-i", "in.wav", "--ac4-ims-mode", "music"]);
    ok(&out);
    assert!(
        stdout(&out).contains("--ac4-ims-mode only applies to AC4 IMS output, it is ignored."),
        "{}",
        stdout(&out)
    );
}

#[test]
fn ac4_ims_input_and_option_errors() {
    let env = env_or_skip!();
    env.media("st.wav", "stereo", 48000, "pcm_s24le");
    env.media("mono.wav", "mono", 48000, "pcm_s24le");
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    for name in ["st.wav", "mono.wav"] {
        assert_error(
            &env.deew(&["-i", name, "-f", "ac4-ims"]),
            "ac4-ims needs a 5.1 (or 5.0, 7.0, 7.1) or an Atmos input, stereo and mono can't be encoded to immersive stereo.",
        );
    }
    assert_error(
        &env.deew(&["-i", "in.wav", "-f", "ac4-ims", "-dm", "2"]),
        "-dm/--downmix doesn't work for ac4-ims, it always encodes immersive stereo (a 7.1 input is downmixed to 5.1 first).",
    );
    assert_error(
        &env.deew(&["-i", "in.wav", "-f", "ac4-ims", "-dl"]),
        "-dl/--disable-lfe doesn't work for ac4-ims, it needs the LFE channel of a 5.1 source.",
    );
    assert_error(
        &env.deew(&["-i", "in.wav", "-f", "ac4-ims", "-dn", "-27"]),
        "custom dialnorm is only supported for DD, DDP, DDP Atmos, TrueHD, TrueHD Atmos and AC4 encoding.",
    );
    assert!(env.last_run("dee_ac4ims_encoder").is_none());
}

#[test]
fn ac4_ims_from_truehd_atmos() {
    let env = env_or_skip!();
    env.truehd("atmos.thd");
    let out = env.deew(&["-i", "atmos.thd", "-f", "ac4-ims", "-d", "+100ms"]);
    ok(&out);
    let text = stdout(&out);
    assert_eq!(last_summary_row(&text, "Channels"), "immersive stereo");
    let truehdd = env.truehdd_run().unwrap();
    assert_eq!(
        opt(&truehdd, "--output-path"),
        env.temp().join("atmos").to_str().unwrap()
    );
    let args = env.run("dee_ac4ims_encoder");
    assert_eq!(opt(&args, "--input-format"), "atmos_mezz");
    assert_eq!(
        opt(&args, "-i"),
        env.temp().join("atmos.atmos").to_str().unwrap()
    );
    assert_eq!(opt(&args, "--add-silence"), "0.057333:0");
    assert!(!has(&args, "--temp-dir"));
    assert!(env.out().join("atmos.ac4").exists());
    assert!(temp_is_empty(&env));
    assert!(env.dshim_run().is_none());
    // music mode and the long command
    let out = env.deew(&[
        "-i",
        "atmos.thd",
        "-f",
        "ac4-ims",
        "--ac4-ims-mode",
        "music",
        "-la",
    ]);
    ok(&out);
    assert_eq!(
        opt(&env.run("dee_ac4ims_encoder"), "-e"),
        "mode=music:drc_profile=music_light"
    );
    assert!(stdout(&out).contains("truehdd"), "{}", stdout(&out));
    // a TrueHD without Atmos is an ordinary 7.1 source
    env.truehd("plain.thd");
    let out = env.deew_env(
        &["-i", "plain.thd", "-f", "ac4-ims"],
        &[("FAKE_NO_PROFILE", "1")],
    );
    ok(&out);
    assert_eq!(opt(&env.run("dee_ac4ims_encoder"), "--input-format"), "wav");
    // channel-based and Atmos inputs don't mix
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    assert_error(
        &env.deew(&["-i", "atmos.thd", "in.wav", "-f", "ac4-ims"]),
        "ac4-ims can't encode Atmos inputs (TrueHD Atmos, DTS:X, Auro-3D, ADM BWF, DAMF) and channel-based inputs in the same run.",
    );
}

fn harletty_info_runs(env: &Env) -> usize {
    fs::read_to_string(env.path("bin/harletty.info.args"))
        .map(|text| text.matches("---").count())
        .unwrap_or(0)
}

#[test]
fn ac4_ims_from_dtsx_goes_through_harletty() {
    let env = env_or_skip!();
    env.dts("x.dts");
    let out = env.deew_env(
        &["-i", "x.dts", "-f", "ac4-ims", "-wm", "loro", "-bc", "off"],
        &[("FAKE_DTSX", "1")],
    );
    ok(&out);
    let text = stdout(&out);
    assert_eq!(last_summary_row(&text, "Channels"), "immersive stereo");
    assert_eq!(
        summary_row(&text, "harletty"),
        "warp mode loro, bed conform on"
    );
    // harletty looked at the track once, and decoded it
    assert_eq!(harletty_info_runs(&env), 1);
    let decode = env.harletty_run("decode").unwrap();
    assert!(has(&decode, "--bed-conform"));
    assert_eq!(opt(&decode, "--warp-mode"), "loro");
    assert!(env.truehdd_run().is_none());
    // from the .atmos on it is a TrueHD Atmos source of AC4 IMS
    let args = env.run("dee_ac4ims_encoder");
    assert_eq!(opt(&args, "--input-format"), "atmos_mezz");
    assert_eq!(
        opt(&args, "-i"),
        env.temp().join("x.wm-loro.bc.atmos").to_str().unwrap()
    );
    assert!(env.out().join("x.ac4").exists());
    assert!(temp_is_empty(&env));
    // the check of harletty alone finds it too, ffprobe says plain DTS
    let out = env.deew(&["-i", "x.dts", "-f", "ac4-ims"]);
    ok(&out);
    assert_eq!(
        opt(&env.run("dee_ac4ims_encoder"), "--input-format"),
        "atmos_mezz"
    );
}

#[test]
fn ac4_ims_from_plain_dts_is_channel_based() {
    let env = env_or_skip!();
    env.dts("x.dts");
    let out = env.deew_env(
        &["-i", "x.dts", "-f", "ac4-ims"],
        &[("FAKE_SPATIAL", "plain")],
    );
    ok(&out);
    assert!(env.harletty_run("decode").is_none());
    assert!(env.truehdd_run().is_none());
    assert_eq!(opt(&env.run("dee_ac4ims_encoder"), "--input-format"), "wav");
    // without harletty a plain DTS track needs none
    fs::remove_file(env.path("bin/harletty")).unwrap();
    ok(&env.deew(&["-i", "x.dts", "-f", "ac4-ims"]));
    assert_eq!(opt(&env.run("dee_ac4ims_encoder"), "--input-format"), "wav");
    // nor does a harletty that is too old
    write_script(&env.path("bin/harletty"), FAKE_HARLETTY);
    ok(&env.deew_env(
        &["-i", "x.dts", "-f", "ac4-ims"],
        &[("FAKE_HARLETTY_VERSION", "0.8.2")],
    ));
    assert_eq!(opt(&env.run("dee_ac4ims_encoder"), "--input-format"), "wav");
}

#[test]
fn ac4_ims_needs_harletty_for_a_dtsx_track() {
    let env = env_or_skip!();
    env.dts("x.dts");
    let out = env.deew_env(&["-i", "x.dts", "-f", "ac4-ims"], &[("FAKE_DTSX", "1")]);
    ok(&out);
    fs::remove_file(env.path("bin/harletty")).unwrap();
    let out = env.deew_env(&["-i", "x.dts", "-f", "ac4-ims"], &[("FAKE_DTSX", "1")]);
    assert_error(
        &out,
        "harletty is required for DTS:X and Auro-3D sources of the Atmos formats and AC-4 IMS (to read and decode them), put harletty next to deew or in your PATH, or get it with -gt (https://github.com/harletty/harletty-bridge)",
    );
    write_script(&env.path("bin/harletty"), FAKE_HARLETTY);
    let out = env.deew_env(
        &["-i", "x.dts", "-f", "ac4-ims"],
        &[("FAKE_DTSX", "1"), ("FAKE_HARLETTY_VERSION", "0.8.2")],
    );
    assert_error(
        &out,
        "harletty 0.8.2 is too old, deew needs 0.9.0 or newer: update it with -gt",
    );
}

#[test]
fn ac4_ims_does_not_mix_dtsx_and_channel_based_inputs() {
    let env = env_or_skip!();
    env.dts("x.dts");
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    assert_error(
        &env.deew(&["-i", "x.dts", "in.wav", "-f", "ac4-ims"]),
        "ac4-ims can't encode Atmos inputs (TrueHD Atmos, DTS:X, Auro-3D, ADM BWF, DAMF) and channel-based inputs in the same run.",
    );
}

#[test]
fn ac4_ims_bitrate_default_from_the_config() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    // an old config without the key: 256
    ok(&env.deew(&["-i", "in.wav", "-f", "ac4-ims"]));
    assert_eq!(opt(&env.run("dee_ac4ims_encoder"), "--data-rate"), "256");
    let text = fs::read_to_string(env.config()).unwrap();
    fs::write(
        env.config(),
        text.replace("ac4_atmos = 448\n", "ac4_atmos = 448\nac4_ims = 128\n"),
    )
    .unwrap();
    ok(&env.deew(&["-i", "in.wav", "-f", "ac4-ims"]));
    assert_eq!(opt(&env.run("dee_ac4ims_encoder"), "--data-rate"), "128");
}

#[test]
fn ac4_ims_mode_default_from_the_config() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    let text = fs::read_to_string(env.config()).unwrap();
    // no [default_modes] section or no key: general
    ok(&env.deew(&["-i", "in.wav", "-f", "ac4-ims"]));
    assert_eq!(
        opt(&env.run("dee_ac4ims_encoder"), "-e"),
        "drc_profile=none"
    );
    fs::write(
        env.config(),
        format!("{text}[default_modes]\nddp = 'bluray'\n"),
    )
    .unwrap();
    ok(&env.deew(&["-i", "in.wav", "-f", "ac4-ims"]));
    assert_eq!(
        opt(&env.run("dee_ac4ims_encoder"), "-e"),
        "drc_profile=none"
    );
    fs::write(
        env.config(),
        format!("{text}[default_modes]\nac4_ims = 'music'\n"),
    )
    .unwrap();
    let out = env.deew(&["-i", "in.wav", "-f", "ac4-ims"]);
    ok(&out);
    assert_eq!(summary_row(&stdout(&out), "Mode"), "music");
    assert_eq!(
        opt(&env.run("dee_ac4ims_encoder"), "-e"),
        "mode=music:drc_profile=music_light"
    );
    // the flag overrides the config
    ok(&env.deew(&["-i", "in.wav", "-f", "ac4-ims", "--ac4-ims-mode", "general"]));
    assert_eq!(
        opt(&env.run("dee_ac4ims_encoder"), "-e"),
        "drc_profile=none"
    );
    // the DRC check applies to the config's music too
    assert_error(
        &env.deew(&["-i", "in.wav", "-f", "ac4-ims", "-r", "speech"]),
        "--ac4-ims-mode music (or ac4_ims = 'music' in the config) needs the DRC profile music_light (DEE's music mode takes no other), leave out -r or use -r music_light.",
    );
}

#[test]
fn adm_bwf_master_goes_straight_to_dee() {
    let env = env_or_skip!();
    let adm = env.adm("film.wav");
    let adm = adm.to_str().unwrap();
    // ddp-atmos: the shim and encoder of the TrueHD Atmos path, no truehdd or ffmpeg
    let out = env.deew(&["-i", adm, "-f", "ddp-atmos", "-la"]);
    ok(&out);
    let text = stdout(&out);
    assert_eq!(summary_row(&text, "Source"), "ADM BWF Atmos master");
    assert!(!text.contains("│ Bit depth"), "{text}");
    assert!(
        !text.contains("ffmpeg -") && !text.contains("truehdd decode"),
        "{text}"
    );
    assert!(env.truehdd_run().is_none());
    let shim = env.dshim_run().unwrap();
    assert_eq!(opt(&shim, "--oamd-as-is"), "on");
    let rest: Vec<String> = shim[shim.iter().position(|a| a == "--").unwrap() + 1..].to_vec();
    assert_eq!(opt(&rest, "--input-format"), "atmos_mezz");
    assert_eq!(opt(&rest, "-i"), adm);
    assert!(env.out().join("film.ec3").exists());
    // the master is never removed
    assert!(Path::new(adm).exists());
    assert!(temp_is_empty(&env));
    // the others
    for (format, tool) in [
        ("thd-atmos", "dee_dthd_encoder"),
        ("ac4-atmos", "dee_ac4ajoc_encoder"),
        ("ac4-ims", "dee_ac4ims_encoder"),
    ] {
        ok(&env.deew(&["-i", adm, "-f", format, "-d", "+100ms"]));
        let args = env.run(tool);
        assert_eq!(opt(&args, "-i"), adm, "{format}");
        if format != "ac4-atmos" {
            assert_eq!(opt(&args, "--input-format"), "atmos_mezz", "{format}");
        }
        assert!(
            has(&args, "--start") || has(&args, "--add-silence"),
            "{format}: {args:?}"
        );
        assert!(Path::new(adm).exists() && temp_is_empty(&env), "{format}");
    }
    assert!(env.truehdd_run().is_none());
}

#[test]
fn damf_master_and_its_parts() {
    let env = env_or_skip!();
    let dir = env.work().join("set");
    let atmos = env.damf(&dir, "mix", true);
    let atmos = atmos.to_str().unwrap();
    let out = env.deew(&["-i", atmos, "-f", "ac4-ims", "-ti", "5"]);
    ok(&out);
    assert_eq!(summary_row(&stdout(&out), "Source"), "DAMF Atmos master");
    let args = env.run("dee_ac4ims_encoder");
    assert_eq!(opt(&args, "-i"), atmos);
    assert_eq!(opt(&args, "--input-format"), "atmos_mezz");
    assert!(env.out().join("mix.ac4").exists());
    for part in ["mix.atmos", "mix.atmos.metadata", "mix.atmos.audio"] {
        assert!(dir.join(part).exists(), "{part}");
    }
    // a folder takes the set once, the parts are no inputs
    let out = env.deew(&["-i", dir.to_str().unwrap(), "-f", "ac4-atmos"]);
    ok(&out);
    assert_eq!(summary_row(&stdout(&out), "Files"), "1");
    // the extension is case-insensitive
    let upper = dir.join("LOUD.ATMOS");
    fs::write(&upper, "version: 0.5.1\n").unwrap();
    fs::write(dir.join("LOUD.ATMOS.metadata"), "m").unwrap();
    fs::write(dir.join("LOUD.ATMOS.audio"), "a").unwrap();
    ok(&env.deew(&["-i", upper.to_str().unwrap(), "-f", "ac4-atmos"]));
    // a missing part is an error before anything runs
    let broken = env.damf(&env.work().join("broken"), "b", false);
    let out = env.deew(&["-i", broken.to_str().unwrap(), "-f", "ac4-atmos"]);
    assert_eq!(out.status.code(), Some(1), "{}", stdout(&out));
    let text = stdout(&out);
    assert!(text.contains("DAMF Atmos master is incomplete"), "{text}");
    assert!(
        text.contains("b.atmos.metadata") && text.contains("b.atmos.audio"),
        "{text}"
    );
}

#[test]
fn masters_measure_and_render_for_dd_and_ddp() {
    let env = env_or_skip!();
    let adm = env.adm("film.wav");
    let adm = adm.to_str().unwrap();
    // -mo: the measurement of the master itself
    let out = env.deew(&["-i", adm, "-mo"]);
    ok(&out);
    let args = env.run("dee_measure_loudness");
    assert_eq!(opt(&args, "--input-format"), "atmos_mezz");
    assert_eq!(opt(&args, "-i"), adm);
    assert!(
        stdout(&out).contains("finished: film.wav (-23 dB, dialogue-gated"),
        "{}",
        stdout(&out)
    );
    assert!(env.truehdd_run().is_none() && temp_is_empty(&env));
    // dd and ddp are rendered by DEE: 5.1, or `-dm`
    let out = env.deew(&["-i", adm, "-f", "ddp"]);
    ok(&out);
    assert_eq!(last_summary_row(&stdout(&out), "Channels"), "5.1");
    let args = env.run("dee_ddp_encoder");
    assert_eq!(opt(&args, "--input-format"), "atmos_mezz");
    assert_eq!(opt(&args, "-i"), adm);
    assert!(!has(&args, "-c"));
    ok(&env.deew(&["-i", adm, "-f", "dd", "-dm", "2"]));
    let args = env.run("dee_ddp_encoder");
    assert_eq!(opt(&args, "-c"), "stereo");
    assert_eq!(opt(&args, "--input-format"), "atmos_mezz");
    assert!(Path::new(adm).exists() && temp_is_empty(&env));
    // the rest has no master input
    for format in ["thd", "ac4", "aac"] {
        assert_error(
            &env.deew(&["-i", adm, "-f", format]),
            &format!(
                "Atmos masters (ADM BWF, DAMF) can't be encoded to {format}, use ddp-atmos, thd-atmos, ac4-atmos or ac4-ims (or dd/ddp for a 5.1 render)."
            ),
        );
    }
    // dd/ddp take a master alone
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    assert_error(
        &env.deew(&["-i", adm, "in.wav", "-f", "ddp"]),
        "Atmos masters (ADM BWF, DAMF) can't be encoded to dd/ddp together with other inputs.",
    );
}

#[test]
fn plain_wav_and_caf_are_no_masters() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    env.media("in.caf", "5.1", 48000, "pcm_s24le");
    for name in ["in.wav", "in.caf"] {
        let out = env.deew(&["-i", name, "-f", "ddp"]);
        ok(&out);
        let text = stdout(&out);
        assert_eq!(summary_row(&text, "Channels"), "5.1", "{name}");
        let args = env.run("dee_ddp_encoder");
        assert_eq!(opt(&args, "--input-format"), "wav", "{name}");
        assert!(temp_is_empty(&env), "{name}");
    }
    // a wav that is no ADM is no input of the Atmos formats
    let out = env.deew(&["-i", "in.wav", "-f", "ac4-atmos"]);
    assert_eq!(out.status.code(), Some(1));
}

// ---------------------------------------------------------------------------
// `--probe-json` (hidden, for the GUI)

impl Env {
    /// Run `deew --probe-json <args>`: the JSON of stdout (nothing else may
    /// be there) and the exit code.
    fn probe_json(&self, args: &[&str], envs: &[(&str, &str)]) -> (serde_json::Value, Option<i32>) {
        let mut all = vec!["--probe-json"];
        all.extend_from_slice(args);
        let out = self.deew_with(&all, envs);
        let text = stdout(&out);
        let value: serde_json::Value =
            serde_json::from_str(&text).unwrap_or_else(|e| panic!("{e}: {text:?}"));
        assert!(
            text.ends_with("}\n") && text.matches('\n').count() == 1,
            "{text:?}"
        );
        assert!(
            out.stderr.is_empty(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        (value, out.status.code())
    }
}

#[test]
fn probe_json_has_the_tags_of_the_track() {
    let env = env_or_skip!();
    env.tagged("t.mka", "eng", "Main");
    let (json, code) = env.probe_json(&["-i", "t.mka"], &[]);
    assert_eq!(code, Some(0));
    assert_eq!(json["version"], 1);
    assert_eq!(json["files"][0]["language"], "eng");
    assert_eq!(json["files"][0]["title"], "Main");
}

#[test]
fn probe_json_of_files_and_tracks() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    env.mka("two.mka", true);
    let config = fs::read(env.config()).unwrap();
    let (json, code) = env.probe_json(&["-i", "in.wav", "two.mka"], &[]);
    assert_eq!(code, Some(0));
    assert_eq!(json["version"], 1);
    assert_eq!(json["error"], serde_json::Value::Null);
    assert_eq!(json["ffprobe"], env.path("bin/ffprobe").to_str().unwrap());
    let files = json["files"].as_array().unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(files[0]["path"], "in.wav");
    assert_eq!(files[0]["channels"], 6);
    assert_eq!(files[0]["sampleRate"], 48000);
    assert_eq!(files[0]["bitDepth"], 24);
    assert_eq!(files[0]["codec"], "pcm_s24le");
    assert_eq!(files[0]["atmos"], false);
    assert_eq!(files[0]["master"], false);
    assert_eq!(files[0]["error"], serde_json::Value::Null);
    assert_eq!(files[0]["skipped"], false);
    // no tags: null
    assert_eq!(files[0]["language"], serde_json::Value::Null);
    assert_eq!(files[0]["title"], serde_json::Value::Null);
    // the first audio track of the mka is AC-3
    assert_eq!(files[1]["codec"], "ac3");
    assert_eq!(files[1]["channels"], 6);
    // -ti: the second audio track is the TrueHD one with the Atmos profile
    let (json, code) = env.probe_json(&["-ti", "1", "-i", "two.mka"], &[]);
    assert_eq!(code, Some(0));
    let file = &json["files"][0];
    assert_eq!(
        (file["codec"].as_str(), file["atmos"].as_bool()),
        (Some("truehd"), Some(true))
    );
    assert!(file["channels"].as_i64().is_some_and(|c| c > 0));
    // `s:N`: the stream index of the container
    let (json, _) = env.probe_json(&["-ti", "s:1", "-i", "two.mka"], &[]);
    assert_eq!(json["files"][0]["codec"], "truehd");
    // a track that isn't there is an error of that file, not a skip
    let (json, code) = env.probe_json(&["-ti", "5", "-i", "two.mka"], &[]);
    assert_eq!(code, Some(0));
    assert_eq!(
        json["files"][0]["error"],
        "no audio track 5 (-ti), it has 2 (0-1)"
    );
    assert_eq!(json["files"][0]["skipped"], false);
    assert_eq!(json["files"][0]["channels"], serde_json::Value::Null);
    let (json, _) = env.probe_json(&["-ti", "s:9", "-i", "two.mka"], &[]);
    assert!(
        json["files"][0]["error"]
            .as_str()
            .unwrap()
            .starts_with("no stream 9")
    );
    // other flags are ignored
    let (json, code) = env.probe_json(&["-np", "-f", "ac4", "-b", "999", "-i", "in.wav"], &[]);
    assert_eq!(
        (code, json["files"][0]["channels"].as_i64()),
        (Some(0), Some(6))
    );
    // nothing is created or changed
    assert_eq!(fs::read(env.config()).unwrap(), config);
    assert!(!env.temp().exists());
}

#[test]
fn probe_json_folder_skips_and_errors() {
    let env = env_or_skip!();
    let dir = env.work().join("dir");
    fs::create_dir_all(&dir).unwrap();
    fs::copy(
        env.media("a.wav", "stereo", 48000, "pcm_s16le"),
        dir.join("a.wav"),
    )
    .unwrap();
    fs::write(dir.join("notes.txt"), "not audio").unwrap();
    let (json, code) = env.probe_json(&["-i", "dir"], &[]);
    assert_eq!(code, Some(0));
    let files = json["files"].as_array().unwrap();
    assert_eq!(files.len(), 2);
    assert_eq!(
        files[0]["path"],
        format!("dir{}a.wav", std::path::MAIN_SEPARATOR)
    );
    assert_eq!(files[0]["channels"], 2);
    assert_eq!(files[0]["bitDepth"], 16);
    assert_eq!(files[0]["skipped"], false);
    assert_eq!(files[1]["skipped"], true);
    assert!(files[1]["error"].as_str().unwrap().starts_with("ffprobe: "));
    // the same file given directly is an error, not a skip
    let (json, code) = env.probe_json(&["-i", "dir/notes.txt"], &[]);
    assert_eq!(code, Some(0));
    assert_eq!(json["files"][0]["skipped"], false);
    assert!(
        json["files"][0]["error"]
            .as_str()
            .unwrap()
            .starts_with("ffprobe: ")
    );
    // a pattern, and the same file twice
    let (json, _) = env.probe_json(&["-i", "dir/*.wav", "dir/a.wav"], &[]);
    assert_eq!(json["files"].as_array().unwrap().len(), 1);
}

#[test]
fn probe_json_fatal_problems() {
    let env = env_or_skip!();
    let fatal = |args: &[&str], envs: &[(&str, &str)]| {
        let (json, code) = env.probe_json(args, envs);
        assert_eq!(code, Some(1), "{json}");
        assert_eq!(json["version"], 1);
        assert_eq!(json["files"], serde_json::json!([]));
        json["error"].as_str().unwrap().to_string()
    };
    assert_eq!(
        fatal(&["-i", "nope.wav"], &[]),
        "path nope.wav does not exist."
    );
    assert_eq!(fatal(&["-i", "*.nope"], &[]), "no files match '*.nope'");
    assert_eq!(
        fatal(&[], &[]),
        "the following arguments are required: -i/--input"
    );
    fs::create_dir_all(env.work().join("empty-dir")).unwrap();
    assert_eq!(fatal(&["-i", "empty-dir"], &[]), "no input files found.");
    assert_eq!(
        fatal(&["-ti", "x", "-i", "a.wav"], &[]),
        "argument -ti/--track-index: invalid int value: 'x'"
    );
    // no ffprobe next to deew or in the PATH
    fs::remove_file(env.path("bin/ffprobe")).unwrap();
    let error = fatal(&["-i", "a.wav"], &[("PATH", "/nonexistent")]);
    assert!(error.contains("ffprobe"), "{error}");
}

#[test]
fn probe_json_needs_no_config_and_makes_none() {
    let env = env_or_skip!();
    env.media("in.wav", "stereo", 48000, "pcm_s16le");
    fs::remove_file(env.config()).unwrap();
    let (json, code) = env.probe_json(&["-i", "in.wav"], &[]);
    assert_eq!(code, Some(0));
    assert_eq!(json["files"][0]["channels"], 2);
    assert!(!env.config().exists());
    assert!(!env.temp().exists());
}

#[test]
fn probe_json_of_atmos_masters() {
    let env = env_or_skip!();
    env.adm("film.wav");
    let dir = env.work().join("set");
    env.damf(&dir, "mix", true);
    env.damf(&env.work().join("broken"), "b", false);
    let (json, code) = env.probe_json(&["-ti", "s:3", "-i", "film.wav", "set"], &[]);
    assert_eq!(code, Some(0));
    let files = json["files"].as_array().unwrap();
    // the parts of the DAMF are no inputs
    assert_eq!(files.len(), 2);
    for (file, label) in [
        (&files[0], "ADM BWF Atmos master"),
        (&files[1], "DAMF Atmos master"),
    ] {
        assert_eq!(file["codec"], label);
        assert_eq!(file["master"], true);
        assert_eq!(file["atmos"], true);
        assert_eq!(file["channels"], 6);
        assert_eq!(file["sampleRate"], 48000);
        assert_eq!(file["bitDepth"], 24);
        assert_eq!(file["error"], serde_json::Value::Null);
    }
    let (json, code) = env.probe_json(&["-i", "broken/b.atmos"], &[]);
    assert_eq!(code, Some(0));
    let file = &json["files"][0];
    assert_eq!(file["skipped"], false);
    assert_eq!(file["master"], false);
    let error = file["error"].as_str().unwrap();
    assert!(
        error.starts_with("the DAMF Atmos master is incomplete, it needs "),
        "{error}"
    );
    assert!(
        error.contains("b.atmos.metadata") && error.contains("b.atmos.audio"),
        "{error}"
    );
}

// ---------------------------------------------------------------------------
// Output names: -on, -ot and the config's template

#[test]
fn output_name_names_the_single_output() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    // a bare name goes to -o, the extension is added
    let out = env.deew(&["-i", "in.wav", "-on", "Movie {x}"]);
    ok(&out);
    assert!(env.out().join("Movie {x}.ec3").exists());
    assert!(!env.out().join("in.ec3").exists());
    let text = stdout(&out);
    assert_eq!(summary_row(&text, "File name"), "Movie {x}.ec3", "{text}");
    let args = env.run("dee_ddp_encoder");
    assert_eq!(
        opt(&args, "-o"),
        env.out().join("Movie {x}.ec3").to_str().unwrap()
    );
    // the extension is not doubled (case-insensitive), another part is a name
    ok(&env.deew(&["-i", "in.wav", "-on", "Two.EC3"]));
    assert!(env.out().join("Two.EC3").exists());
    ok(&env.deew(&["-i", "in.wav", "-on", "Three.commentary"]));
    assert!(env.out().join("Three.commentary.ec3").exists());
    // illegal characters
    ok(&env.deew(&["-i", "in.wav", "-on", "a:b*c"]));
    assert!(env.out().join("a_b_c.ec3").exists());
    // a bare name goes next to the input with -oi
    ok(&env.deew_with(&["-np", "-oi", "-i", "in.wav", "-on", "Next"], &[]));
    assert!(env.work().join("Next.ec3").exists());
    assert!(temp_is_empty(&env));
}

#[test]
fn output_name_with_a_folder_is_an_error() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    let message =
        "invalid output name: the name can't contain a path separator, use -o for the folder";
    for name in ["a/b", "a\\b", "new/deep/Movie"] {
        assert_error(&env.deew(&["-i", "in.wav", "-on", name]), message);
    }
    let absolute = env.path("elsewhere/Abs");
    assert_error(
        &env.deew_with(
            &[
                "-np",
                "-oi",
                "-i",
                "in.wav",
                "-on",
                absolute.to_str().unwrap(),
            ],
            &[],
        ),
        message,
    );
    assert!(!env.work().join("a").exists());
    assert!(!env.work().join("new").exists());
    assert!(!env.path("elsewhere").exists());
    assert!(!env.out().join("b.ec3").exists());
    assert!(temp_is_empty(&env));
}

#[test]
fn output_name_needs_exactly_one_input() {
    let env = env_or_skip!();
    env.media("a.wav", "5.1", 48000, "pcm_s24le");
    env.media("b.wav", "5.1", 48000, "pcm_s24le");
    let message = "-on/--output-name only works with a single input file (found 2), \
                   use -ot/--output-template for several.";
    assert_error(&env.deew(&["-i", "a.wav", "b.wav", "-on", "x"]), message);
    // folders and wildcards count after they are expanded
    assert_error(&env.deew(&["-i", ".", "-on", "x"]), message);
    assert_error(&env.deew(&["-i", "*.wav", "-on", "x"]), message);
    assert!(!env.out().join("x.ec3").exists());
    ok(&env.deew(&["-i", "a*.wav", "-on", "x"]));
    assert!(env.out().join("x.ec3").exists());
    fs::create_dir_all(env.work().join("one")).unwrap();
    fs::rename(env.work().join("b.wav"), env.work().join("one/b.wav")).unwrap();
    ok(&env.deew(&["-i", "one", "-on", "y"]));
    assert!(env.out().join("y.ec3").exists());
}

#[test]
fn output_name_extension_rules() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    let out = env.deew(&["-i", "in.wav", "-f", "dd", "-on", "x.ec3"]);
    assert_error(
        &out,
        "invalid output name: in.wav: the name ends with .ec3, but the extension of this output is .ac3, leave it out or end the name with .ac3",
    );
    ok(&env.deew(&["-i", "in.wav", "-f", "dd", "-on", "x.ac3"]));
    assert!(env.out().join("x.ac3").exists());
    // Blu-ray 7.1 is .eb3
    env.media("in8.wav", "7.1", 48000, "pcm_s24le");
    let out = env.deew(&["-i", "in8.wav", "--ddp-mode", "bluray", "-on", "x.ec3"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stdout(&out).contains("extension of this output is .eb3"));
    assert!(stdout(&out).contains("invalid output name"));
}

#[test]
fn output_name_conflicts() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    assert_error(
        &env.deew(&["-i", "in.wav", "-on", "x", "-ot", "{stem}"]),
        "-on/--output-name and -ot/--output-template can't be used together.",
    );
    for option in ["-on", "-ot"] {
        assert_error(
            &env.deew(&["-i", "in.wav", "-mo", option, "x"]),
            "-on/--output-name and -ot/--output-template don't work with -mo, it writes no output file.",
        );
    }
    assert!(!env.out().join("x.ec3").exists());
}

#[test]
fn output_template_placeholders() {
    let env = env_or_skip!();
    env.tagged("x DELAY -24ms.mka", "eng", "Main: cut/1");
    env.media("plain.wav", "5.1", 48000, "pcm_s24le");
    let out = env.deew(&[
        "-i",
        "x DELAY -24ms.mka",
        "plain.wav",
        "-ot",
        "{stem}.{lang}.{title} {name} {format} {codec} {channels} {bitrate} {track}",
    ]);
    ok(&out);
    let tagged = "x.eng.Main_ cut_1 x DELAY -24ms ddp DDP 5.1 1024 0.ec3";
    assert!(env.out().join(tagged).exists(), "{}", stdout(&out));
    // no tags: the values drop out with a separator
    let plain = "plain.plain ddp DDP 5.1 1024 0.ec3";
    assert!(env.out().join(plain).exists(), "{}", stdout(&out));
    let text = stdout(&out);
    assert!(text.contains("│ File name"), "{text}");
    assert!(
        text.contains("x.eng.Main_ cut_1 x DELAY -24ms ddp"),
        "{text}"
    );
    // the report of -kl is named like the output
    ok(&env.deew(&["-i", "plain.wav", "-ot", "{stem}.{codec}", "-kl"]));
    assert!(env.out().join("plain.DDP.ec3").exists());
    assert!(env.out().join("plain.DDP.deew-log.html").exists());
}

#[test]
fn output_template_follows_the_output() {
    let env = env_or_skip!();
    env.media("in.wav", "7.1", 48000, "pcm_s24le");
    ok(&env.deew(&[
        "-i",
        "in.wav",
        "-f",
        "ac4",
        "-dm",
        "2",
        "-b",
        "128",
        "-ti",
        "a:0",
        "-ot",
        "{stem}_{codec}_{channels}_{bitrate}_{track}_{format}",
    ]));
    assert!(env.out().join("in_AC4_2.0_128_a_0_ac4.ac4").exists());
    env.truehd("atmos.thd");
    ok(&env.deew(&[
        "-i",
        "atmos.thd",
        "-f",
        "ddp-atmos",
        "-ot",
        "{stem}.{channels}.{lang}.{codec}",
    ]));
    assert!(env.out().join("atmos.5.1.DDP Atmos.ec3").exists());
    ok(&env.deew(&[
        "-i",
        "atmos.thd",
        "-f",
        "ddp-atmos",
        "--ddp-atmos-mode",
        "bluray",
        "-ot",
        "{stem}.{channels}",
    ]));
    assert!(env.out().join("atmos.7.1.eb3").exists());
}

#[test]
fn output_template_errors() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    for (template, fragment) in [
        ("{nope}", "unknown placeholder {nope}"),
        ("{stem", "unbalanced brace"),
        ("stem}", "unbalanced brace"),
        (
            "a/{stem}",
            "can't contain a path separator, use -o for the folder",
        ),
        ("{stem}.ac3", "the extension of this output is .ec3"),
    ] {
        let out = env.deew(&["-i", "in.wav", "-ot", template]);
        assert_eq!(out.status.code(), Some(1), "{template}");
        let text = stdout(&out);
        assert!(
            text.contains("invalid output template: "),
            "{template}: {text}"
        );
        assert!(text.contains(fragment), "{template}: {text}");
    }
    let out = env.deew(&["-i", "in.wav", "-ot", "{nope}"]);
    assert!(stdout(&out).contains("{stem}, {name}, {format}, {codec}, {channels}"));
    // a template that leaves nothing
    let out = env.deew(&["-i", "in.wav", "-ot", "{lang}"]);
    assert!(stdout(&out).contains("the output name is empty"));
    assert!(fs::read_dir(env.out()).map_or(true, |mut d| d.next().is_none()));
}

#[test]
fn outputs_must_not_clash() {
    let env = env_or_skip!();
    fs::create_dir_all(env.work().join("a")).unwrap();
    fs::create_dir_all(env.work().join("b")).unwrap();
    for name in ["a/x.wav", "b/X.wav"] {
        let target = env.media("tmp.wav", "5.1", 48000, "pcm_s24le");
        fs::rename(target, env.work().join(name)).unwrap();
    }
    // the same name in two folders, one out folder; compared without case
    let out = env.deew(&["-i", "a/x.wav", "b/X.wav"]);
    assert_eq!(out.status.code(), Some(1));
    let text = stdout(&out);
    assert!(
        text.contains("more than one output would be written to the same file"),
        "{text}"
    );
    assert!(text.contains("a/x.wav, b/X.wav"), "{text}");
    assert!(!env.out().join("x.ec3").exists());
    // next to the inputs there is no clash
    ok(&env.deew_with(&["-np", "-oi", "-i", "a/x.wav", "b/X.wav"], &[]));
    assert!(env.work().join("a/x.ec3").exists() && env.work().join("b/X.ec3").exists());
}

#[test]
fn an_output_must_not_be_its_input() {
    let env = env_or_skip!();
    env.media("in.ec3", "5.1", 48000, "eac3");
    let out = env.deew_with(&["-np", "-oi", "-i", "in.ec3"], &[]);
    assert_eq!(out.status.code(), Some(1));
    let text = stdout(&out);
    assert!(
        text.contains("an output would overwrite its own input file"),
        "{text}"
    );
    assert!(text.contains("in.ec3 (the output of "), "{text}");
    // another name is fine
    ok(&env.deew_with(&["-np", "-oi", "-i", "in.ec3", "-on", "other"], &[]));
    assert!(env.work().join("other.ec3").exists());
}

#[test]
fn config_template() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    let cfg = env.config();
    let base = fs::read_to_string(&cfg).unwrap();
    // missing key: {stem}
    ok(&env.deew(&["-i", "in.wav"]));
    assert!(env.out().join("in.ec3").exists());
    fs::remove_file(env.out().join("in.ec3")).unwrap();
    fs::write(
        &cfg,
        format!("{base}[output]\ntemplate = '{{stem}} [{{codec}}]'\n"),
    )
    .unwrap();
    ok(&env.deew(&["-i", "in.wav"]));
    assert!(env.out().join("in [DDP].ec3").exists());
    // -ot overrides it, -on replaces it
    ok(&env.deew(&["-i", "in.wav", "-ot", "{stem}.{track}"]));
    assert!(env.out().join("in.0.ec3").exists());
    ok(&env.deew(&["-i", "in.wav", "-on", "named"]));
    assert!(env.out().join("named.ec3").exists());
    // -mo names nothing
    ok(&env.deew(&["-i", "in.wav", "-mo"]));
    // an invalid one is a config error
    fs::write(&cfg, format!("{base}[output]\ntemplate = '{{nope}}'\n")).unwrap();
    let out = env.deew(&["-i", "in.wav"]);
    assert_eq!(out.status.code(), Some(1));
    let text = stdout(&out);
    assert!(text.contains("invalid config"), "{text}");
    assert!(text.contains("unknown placeholder {nope}"), "{text}");
}

impl Env {
    /// A Matroska audio file with a 24-bit FLAC track.
    fn flac_mka(&self, name: &str, layout: &str, bits: &str) -> PathBuf {
        let path = self.work().join(name);
        let status = command("ffmpeg")
            .args(["-v", "error", "-y", "-f", "lavfi", "-i"])
            .arg(format!(
                "anullsrc=channel_layout={layout}:sample_rate=48000"
            ))
            .args(["-t", "2", "-c:a", "flac", "-sample_fmt", bits])
            .arg(&path)
            .status()
            .unwrap();
        assert!(status.success(), "ffmpeg failed to generate {name}");
        path
    }
}

#[test]
fn auro_in_flac_goes_through_a_wav_pipe_into_harletty() {
    let env = env_or_skip!();
    env.flac_mka("x.mka", "7.1", "s32");
    assert_eq!(
        ffprobe_field(&env.work().join("x.mka"), "bits_per_raw_sample"),
        "24"
    );
    for format in ["ddp-atmos", "thd-atmos", "ac4-atmos", "ac4-ims"] {
        let out = env.deew_env(
            &["-i", "x.mka", "-f", format, "-wm", "loro", "-la"],
            &[("FAKE_SPATIAL", "auro")],
        );
        ok(&out);
        let text = stdout(&out);
        assert!(
            text.contains("-c:a pcm_s24le -f wav - | harletty --codec wav "),
            "{text}"
        );
        let info = env.harletty_run("info").unwrap();
        assert_eq!(
            info,
            ["--codec", "wav", "--loglevel", "off", "info", "--json", "-"]
        );
        let decode = env.harletty_run("decode").unwrap();
        assert_eq!(decode[..2], ["--codec", "wav"]);
        assert_eq!(decode.last().unwrap(), "-");
        assert_eq!(opt(&decode, "--warp-mode"), "loro");
        assert!(has(&decode, "--bed-conform"));
        let bytes: u64 = fs::read_to_string(env.path("bin/harletty.stdin"))
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert!(bytes > 1000, "{bytes}");
        assert!(env.truehdd_run().is_none());
        assert!(temp_is_empty(&env), "{format}");
        assert!(
            env.out().join("x.ec3").exists()
                || env.out().join("x.thd").exists()
                || env.out().join("x.ac4").exists()
        );
    }
}

#[test]
fn flac_with_a_harletty_that_cannot_read_pcm_is_as_before() {
    let env = env_or_skip!();
    env.flac_mka("x.mka", "7.1", "s32");
    let envs = [("FAKE_SPATIAL", "auro"), ("FAKE_REJECT_WAV", "1")];
    let out = env.deew_env(&["-i", "x.mka", "-f", "ddp-atmos"], &envs);
    assert_error(
        &out,
        "ddp-atmos, thd-atmos and ac4-atmos need a TrueHD Atmos, DTS:X or Auro-3D (24-bit DTS-HD MA, FLAC, PCM or TrueHD with a harletty that reads PCM) input or an Atmos master, x.mka is none of them.",
    );
    assert!(env.harletty_run("decode").is_none());
    // no label, no message, as a plain source
    let out = env.deew_env(&["-i", "x.mka", "-f", "ddp"], &envs);
    ok(&out);
    assert!(!stdout(&out).contains("Auro"), "{}", stdout(&out));
}

#[test]
fn plain_flac_is_unchanged() {
    let env = env_or_skip!();
    env.flac_mka("x.mka", "7.1", "s32");
    let out = env.deew_env(
        &["-i", "x.mka", "-f", "ddp-atmos"],
        &[("FAKE_SPATIAL", "plain")],
    );
    assert_error(
        &out,
        "ddp-atmos, thd-atmos and ac4-atmos need a TrueHD Atmos, DTS:X or Auro-3D (24-bit DTS-HD MA, FLAC, PCM or TrueHD with a harletty that reads PCM) input or an Atmos master, x.mka is none of them.",
    );
    assert!(env.harletty_run("decode").is_none());
    // 16-bit FLAC is never asked
    let env = env_or_skip!();
    env.flac_mka("y.mka", "7.1", "s16");
    let out = env.deew_env(&["-i", "y.mka", "-f", "ddp"], &[("FAKE_SPATIAL", "auro")]);
    ok(&out);
    assert!(env.harletty_run("info").is_none());
}
