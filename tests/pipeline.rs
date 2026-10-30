//! End-to-end tests of the encoding pipeline with a fake `dee` script and
//! real ffmpeg/ffprobe (the tests are skipped when those are not installed).

#![cfg(target_os = "linux")]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

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

/// A fake DEE: prints its version without arguments, otherwise reads the
/// job XML, checks that the input exists and "encodes" by writing a small
/// output file.
const FAKE_DEE: &str = r#"#!/bin/sh
if [ $# -eq 0 ]; then
  echo "dee, Version 5.2.1, Jun 10 2022"
  exit 0
fi
xml=""
while [ $# -gt 0 ]; do
  case "$1" in
    -x) xml="$2"; shift ;;
  esac
  shift
done
quoted() { sed -n "s|.*<$1>\"\(.*\)\"</$1>.*|\1|p"; }
input=$(sed -n '/<input>/,/<\/input>/p' "$xml")
in_dir=$(printf '%s\n' "$input" | quoted path)
in_name=$(printf '%s\n' "$input" | quoted file_name)
if [ ! -f "$in_dir/$in_name" ]; then
  echo "fatal: missing input $in_dir/$in_name"
  exit 3
fi
output=$(sed -n '/<output>/,/<\/output>/p' "$xml")
out_dir=$(printf '%s\n' "$output" | quoted path)
out_name=$(printf '%s\n' "$output" | quoted file_name)
echo "Stage progress: 20.0"
echo "Stage progress: 100.0"
echo "measured_loudness=-23.4"
echo "Stage progress: 50.0"
if [ -n "$FAKE_DEE_ERROR" ]; then echo "2022-06-10 error: $FAKE_DEE_ERROR"; fi
if [ -n "$FAKE_DEE_SLEEP" ]; then exec sleep "$FAKE_DEE_SLEEP"; fi
printf 'fake dee output' > "$out_dir/$out_name"
echo "Stage progress: 90.0"
"#;

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

/// A test environment: config, temp and output directories.
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
        let dee = env.path("dee");
        fs::write(&dee, FAKE_DEE).unwrap();
        fs::set_permissions(&dee, fs::Permissions::from_mode(0o755)).unwrap();
        fs::create_dir_all(env.path("xdg")).unwrap();
        fs::create_dir_all(env.path("bin")).unwrap();
        // a private copy of the binary: the config lives next to it
        let copy = env.bin();
        install_bin(&copy);
        fs::create_dir_all(env.path("work")).unwrap();
        fs::write(
            env.config(),
            format!(
                "ffmpeg_path = 'ffmpeg'\nffprobe_path = 'ffprobe'\ndee_path = '{}'\n\
                 temp_path = '{}'\nlogo = 0\nmax_instances = 2\n\
                 [default_bitrates]\ndd_1_0 = 128\ndd_2_0 = 256\ndd_5_1 = 640\n\
                 ddp_1_0 = 128\nddp_2_0 = 256\nddp_5_1 = 1024\nddp_7_1 = 1536\nac4_2_0 = 320\n\
                 [summary_sections]\ndeew_info = false\nbinaries = true\ninput_info = true\n\
                 output_info = true\nother = true\n",
                dee.display(),
                env.temp().display()
            ),
        )
        .unwrap();
        Some(env)
    }

    /// The private copy of the binary.
    fn bin(&self) -> PathBuf {
        self.path(if cfg!(windows) {
            "bin/deew.exe"
        } else {
            "bin/deew"
        })
    }

    /// The config file next to the private copy of the binary.
    fn config(&self) -> PathBuf {
        self.path("bin/config.toml")
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
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
            .arg(&path)
            .status()
            .unwrap();
        assert!(status.success(), "ffmpeg failed to generate {name}");
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
        let out = self.out();
        let mut all = vec!["-np", "-o", out.to_str().unwrap()];
        all.extend_from_slice(args);
        self.deew_with(&all, &[])
    }

    fn xml(&self, stem: &str) -> String {
        fs::read_to_string(self.temp().join(format!("{stem}.xml")))
            .unwrap_or_else(|e| panic!("no xml for {stem}: {e}"))
    }
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).into_owned()
}

fn ok(o: &Output) {
    assert_eq!(o.status.code(), Some(0), "{}", stdout(o));
}

/// Text of the first `<tag>` element.
fn element(xml: &str, tag: &str) -> String {
    let open = format!("<{tag}>");
    let start = xml
        .find(&open)
        .unwrap_or_else(|| panic!("no <{tag}> in\n{xml}"))
        + open.len();
    let end = start + xml[start..].find(&format!("</{tag}>")).unwrap();
    xml[start..end].to_string()
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

macro_rules! env_or_skip {
    () => {
        match Env::new() {
            Some(env) => env,
            None => return,
        }
    };
}

#[test]
fn ddp_51_default_and_cleanup() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    let out = env.deew(&["-i", "in.wav"]);
    ok(&out);
    let text = stdout(&out);
    assert!(text.contains("Max instances"), "{text}");
    assert!(text.contains("Running the following commands:"), "{text}");
    assert!(text.contains("5.1"), "{text}");
    assert!(text.contains("1024 kbps"), "{text}");
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
    // the fake DEE wrote the output; the temp files are gone
    assert_eq!(
        fs::read(env.out().join("in.ec3")).unwrap(),
        b"fake dee output"
    );
    assert!(!env.temp().join("in.wav").exists());
    assert!(!env.temp().join("in.xml").exists());
}

#[test]
fn ddp_51_xml_and_kept_files() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    ok(&env.deew(&["-i", "in.wav", "-k"]));
    let xml = env.xml("in");
    assert_eq!(element(&xml, "encoder_mode"), "ddp");
    assert_eq!(element(&xml, "data_rate"), "1024");
    assert_eq!(element(&xml, "downmix_config"), "off");
    assert_eq!(element(&xml, "custom_dialnorm"), "0");
    assert_eq!(element(&xml, "line_mode_drc_profile"), "music_light");
    assert!(xml.contains("<ec3 version=\"1\">"));
    assert!(xml.contains("<file_name>\"in.ec3\"</file_name>"));
    assert!(xml.contains(&format!("<path>\"{}\"</path>", env.out().display())));
    assert!(xml.contains("<!--"), "template comments are kept");
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
    let out = env.deew(&["-i", "st.wav", "-f", "dd", "-b", "200", "-k"]);
    ok(&out);
    assert!(stdout(&out).contains("Consider using qaac or opus for mono and stereo encoding."));
    let xml = env.xml("st");
    assert_eq!(element(&xml, "data_rate"), "192");
    assert_eq!(element(&xml, "encoder_mode"), "dd");
    assert!(xml.contains("<ac3 version=\"1\">") && !xml.contains("<ec3"));
    assert!(xml.contains("<file_name>\"st.ac3\"</file_name>"));
    assert!(env.out().join("st.ac3").exists());
}

#[test]
fn ddp_71_bitrate_picks_profile() {
    let env = env_or_skip!();
    env.media("sur.wav", "7.1", 48000, "pcm_s24le");
    // default 1536 -> bluray profile
    ok(&env.deew(&["-i", "sur.wav", "-k"]));
    let xml = env.xml("sur");
    assert_eq!(element(&xml, "encoder_mode"), "bluray");
    assert_eq!(element(&xml, "data_rate"), "1536");
    assert!(xml.contains("<file_name>\"sur.eb3\"</file_name>"));
    assert!(env.out().join("sur.eb3").exists());

    // 1100 rounds to 1024 -> standard 7.1 profile
    ok(&env.deew(&["-i", "sur.wav", "-k", "-b", "1100"]));
    let xml = env.xml("sur");
    assert_eq!(element(&xml, "encoder_mode"), "ddp71");
    assert_eq!(element(&xml, "data_rate"), "1024");
    assert!(xml.contains("<file_name>\"sur.ec3\"</file_name>"));

    // -fb forces the bluray profile and extension
    ok(&env.deew(&["-i", "sur.wav", "-k", "-b", "1100", "-fb"]));
    let xml = env.xml("sur");
    assert_eq!(element(&xml, "encoder_mode"), "bluray");
    assert_eq!(element(&xml, "data_rate"), "1024");
    assert!(xml.contains("<file_name>\"sur.eb3\"</file_name>"));

    // -fs forces the standard profile
    ok(&env.deew(&["-i", "sur.wav", "-k", "-fs"]));
    let xml = env.xml("sur");
    assert_eq!(element(&xml, "encoder_mode"), "ddp71");
    assert_eq!(element(&xml, "data_rate"), "1024");
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
    ok(&env.deew(&["-i", "sur.wav", "-f", "dd", "-k"]));
    let xml = env.xml("sur");
    assert_eq!(element(&xml, "downmix_config"), "5.1");
    assert_eq!(element(&xml, "encoder_mode"), "dd");
    assert_eq!(element(&xml, "data_rate"), "640");
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
fn delay_in_the_file_name() {
    let env = env_or_skip!();
    env.media("x DELAY -24ms.wav", "5.1", 48000, "pcm_s24le");
    ok(&env.deew(&["-i", "x DELAY -24ms.wav", "-k"]));
    // sanitized xml name; the output name has no DELAY part
    let xml = env.xml("x_DELAY_-24ms");
    assert_eq!(element(&xml, "start"), "0:00:00.029333");
    assert_eq!(element(&xml, "prepend_silence_duration"), "0.0");
    assert!(xml.contains("<file_name>\"x.ec3\"</file_name>"));
    assert!(xml.contains("<file_name>\"x DELAY -24ms.wav\"</file_name>"));
    assert!(env.out().join("x.ec3").exists());
}

#[test]
fn delay_argument_overrides() {
    let env = env_or_skip!();
    env.media("d DELAY -24ms.wav", "5.1", 48000, "pcm_s24le");
    let out = env.deew(&["-i", "d DELAY -24ms.wav", "-k", "-d=+10@pal"]);
    ok(&out);
    assert_eq!(summary_row(&stdout(&out), "Delay"), "400ms");
    let xml = env.xml("d_DELAY_-24ms");
    assert_eq!(element(&xml, "prepend_silence_duration"), "0.394667");
    assert_eq!(element(&xml, "start"), "00:00:00.0");
}

#[test]
fn measure_only_kills_dee_after_the_measurement() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    let start = Instant::now();
    let out = env.deew_with(
        &[
            "-np",
            "-o",
            env.out().to_str().unwrap(),
            "-i",
            "in.wav",
            "-mo",
            "-f",
            "dd",
        ],
        &[("FAKE_DEE_SLEEP", "20")],
    );
    ok(&out);
    assert!(
        start.elapsed() < Duration::from_secs(15),
        "dee was not killed"
    );
    // the fake DEE never got to write its output
    assert!(!env.out().join("in.ec3").exists());
    assert!(!env.out().join("in.ac3").exists());
    let text = stdout(&out);
    // the format is overridden with ddp and the output section is not shown
    assert!(!text.contains("Dialnorm"), "{text}");
    assert!(!text.contains("Bitrate"), "{text}");
}

#[test]
fn dee_error_lines_are_printed() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    let out = env.deew_with(
        &["-np", "-o", env.out().to_str().unwrap(), "-i", "in.wav"],
        &[("FAKE_DEE_ERROR", "[boom] failed")],
    );
    ok(&out);
    let text = stdout(&out);
    assert!(text.lines().any(|l| l == "[boom] failed"), "{text}");
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
}

#[test]
fn long_argument_prints_every_command() {
    let env = env_or_skip!();
    env.media("a.wav", "5.1", 48000, "pcm_s24le");
    env.media("b.wav", "5.1", 48000, "pcm_s24le");
    let out = env.deew(&["-i", "a.wav", "b.wav", "-la"]);
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
    assert!(commands[0].contains("&& "), "{}", commands[0]);
    assert!(
        commands[0].ends_with("--disable-xml-validation"),
        "{}",
        commands[0]
    );
}

fn assert_error(out: &Output, message: &str) {
    assert_eq!(out.status.code(), Some(1), "{}", stdout(out));
    let text = stdout(out);
    assert!(text.contains(&format!("ERROR: {message}")), "{text}");
    assert!(!text.contains("panicked"), "{text}");
}

#[test]
fn ac4_needs_5_1() {
    let env = env_or_skip!();
    env.media("st.wav", "stereo", 48000, "pcm_s16le");
    assert_error(
        &env.deew(&["-i", "st.wav", "-f", "ac4"]),
        "input channels for AC4 encoding can only be 6.",
    );
}

#[test]
fn ac4_encodes_5_1() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    ok(&env.deew(&["-i", "in.wav", "-f", "ac4", "-k"]));
    let xml = env.xml("in");
    assert_eq!(element(&xml, "data_rate"), "320");
    assert_eq!(element(&xml, "ddp_drc_profile"), "music_light");
    assert!(xml.contains("<file_name>\"in.ac4\"</file_name>"));
    assert!(env.out().join("in.ac4").exists());
}

#[test]
fn thd_is_not_supported_with_a_native_dee() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    assert_error(
        &env.deew(&["-i", "in.wav", "-f", "thd"]),
        "Linux version of DEE does not support TrueHD encoding.",
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
    assert_eq!(summary_row(&text, "Bit depth"), "mixed bit depth");
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
        "input file couldn't be parsed by ffprobe, try to extract/remux it with ffmpeg or mkvmerge.",
    );
}

#[test]
fn argument_errors() {
    let env = env_or_skip!();
    env.media("a.wav", "5.1", 48000, "pcm_s24le");
    assert_error(
        &env.deew(&["-i", "a.wav", "-f", "mp3"]),
        "-f/--format can only be dd, ddp, ac4 or thd.",
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
    let config = env.config();
    let text = fs::read_to_string(&config).unwrap();
    fs::write(
        &config,
        text.replace("ffmpeg_path = 'ffmpeg'", "ffmpeg_path = 'no-such-ffmpeg'"),
    )
    .unwrap();
    assert_error(
        &env.deew(&["-i", "a.wav"]),
        "no-such-ffmpeg does not exist.",
    );
}

#[test]
fn unknown_duration_still_encodes() {
    let env = env_or_skip!();
    env.media("in.wav", "5.1", 48000, "pcm_s24le");
    // an ffprobe that does not report durations
    let wrapper = env.path("ffprobe-no-duration");
    fs::write(
        &wrapper,
        "#!/bin/sh\nexec ffprobe \"$@\" | sed 's/\"duration\": \"[^\"]*\",//'\n",
    )
    .unwrap();
    fs::set_permissions(&wrapper, fs::Permissions::from_mode(0o755)).unwrap();
    let config = env.config();
    let text = fs::read_to_string(&config).unwrap();
    let text = text.replace(
        "ffprobe_path = 'ffprobe'",
        &format!("ffprobe_path = '{}'", wrapper.display()),
    );
    fs::write(&config, text).unwrap();
    ok(&env.deew(&["-i", "in.wav"]));
    assert!(env.out().join("in.ec3").exists());
}

#[test]
fn dialnorm_note_with_no_prompt() {
    let env = env_or_skip!();
    env.media("a.wav", "5.1", 48000, "pcm_s24le");
    let out = env.deew(&["-i", "a.wav", "-dn", "-27", "-k"]);
    ok(&out);
    assert!(stdout(&out).contains(
        "Consider leaving the dialnorm value at 0 (auto), setting it manually can be dangerous."
    ));
    assert_eq!(element(&env.xml("a"), "custom_dialnorm"), "-27");
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
    let out = env.deew_with(
        &["-np", "-o", env.out().to_str().unwrap(), "-i", "in.wav"],
        &[
            ("DEEW_EVENTS_FILE", file.to_str().unwrap()),
            ("FAKE_DEE_ERROR", "boom"),
        ],
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
    assert!(progress.iter().any(|e| e["percent"] == 100.0));
    assert!(
        all.iter()
            .any(|e| e["type"] == "error" && e["text"] == "boom")
    );
    assert_eq!(
        all.last().unwrap(),
        &serde_json::json!({"type": "done", "job": 0})
    );
    // the final bar lines still reach the terminal log
    assert!(stdout(&out).contains("100.00%"), "{}", stdout(&out));
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
