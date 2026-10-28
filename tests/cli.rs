//! End-to-end tests of the command line interface. Stdout is piped, so no
//! colors are emitted and the output can be compared with the goldens
//! captured from the Python version.

use std::io::Write;
use std::path::Path;
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

fn golden(name: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(name);
    std::fs::read(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

/// Run deew with an isolated config home. `stdin` is fed to the process.
fn run_bin(bin: &Path, config_home: &Path, args: &[&str], stdin: &str) -> Output {
    let mut child = spawn_retry(
        Command::new(bin)
            .args(args)
            .env("XDG_CONFIG_HOME", config_home)
            .env_remove("NO_COLOR")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped()),
    );
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().unwrap()
}

/// Run the real binary; only for tests that never need a config.
fn run(args: &[&str]) -> Output {
    let home = tempfile::tempdir().unwrap();
    run_bin(Path::new(BIN), home.path(), args, "")
}

/// A private copy of the binary in its own temp dir, so that the config
/// (which lives next to the executable) never touches target/debug.
#[cfg(target_os = "linux")]
struct Sandbox {
    dir: tempfile::TempDir,
    bin: std::path::PathBuf,
}

#[cfg(target_os = "linux")]
impl Sandbox {
    fn new() -> Sandbox {
        let dir = tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR")).unwrap();
        let bin = dir
            .path()
            .join(if cfg!(windows) { "deew.exe" } else { "deew" });
        install_bin(&bin);
        std::fs::create_dir(dir.path().join("xdg")).unwrap();
        Sandbox { dir, bin }
    }

    fn config(&self) -> std::path::PathBuf {
        self.dir.path().join("config.toml")
    }

    fn run(&self, args: &[&str], stdin: &str) -> Output {
        run_bin(&self.bin, &self.dir.path().join("xdg"), args, stdin)
    }
}

#[cfg(target_os = "linux")]
fn stdout(o: &Output) -> String {
    String::from_utf8(o.stdout.clone()).unwrap()
}

fn assert_golden(args: &[&str], name: &str, code: i32) {
    let out = run(args);
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&golden(name)),
        "stdout of {args:?}"
    );
    assert_eq!(out.status.code(), Some(code), "exit code of {args:?}");
    assert!(out.stderr.is_empty(), "stderr of {args:?}");
}

#[test]
fn help() {
    assert_golden(&["-h"], "help.txt", 0);
    assert_golden(&["--help"], "help.txt", 0);
}

#[test]
fn no_args_prints_help_and_exits_1() {
    assert_golden(&[], "no_args.txt", 1);
}

#[test]
fn version() {
    assert_golden(&["-v"], "version.txt", 0);
    assert_golden(&["--version"], "version.txt", 0);
}

#[test]
fn list_bitrates() {
    assert_golden(&["-lb"], "list_bitrates.txt", 0);
}

#[test]
fn print_logos() {
    assert_golden(&["-pl"], "print_logos.txt", 0);
}

#[test]
fn usage_errors() {
    assert_golden(&["-x"], "err_unknown.txt", 2);
    assert_golden(&["-b", "abc"], "err_invalid_int.txt", 2);
    assert_golden(&["-b"], "err_missing_value.txt", 2);
    assert_golden(&["-d", "-5ms"], "err_negative_value.txt", 2);
    assert_golden(&["foo"], "err_positional.txt", 2);
}

#[test]
fn info_flags_win_over_other_arguments() {
    assert_golden(&["-lb", "-i", "x.mkv"], "list_bitrates.txt", 0);
}

fn template() -> String {
    std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("src/config_template.toml"))
        .unwrap()
}

#[test]
fn template_matches_python() {
    // The template is dumped from the Python source; spot-check its shape.
    let t = template();
    assert!(t.starts_with("# These are required.\n"));
    assert!(t.contains("max_instances = '50%'\n"));
    assert!(t.contains("    other = true\n"));
    assert!(t.ends_with("    pass = ''\n"));
}

#[cfg(target_os = "linux")]
mod linux {
    use super::*;

    #[test]
    fn generate_config_writes_template_without_asking() {
        let sb = Sandbox::new();
        let out = sb.run(&["-gc"], "");
        let text = stdout(&out);
        let path = sb.config();

        assert_eq!(out.status.code(), Some(0), "{text}");
        assert_eq!(std::fs::read_to_string(&path).unwrap(), template());
        assert!(!text.contains("Please choose"), "{text}");
        assert!(!text.contains("Location"), "{text}");
        assert!(
            text.ends_with(&format!(
                "\nThe above config has been created at:\n{}\n",
                path.display()
            )),
            "{text}"
        );
        assert!(text.contains(&template()), "{text}");
    }

    #[test]
    fn missing_config_is_auto_created_and_exits_1() {
        let sb = Sandbox::new();
        let out = sb.run(&["-i", "x.mkv"], "");
        let text = stdout(&out);
        assert_eq!(out.status.code(), Some(1), "{text}");
        assert!(text.contains("is missing, creating one..."), "{text}");
        assert_eq!(std::fs::read_to_string(sb.config()).unwrap(), template());
    }

    #[test]
    fn config_without_config() {
        let sb = Sandbox::new();
        let out = sb.run(&["-c"], "");
        let text = stdout(&out);
        assert_eq!(out.status.code(), Some(0));
        assert_eq!(
            text,
            format!(
                "Your config location:\n{}\n\nYour current config:\nYou don't have a config currently.\n",
                sb.config().display()
            )
        );
    }

    #[test]
    fn config_with_config() {
        let sb = Sandbox::new();
        let content = "logo = 3\n# [not markup]\n[bold]\n";
        std::fs::write(sb.config(), content).unwrap();
        let out = sb.run(&["-c"], "");
        let text = stdout(&out);
        assert_eq!(out.status.code(), Some(0));
        assert!(
            text.starts_with(&format!(
                "Your config location:\n{}\n\nYour current config:\n",
                sb.config().display()
            )),
            "{text}"
        );
        assert!(
            text.contains("\nYour current config:\nlogo = 3\n# [not markup]\n[bold]\n"),
            "{text}"
        );
    }

    #[test]
    fn missing_config_is_generated() {
        let sb = Sandbox::new();
        let out = sb.run(&["-i", "x.mkv"], "");
        let text = stdout(&out);
        assert_eq!(out.status.code(), Some(1), "{text}");
        assert!(
            text.starts_with("config.toml is missing, creating one...\n"),
            "{text}"
        );
        assert!(!text.contains("Please choose"), "{text}");
        assert_eq!(std::fs::read_to_string(sb.config()).unwrap(), template());
    }

    fn with_config(content: &str, args: &[&str]) -> (Output, String) {
        let sb = Sandbox::new();
        std::fs::write(sb.config(), content).unwrap();
        let out = sb.run(args, "");
        let text = stdout(&out);
        (out, text)
    }

    #[test]
    fn missing_keys_are_listed() {
        let (out, text) = with_config("logo = 0\ntemp_path = ''\n", &["-i", "x.mkv"]);
        assert_eq!(out.status.code(), Some(1), "{text}");
        assert_eq!(
            text,
            "ERROR: the following keys are missing from your config file: \
ffmpeg_path, ffprobe_path, dee_path, max_instances, default_bitrates, summary_sections.\n\
Update your config file.\n"
        );
    }

    #[test]
    fn logo_is_printed_before_the_key_check() {
        let (out, text) = with_config("logo = 2\n", &["-i", "x.mkv"]);
        assert_eq!(out.status.code(), Some(1), "{text}");
        assert!(
            text.starts_with(" ▄▄▄▄▄  ▄▄▄▄▄ ▄▄▄▄▄ ▄▄  ▄▄  ▄▄\n"),
            "{text}"
        );
        assert!(
            text.contains("ERROR: the following keys are missing"),
            "{text}"
        );
        // out of range: no logo
        let (_, text) = with_config("logo = 12\n", &["-i", "x.mkv"]);
        assert!(text.starts_with("ERROR:"), "{text}");
    }

    #[test]
    fn invalid_toml_is_a_clean_error() {
        let (out, text) = with_config("logo = [\n", &["-i", "x.mkv"]);
        assert_eq!(out.status.code(), Some(1), "{text}");
        assert!(
            text.starts_with("ERROR: failed to parse config.toml: "),
            "{text}"
        );
        assert!(text.contains("config.toml"), "{text}");
        assert!(!text.contains("panicked"), "{text}");
    }

    #[test]
    fn type_errors_are_clean_errors() {
        let bad = template().replace("logo = 1", "logo = 'one'");
        let (out, text) = with_config(&bad, &["-i", "x.mkv"]);
        assert_eq!(out.status.code(), Some(1), "{text}");
        assert!(text.starts_with("ERROR: invalid config "), "{text}");
        assert!(!text.contains("panicked"), "{text}");

        let bad = template().replace("    ddp_7_1 = 1536\n", "");
        let (out, text) = with_config(&bad, &["-i", "x.mkv"]);
        assert_eq!(out.status.code(), Some(1), "{text}");
        assert!(text.contains("ddp_7_1"), "{text}");
    }

    #[test]
    fn input_is_required_after_config_loading() {
        let (out, text) = with_config(&template().replace("logo = 1", "logo = 0"), &["-k"]);
        assert_eq!(out.status.code(), Some(2), "{text}");
        assert!(
            text.ends_with("deew: error: the following arguments are required: -i/--input\n"),
            "{text}"
        );
        assert!(text.starts_with("deew 3.6.1\n\nUSAGE: deew"), "{text}");
    }

    #[test]
    fn info_flags_do_not_need_input_or_config() {
        let sb = Sandbox::new();
        let out = sb.run(&["-lb"], "");
        assert_eq!(out.status.code(), Some(0));
        assert!(!sb.config().exists());
    }
}
