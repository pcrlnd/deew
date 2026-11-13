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
input=""; output=""; tempdir=""; omanifest=""; imanifest=""
while [ $# -gt 0 ]; do
  case "$1" in
    -i) input="$2"; shift ;;
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
if [ "$tool" = dee_measure_loudness ]; then
  echo "[2026-11-01 22:03:14.420] Step 1/1 : Loudness Measurement"
  printf '[>          ] 0 %%\r[=====>     ] 50 %%\r[==========>] 100 %%\r\n'
  echo "[2026-11-01 22:03:14.570] [Source loudness] measured_loudness=-23.4"
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
  if [ -n "$FAKE_HANG_AFTER_MEASURED" ]; then exec sleep 120; fi
  echo "[2026-11-01 22:01:24.838] [Source loudness] gating=dialogue"
  echo "[2026-11-01 22:01:24.838] [Source loudness] speech_percentage=16.1"
  if [ -n "$FAKE_HANG_AFTER_BLOCK" ]; then exec sleep 120; fi
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
  echo "truehdd 0.6.2 (truehd library 0.7.2) Built: fake"
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

/// A fake dee_shim: runs the binary after `--` with the arguments after it
/// and glues a line of its own in front of the first line of DEE.
const FAKE_DSHIM: &str = r#"#!/bin/sh
dir=$(dirname "$0")
if [ "$1" = "--version" ]; then
  echo "dee_shim v0.1.0+282-g0301b3e"
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
/// real output.
const FAKE_FFPROBE: &str = r#"#!/bin/sh
if [ -n "$FAKE_NO_PROFILE" ]; then exec ffprobe "$@"; fi
# keep the exit code and stderr of ffprobe
tmp=$(mktemp)
ffprobe "$@" > "$tmp" 2> "$tmp.err"
status=$?
cat "$tmp.err" >&2
sed 's/"codec_name": "truehd",/"codec_name": "truehd", "profile": "Dolby TrueHD + Dolby Atmos",/' "$tmp"
rm -f "$tmp" "$tmp.err"
exit $status
"#;

const DEE_TOOLS: [&str; 7] = [
    "dee_ddp_encoder",
    "dee_ddpjoc_encoder",
    "dee_ac4_encoder",
    "dee_ac4ajoc_encoder",
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
            text.replace("pipe = false", &format!("pipe = {on}")),
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
    assert_eq!(summary_row(&text, "truehdd version"), "0.6.2");
    assert_eq!(summary_row(&text, "dee_shim version"), "0.1.0+282-g0301b3e");
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
    for flag in ["-nps", "--no-phase-shift"] {
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
        let out = env.deew(&["-i", "fv.wav", "-f", format, "-nps"]);
        assert_error(
            &out,
            "-nps/--no-phase-shift only works for DD, DDP and DDP Atmos.",
        );
    }
    // -mo ignores the format rules
    ok(&env.deew(&["-i", "fv.wav", "-mo", "-f", "thd", "-nps"]));
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
    // the 1600 sample priming of the AAC encoder is trimmed
    assert_eq!(opt(&args, "--start"), "0.033333");
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
fn measure_only_of_an_atmos_source_measures_it_as_atmos() {
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
        // the fake encoder would sleep for two minutes after the loudness
        let out = env.deew_env(&args, &[("FAKE_HANG_AFTER_BLOCK", "1")]);
        ok(&out);
        let text = stdout(&out);
        assert!(
            text.contains("finished: atmos.thd (-23 dB, dialogue-gated, 16% speech) - took "),
            "{index}: {text}"
        );
        let truehdd = env.truehdd_run().unwrap();
        assert_eq!(
            opt(&truehdd, "--output-path"),
            env.temp().join("atmos").to_str().unwrap()
        );
        let args = env.run("dee_ddpjoc_encoder");
        assert_eq!(opt(&args, "--input-format"), "atmos_mezz");
        assert_eq!(
            opt(&args, "-i"),
            env.temp().join("atmos.atmos").to_str().unwrap()
        );
        assert_eq!(opt(&args, "-e"), "drc_profile=none");
        assert_eq!(opt(&args, "--data-rate"), "1024");
        assert_eq!(opt(&args, "--loudness-management"), "measure_only");
        assert!(!has(&args, "--temp-dir"));
        assert!(env.dshim_run().is_none());
        assert!(env.last_run("dee_measure_loudness").is_none());
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
    assert_eq!(summary_row(&text, "dee_shim version"), "0.1.0+282-g0301b3e");
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
            "ddp-atmos, thd-atmos and ac4-atmos need a TrueHD input with an Atmos presentation, in.wav is not a TrueHD file.",
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
            "ddp-atmos, thd-atmos and ac4-atmos need a TrueHD input with an Atmos presentation, plain.thd has no Atmos.",
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
    // -mo measures 7.1 with the DDP encoder
    ok(&env.deew(&["-i", "sur.wav", "-mo"]));
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
        stdout(&out).contains("finished: sur.wav (-33 dB, dialogue-gated, 16% speech) - took "),
        "{}",
        stdout(&out)
    );
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    let out = env.deew_env(&["-i", "in.wav"], &loud);
    ok(&out);
    assert!(stdout(&out).contains("finished: in.wav (-31 dB) - took "));
}

#[test]
fn measure_only_ends_an_encoder_that_stops_printing_after_the_loudness() {
    let env = env_or_skip!();
    env.media("sur.wav", "7.1", 48000, "pcm_s24le");
    let started = std::time::Instant::now();
    let out = env.deew_env(
        &["-i", "sur.wav", "-mo"],
        &[("FAKE_HANG_AFTER_MEASURED", "1")],
    );
    ok(&out);
    assert!(started.elapsed().as_secs() < 60);
    assert!(stdout(&out).contains("finished: sur.wav (-23 dB) - took "));
    assert!(temp_is_empty(&env));
}

#[test]
fn measure_only_of_7_1_stops_the_ddp_encoder() {
    let env = env_or_skip!();
    env.media("sur.wav", "7.1", 48000, "pcm_s24le");
    let started = std::time::Instant::now();
    // the fake encoder would sleep for two minutes after the loudness
    let out = env.deew_env(
        &["-i", "sur.wav", "-mo", "-la"],
        &[("FAKE_HANG_AFTER_BLOCK", "1")],
    );
    ok(&out);
    assert!(started.elapsed().as_secs() < 60);
    let text = stdout(&out);
    let args = env.run("dee_ddp_encoder");
    assert_eq!(opt(&args, "-e"), "bluray:drc_profile=none");
    assert_eq!(opt(&args, "--loudness-management"), "measure_only");
    assert_eq!(opt(&args, "--data-rate"), "1024");
    assert!(env.last_run("dee_measure_loudness").is_none());
    assert!(text.contains("dee_ddp_encoder --progress 1"), "{text}");
    assert!(
        text.contains("finished: sur.wav (-23 dB, dialogue-gated, 16% speech) - took "),
        "{text}"
    );
    assert!(!env.out().join("sur.ec3").exists());
    assert!(temp_is_empty(&env));
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
    // AAC: 1600 samples, AC4: 2048 samples
    ok(&env.deew(&["-i", "in.wav", "-f", "aac", "-d", "+1s"]));
    assert_eq!(
        opt(&env.run("dee_aac_encoder"), "--add-silence"),
        "0.966667:0"
    );
    ok(&env.deew(&["-i", "in.wav", "-f", "aac", "-d", "+0ms"]));
    let args = env.run("dee_aac_encoder");
    assert_eq!(opt(&args, "--start"), "0.033333");
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
        text.contains("truehdd decode --presentation 3 --json --output-path [output] [input] && dee_ac4ajoc_encoder"),
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
        "-f/--format can only be dd, ddp, ddp-atmos, thd, thd-atmos, ac4, ac4-atmos or aac.",
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
    assert_eq!(
        all.last().unwrap(),
        &serde_json::json!({"type": "done", "job": 0})
    );
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
        text.contains("truehdd decode --presentation 3 --json --output-path "),
        "{text}"
    );
    assert_eq!(env.truehdd_run().unwrap().last().unwrap(), "atmos.thd");
    assert!(!env.path("bin").join("truehdd.stdin").exists());
    assert!(env.out().join("atmos.ec3").exists());
    let out = env.deew(&["-i", "atmos.thd", "-f", "ac4-atmos"]);
    assert!(stdout(&out).contains("truehdd decode --presentation 3 --json --output-path [output] [input] && dee_ac4ajoc_encoder"));
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
        "ddp-atmos, thd-atmos and ac4-atmos need a TrueHD input with an Atmos presentation, two.mka is not a TrueHD file.",
    );
    ok(&env.deew(&["-i", "two.mka", "-f", "ddp-atmos", "-ti", "1", "-k"]));
    let kept = fs::read_to_string(env.path("bin").join("truehdd.stdin")).unwrap();
    assert!(kept.trim().parse::<u64>().unwrap() > 100);
    assert!(env.out().join("two.ec3").exists());
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
    assert!(html.contains("6.7.0") && html.contains("0.6.2"));
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
        "0.1.0+282-g0301b3e"
    );
}

#[test]
fn five_and_seven_channel_inputs_get_a_silent_lfe_slot() {
    let env = env_or_skip!();
    // 5.0: a real 5.0 DDP, the wav for DEE has an empty LFE slot
    env.media("five.wav", "5.0", 48000, "pcm_s24le");
    let out = env.deew(&["-i", "five.wav", "-k", "-la"]);
    ok(&out);
    let text = stdout(&out);
    assert!(
        text.contains("pan=5.1|c0=c0|c1=c1|c2=c2|c3=0*c0|c4=c3|c5=c4"),
        "{text}"
    );
    assert_eq!(ffprobe_field(&env.temp().join("five.wav"), "channels"), "6");
    assert_eq!(last_summary_row(&text, "Channels"), "5.0");
    let args = env.run("dee_ddp_encoder");
    assert_eq!(opt(&args, "-e"), "ddp:drc_profile=none:lfe_on=0");
    assert!(!has(&args, "-c"));
    assert!(env.dshim_run().is_none());
    // DD as well
    ok(&env.deew(&["-i", "five.wav", "-f", "dd"]));
    assert_eq!(
        opt(&env.run("dee_ddp_encoder"), "-e"),
        "dd:drc_profile=none:lfe_on=0"
    );
    // a stereo downmix has no LFE to remove
    ok(&env.deew(&["-i", "five.wav", "-dm", "2"]));
    let args = env.run("dee_ddp_encoder");
    assert_eq!(opt(&args, "-e"), "ddp:drc_profile=none");
    assert_eq!(opt(&args, "-c"), "stereo");

    // 7.0 (FL FR FC BL BR SL SR): side pair to Ls/Rs, back pair to Lrs/Rrs
    env.media("seven.wav", "7.0", 48000, "pcm_s24le");
    let out = env.deew(&["-i", "seven.wav", "-k", "-la"]);
    ok(&out);
    let text = stdout(&out);
    assert!(
        text.contains("pan=7.1|c0=c0|c1=c1|c2=c2|c3=0*c0|c4=c5|c5=c6|c6=c3|c7=c4"),
        "{text}"
    );
    assert_eq!(
        ffprobe_field(&env.temp().join("seven.wav"), "channels"),
        "8"
    );
    assert_eq!(last_summary_row(&text, "Channels"), "7.0");
    assert_eq!(summary_row(&text, "Profile"), "Standard (dee_shim)");
    assert_eq!(opt(&env.dshim_run().unwrap(), "--layout"), "7.0/eac3");
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
    // downmix from 7.0
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
    // 7.1 is measured by the DDP encoder, which is stopped when it has the loudness
    env.media("in8.wav", "7.1", 48000, "pcm_s24le");
    let out = env.deew(&["-i", "in8.wav", "-mo"]);
    ok(&out);
    assert!(stdout(&out).contains("-23"), "{}", stdout(&out));
    let args = env.run("dee_ddp_encoder");
    assert_eq!(opt(&args, "-i"), "/dev/stdin");
    assert!(opt(&args, "--input-format").contains("channels=8"));
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
