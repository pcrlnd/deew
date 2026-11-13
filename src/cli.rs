//! Hand-written emulation of the `argparse` setup used by the Python version.
//!
//! argparse uses multi-letter single-dash flags (`-ti`, `-dm`, ...), which
//! the usual Rust parsers cannot express, so the (small) subset of argparse
//! behaviour that deew relies on is reimplemented here.

use crate::help::{HELP, wrap_help};
use crate::rprintln;
use crate::term::console_width;

#[derive(Debug, Clone)]
pub struct Args {
    pub input: Option<Vec<String>>, // None if -i/--input never given
    pub track_index: i64,           // default 0
    pub output: Option<String>,
    pub ddp_mode: Option<String>,
    pub ddp_atmos_mode: Option<String>,
    pub format: String, // default "ddp" (raw, not lowercased)
    pub bitrate: Option<i64>,
    pub downmix: Option<i64>,
    pub delay: Option<String>,
    pub drc: Option<String>, // None: the default of the format
    pub dialnorm: i64,       // default 0
    pub instances: Option<String>,
    pub keeptemp: bool,
    pub keep_log: bool,
    pub output_to_input: bool,
    pub disable_lfe: bool,
    pub no_phase_shift: bool,
    pub measure_only: bool,
    pub list_bitrates: bool,
    pub long_argument: bool,
    pub no_prompt: bool,
    pub print_logos: bool,
    pub changelog: bool,
    pub get_tools: bool,
    pub config: bool,
    pub generate_config: bool,
}

impl Default for Args {
    fn default() -> Self {
        Args {
            input: None,
            track_index: 0,
            output: None,
            ddp_mode: None,
            ddp_atmos_mode: None,
            format: "ddp".to_string(),
            bitrate: None,
            downmix: None,
            delay: None,
            drc: None,
            dialnorm: 0,
            instances: None,
            keeptemp: false,
            keep_log: false,
            output_to_input: false,
            disable_lfe: false,
            no_phase_shift: false,
            measure_only: false,
            list_bitrates: false,
            long_argument: false,
            no_prompt: false,
            print_logos: false,
            changelog: false,
            get_tools: false,
            config: false,
            generate_config: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Help,
    Version,
    /// `nargs='*'`
    Many,
    Str,
    Int,
    Flag,
}

/// (short, long, kind)
const OPTIONS: &[(&str, &str, Kind)] = &[
    ("-h", "--help", Kind::Help),
    ("-v", "--version", Kind::Version),
    ("-i", "--input", Kind::Many),
    ("-ti", "--track-index", Kind::Int),
    ("-o", "--output", Kind::Str),
    ("-oi", "--output-to-input", Kind::Flag),
    ("-f", "--format", Kind::Str),
    ("", "--ddp-mode", Kind::Str),
    ("", "--ddp-atmos-mode", Kind::Str),
    ("-b", "--bitrate", Kind::Int),
    ("-dm", "--downmix", Kind::Int),
    ("-dl", "--disable-lfe", Kind::Flag),
    ("-nps", "--no-phase-shift", Kind::Flag),
    ("-d", "--delay", Kind::Str),
    ("-r", "--drc", Kind::Str),
    ("-dn", "--dialnorm", Kind::Int),
    ("-in", "--instances", Kind::Str),
    ("-k", "--keeptemp", Kind::Flag),
    ("-kl", "--keep-log", Kind::Flag),
    ("-mo", "--measure-only", Kind::Flag),
    ("-lb", "--list-bitrates", Kind::Flag),
    ("-la", "--long-argument", Kind::Flag),
    ("-np", "--no-prompt", Kind::Flag),
    ("-pl", "--print-logos", Kind::Flag),
    ("-cl", "--changelog", Kind::Flag),
    ("-gt", "--get-tools", Kind::Flag),
    ("-c", "--config", Kind::Flag),
    ("-gc", "--generate-config", Kind::Flag),
];

type Opt = (&'static str, &'static str, Kind);

/// Why parsing stopped early.
#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    Help,
    Version,
    Error(String),
}

/// Classification of a command line token, like argparse's pattern string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tok {
    /// Looks like an option (`O`).
    Option,
    /// A value / positional (`A`).
    Arg,
    /// The first `--`; it and everything after it is never an option.
    Dashes,
}

fn lookup(token: &str) -> Option<&'static Opt> {
    OPTIONS
        .iter()
        .find(|(s, l, _)| (!s.is_empty() && *s == token) || *l == token)
}

/// Split `--long=value` / `-x=value` into the option and its explicit value.
fn lookup_with_explicit(token: &str) -> Option<(&'static Opt, Option<&str>)> {
    if let Some(opt) = lookup(token) {
        return Some((opt, None));
    }
    let (name, value) = token.split_once('=')?;
    lookup(name).map(|opt| (opt, Some(value)))
}

/// `^-\d+$|^-\d*\.\d+$`
fn negative_number_like(token: &str) -> bool {
    let Some(rest) = token.strip_prefix('-') else {
        return false;
    };
    let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
    match rest.split_once('.') {
        None => !rest.is_empty() && digits(rest),
        Some((int, frac)) => digits(int) && !frac.is_empty() && digits(frac),
    }
}

fn looks_like_option(token: &str) -> bool {
    if !token.starts_with('-') {
        return false;
    }
    if lookup_with_explicit(token).is_some() {
        return true;
    }
    if token.len() == 1 || negative_number_like(token) || token.contains(' ') {
        return false;
    }
    true
}

fn classify(argv: &[String]) -> Vec<Tok> {
    let mut seen_dashes = false;
    argv.iter()
        .map(|t| {
            if seen_dashes {
                Tok::Arg
            } else if t == "--" {
                seen_dashes = true;
                Tok::Dashes
            } else if looks_like_option(t) {
                Tok::Option
            } else {
                Tok::Arg
            }
        })
        .collect()
}

/// Python `int()` semantics for base 10 (ASCII digits only), saturating on
/// overflow instead of growing without bound.
fn py_int(s: &str) -> Option<i64> {
    let t = s.trim();
    let (neg, digits) = match t.strip_prefix('-') {
        Some(r) => (true, r),
        None => (false, t.strip_prefix('+').unwrap_or(t)),
    };
    if digits.is_empty()
        || digits.starts_with('_')
        || digits.ends_with('_')
        || digits.contains("__")
        || !digits.bytes().all(|b| b.is_ascii_digit() || b == b'_')
    {
        return None;
    }
    let mut value: i64 = 0;
    let mut overflow = false;
    for b in digits.bytes().filter(|b| *b != b'_') {
        match value
            .checked_mul(10)
            .and_then(|v| v.checked_add(i64::from(b - b'0')))
        {
            Some(v) => value = v,
            None => {
                overflow = true;
                break;
            }
        }
    }
    Some(match (overflow, neg) {
        (true, true) => i64::MIN,
        (true, false) => i64::MAX,
        (false, true) => -value,
        (false, false) => value,
    })
}

fn display_name(opt: &Opt) -> String {
    if opt.0.is_empty() {
        return opt.1.to_string();
    }
    format!("{}/{}", opt.0, opt.1)
}

fn set_flag(args: &mut Args, long: &str) {
    match long {
        "--keeptemp" => args.keeptemp = true,
        "--keep-log" => args.keep_log = true,
        "--output-to-input" => args.output_to_input = true,
        "--disable-lfe" => args.disable_lfe = true,
        "--no-phase-shift" => args.no_phase_shift = true,
        "--measure-only" => args.measure_only = true,
        "--list-bitrates" => args.list_bitrates = true,
        "--long-argument" => args.long_argument = true,
        "--no-prompt" => args.no_prompt = true,
        "--print-logos" => args.print_logos = true,
        "--changelog" => args.changelog = true,
        "--get-tools" => args.get_tools = true,
        "--config" => args.config = true,
        "--generate-config" => args.generate_config = true,
        _ => unreachable!("not a flag: {long}"),
    }
}

fn set_int(args: &mut Args, long: &str, v: i64) {
    match long {
        "--track-index" => args.track_index = v,
        "--bitrate" => args.bitrate = Some(v),
        "--downmix" => args.downmix = Some(v),
        "--dialnorm" => args.dialnorm = v,
        _ => unreachable!("not an int option: {long}"),
    }
}

fn set_str(args: &mut Args, long: &str, v: String) {
    match long {
        "--output" => args.output = Some(v),
        "--format" => args.format = v,
        "--ddp-mode" => args.ddp_mode = Some(v),
        "--ddp-atmos-mode" => args.ddp_atmos_mode = Some(v),
        "--delay" => args.delay = Some(v),
        "--drc" => args.drc = Some(v),
        "--instances" => args.instances = Some(v),
        _ => unreachable!("not a string option: {long}"),
    }
}

fn try_parse(argv: &[String]) -> Result<Args, Outcome> {
    let toks = classify(argv);
    let mut args = Args::default();
    let mut extras: Vec<&str> = Vec::new();
    let mut i = 0;

    while i < argv.len() {
        let token = argv[i].as_str();
        i += 1;
        if toks[i - 1] == Tok::Dashes {
            // `--` and everything after it is left over.
            extras.extend(argv[i - 1..].iter().map(String::as_str));
            break;
        }
        let found = if toks[i - 1] == Tok::Option {
            lookup_with_explicit(token)
        } else {
            None
        };
        let Some((opt, explicit)) = found else {
            extras.push(token);
            continue;
        };
        let name = display_name(opt);

        match opt.2 {
            Kind::Help | Kind::Version | Kind::Flag => {
                if let Some(value) = explicit {
                    return Err(Outcome::Error(format!(
                        "argument {name}: ignored explicit argument '{value}'"
                    )));
                }
                match opt.2 {
                    Kind::Help => return Err(Outcome::Help),
                    Kind::Version => return Err(Outcome::Version),
                    _ => set_flag(&mut args, opt.1),
                }
            }
            Kind::Many => {
                if let Some(value) = explicit {
                    args.input = Some(vec![value.to_string()]);
                    continue;
                }
                let start = i;
                while i < argv.len() && toks[i] == Tok::Arg {
                    i += 1;
                }
                args.input = Some(argv[start..i].to_vec());
            }
            Kind::Str | Kind::Int => {
                let value = match explicit {
                    Some(value) => value,
                    None => {
                        if i >= argv.len() || toks[i] != Tok::Arg {
                            return Err(Outcome::Error(format!(
                                "argument {name}: expected one argument"
                            )));
                        }
                        i += 1;
                        argv[i - 1].as_str()
                    }
                };
                if opt.2 == Kind::Int {
                    let Some(n) = py_int(value) else {
                        return Err(Outcome::Error(format!(
                            "argument {name}: invalid int value: '{value}'"
                        )));
                    };
                    set_int(&mut args, opt.1, n);
                } else {
                    set_str(&mut args, opt.1, value.to_string());
                }
            }
        }
    }

    if !extras.is_empty() {
        return Err(Outcome::Error(format!(
            "unrecognized arguments: {}",
            extras.join(" ")
        )));
    }
    Ok(args)
}

/// The usage block (header, version and the usage lines).
fn usage_markup() -> String {
    let usage = HELP
        .split_once("\n\n[yellow]FLAGS")
        .map_or(HELP, |(usage, _)| usage);
    wrap_help(usage, console_width())
}

/// Print the full help to stdout (the Python version prints it through rich).
pub fn print_help() {
    rprintln!(
        "[not bold white]{}[/not bold white]",
        wrap_help(HELP, console_width())
    );
}

/// Print the usage block followed by `deew: error: <message>` and exit with 2.
pub fn usage_error(message: &str) -> ! {
    rprintln!("[not bold white]{}[/not bold white]", usage_markup());
    rprintln!(
        "[not bold white]deew: error: {}[/not bold white]",
        message.trim()
    );
    std::process::exit(2);
}

/// Parse the command line (without the program name). Prints and exits on
/// help, version and errors.
pub fn parse(argv: &[String]) -> Args {
    match try_parse(argv) {
        Ok(args) => args,
        Err(Outcome::Help) => {
            print_help();
            std::process::exit(0);
        }
        Err(Outcome::Version) => {
            crate::info::print_version();
        }
        Err(Outcome::Error(message)) => usage_error(&message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn p(argv: &[&str]) -> Result<Args, Outcome> {
        let argv: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
        try_parse(&argv)
    }

    fn ok(argv: &[&str]) -> Args {
        p(argv).unwrap_or_else(|e| panic!("{argv:?}: {e:?}"))
    }

    fn err(argv: &[&str]) -> String {
        match p(argv) {
            Err(Outcome::Error(m)) => m,
            other => panic!("{argv:?}: expected error, got {other:?}"),
        }
    }

    #[test]
    fn defaults() {
        let a = ok(&[]);
        assert_eq!(a.input, None);
        assert_eq!(a.track_index, 0);
        assert_eq!(a.output, None);
        assert_eq!(a.format, "ddp");
        assert_eq!(a.bitrate, None);
        assert_eq!(a.downmix, None);
        assert_eq!(a.delay, None);
        assert_eq!(a.drc, None);
        assert_eq!(a.dialnorm, 0);
        assert_eq!(a.instances, None);
        assert!(
            !a.keeptemp && !a.keep_log && !a.measure_only && !a.output_to_input && !a.disable_lfe
        );
        assert!(!a.no_phase_shift);
        assert!(a.ddp_mode.is_none() && a.ddp_atmos_mode.is_none());
        assert!(!a.list_bitrates && !a.long_argument && !a.no_prompt && !a.print_logos);
        assert!(!a.changelog && !a.get_tools && !a.config && !a.generate_config);
    }

    #[test]
    fn no_phase_shift_both_spellings() {
        assert!(ok(&["-nps"]).no_phase_shift);
        assert!(ok(&["--no-phase-shift"]).no_phase_shift);
        assert!(ok(&["-f", "dd", "-dl", "-nps", "-np"]).no_phase_shift);
        // -np stays a different flag
        let a = ok(&["-np"]);
        assert!(a.no_prompt && !a.no_phase_shift);
        assert!(!ok(&["-nps"]).no_prompt);
    }

    #[test]
    fn force_standard_is_unknown() {
        for flag in ["-fs", "--force-standard"] {
            assert_eq!(err(&[flag]), format!("unrecognized arguments: {flag}"));
        }
    }

    #[test]
    fn every_option() {
        let a = ok(&[
            "-i", "a.mkv", "b.mkv", "-ti", "2", "-o", "out", "-f", "DD", "-b", "640", "-dm", "2",
            "-d", "+5ms", "-r", "speech", "-dn", "-27", "-in", "50%", "-k", "-kl", "-mo", "-oi",
            "-dl", "-lb", "-la", "-np", "-pl", "-cl", "-gt", "-c", "-gc",
        ]);
        assert_eq!(
            a.input,
            Some(vec!["a.mkv".to_string(), "b.mkv".to_string()])
        );
        assert_eq!(a.track_index, 2);
        assert_eq!(a.output.as_deref(), Some("out"));
        assert_eq!(a.format, "DD");
        assert_eq!(a.bitrate, Some(640));
        assert_eq!(a.downmix, Some(2));
        assert_eq!(a.delay.as_deref(), Some("+5ms"));
        assert_eq!(a.drc.as_deref(), Some("speech"));
        assert_eq!(a.dialnorm, -27);
        assert_eq!(a.instances.as_deref(), Some("50%"));
        assert!(a.keeptemp && a.keep_log && a.measure_only && a.output_to_input && a.disable_lfe);
        assert!(a.list_bitrates && a.long_argument && a.no_prompt && a.print_logos);
        assert!(a.changelog && a.get_tools && a.config && a.generate_config);
    }

    #[test]
    fn long_names() {
        let a = ok(&[
            "--input",
            "x",
            "--track-index",
            "1",
            "--output",
            "o",
            "--format",
            "thd",
            "--bitrate",
            "256",
            "--downmix",
            "6",
            "--delay=-5ms",
            "--drc",
            "none",
            "--dialnorm",
            "-5",
            "--instances",
            "3",
            "--keeptemp",
            "--keep-log",
            "--output-to-input",
            "--ddp-mode",
            "bluray",
            "--ddp-atmos-mode=standard",
            "--disable-lfe",
            "--measure-only",
            "--list-bitrates",
            "--long-argument",
            "--no-prompt",
            "--print-logos",
            "--changelog",
            "--get-tools",
            "--config",
            "--generate-config",
        ]);
        assert_eq!(a.input, Some(vec!["x".to_string()]));
        assert_eq!(a.track_index, 1);
        assert_eq!(a.bitrate, Some(256));
        assert_eq!(a.delay.as_deref(), Some("-5ms"));
        assert_eq!(a.drc.as_deref(), Some("none"));
        assert_eq!(a.dialnorm, -5);
        assert!(a.keeptemp && a.generate_config && a.config && a.get_tools);
        assert!(a.output_to_input);
        assert_eq!(a.ddp_mode.as_deref(), Some("bluray"));
        assert_eq!(a.ddp_atmos_mode.as_deref(), Some("standard"));
    }

    #[test]
    fn force_bluray_is_gone_and_modes_have_no_short_flags() {
        assert_eq!(err(&["-fb"]), "unrecognized arguments: -fb");
        assert_eq!(
            err(&["--force-bluray"]),
            "unrecognized arguments: --force-bluray"
        );
        assert_eq!(
            err(&["--ddp-mode"]),
            "argument --ddp-mode: expected one argument"
        );
        assert_eq!(err(&["-o", "a", "-oi", "x"]), "unrecognized arguments: x");
    }

    #[test]
    fn equals_forms() {
        let a = ok(&["-b=640", "--format=dd", "-d=-5ms", "--delay=-0ms", "-o=dir"]);
        assert_eq!(a.bitrate, Some(640));
        assert_eq!(a.format, "dd");
        assert_eq!(a.delay.as_deref(), Some("-0ms"));
        assert_eq!(a.output.as_deref(), Some("dir"));
        assert_eq!(ok(&["-d=-5ms"]).delay.as_deref(), Some("-5ms"));
        assert_eq!(ok(&["-d="]).delay.as_deref(), Some(""));
        assert_eq!(ok(&["--delay=a=b"]).delay.as_deref(), Some("a=b"));
        // an explicit value is the only one taken by `-i`
        assert_eq!(ok(&["-i=a"]).input, Some(vec!["a".to_string()]));
        assert_eq!(err(&["-i=a", "b"]), "unrecognized arguments: b");
    }

    #[test]
    fn equals_form_on_flag_is_an_error() {
        assert_eq!(
            err(&["-k=x"]),
            "argument -k/--keeptemp: ignored explicit argument 'x'"
        );
        assert_eq!(
            err(&["--help=x"]),
            "argument -h/--help: ignored explicit argument 'x'"
        );
    }

    #[test]
    fn negative_numbers() {
        assert_eq!(ok(&["-dn", "-5"]).dialnorm, -5);
        assert_eq!(ok(&["-dn", "-31"]).dialnorm, -31);
        assert_eq!(ok(&["-d", "-5"]).delay.as_deref(), Some("-5"));
        assert_eq!(ok(&["-d", "-.5"]).delay.as_deref(), Some("-.5"));
        assert_eq!(ok(&["-d", "-1.5"]).delay.as_deref(), Some("-1.5"));
        assert_eq!(ok(&["-ti", "-1"]).track_index, -1);
    }

    #[test]
    fn negative_value_that_is_not_a_number_errors() {
        assert_eq!(
            err(&["-d", "-5ms"]),
            "argument -d/--delay: expected one argument"
        );
        assert_eq!(
            err(&["-d", "-x"]),
            "argument -d/--delay: expected one argument"
        );
        assert_eq!(
            err(&["-d", "-k"]),
            "argument -d/--delay: expected one argument"
        );
        assert_eq!(err(&["-b"]), "argument -b/--bitrate: expected one argument");
        assert_eq!(
            err(&["-f", "--bitrate", "5"]),
            "argument -f/--format: expected one argument"
        );
        // contains a space: treated as a value
        assert_eq!(ok(&["-d", "-5 ms"]).delay.as_deref(), Some("-5 ms"));
        // a lone dash is a value
        assert_eq!(ok(&["-o", "-"]).output.as_deref(), Some("-"));
    }

    #[test]
    fn input_nargs_star() {
        let a = ok(&["-i", "a", "b", "-k"]);
        assert_eq!(a.input, Some(vec!["a".to_string(), "b".to_string()]));
        assert!(a.keeptemp);
        assert_eq!(
            err(&["-i", "a", "b", "-k", "extra"]),
            "unrecognized arguments: extra"
        );
        // negative numbers are values
        assert_eq!(
            ok(&["-i", "-5", "a", "-1.5"]).input,
            Some(vec!["-5".to_string(), "a".to_string(), "-1.5".to_string()])
        );
        // zero values is fine
        assert_eq!(ok(&["-i"]).input, Some(vec![]));
        assert_eq!(ok(&["-i", "-k"]).input, Some(vec![]));
        // values with spaces and a lone dash are values
        assert_eq!(
            ok(&["-i", "-a b", "-"]).input,
            Some(vec!["-a b".to_string(), "-".to_string()])
        );
        // unknown option-like tokens stop the list
        assert_eq!(
            err(&["-i", "a", "-5ms", "b"]),
            "unrecognized arguments: -5ms b"
        );
    }

    #[test]
    fn repeated_options_last_wins() {
        let a = ok(&[
            "-i", "a", "-i", "b", "c", "-b", "1", "-b", "2", "-f", "dd", "-f", "ac4",
        ]);
        assert_eq!(a.input, Some(vec!["b".to_string(), "c".to_string()]));
        assert_eq!(a.bitrate, Some(2));
        assert_eq!(a.format, "ac4");
        assert_eq!(ok(&["-i", "a", "-i"]).input, Some(vec![]));
    }

    #[test]
    fn unrecognized_are_collected() {
        assert_eq!(
            err(&["a", "-x", "b", "--bogus=1", "c"]),
            "unrecognized arguments: a -x b --bogus=1 c"
        );
        assert_eq!(err(&["foo"]), "unrecognized arguments: foo");
        // no abbreviations and no concatenation
        assert_eq!(err(&["--del", "5"]), "unrecognized arguments: --del 5");
        assert_eq!(err(&["-b640"]), "unrecognized arguments: -b640");
        assert_eq!(err(&["-kk"]), "unrecognized arguments: -kk");
        assert_eq!(err(&["-t", "1"]), "unrecognized arguments: -t 1");
    }

    #[test]
    fn double_dash() {
        assert_eq!(err(&["--", "a"]), "unrecognized arguments: -- a");
        assert_eq!(
            err(&["-i", "a", "--", "-b", "1"]),
            "unrecognized arguments: -- -b 1"
        );
        assert_eq!(
            err(&["-d", "--", "-x"]),
            "argument -d/--delay: expected one argument"
        );
        assert_eq!(
            err(&["-d", "--"]),
            "argument -d/--delay: expected one argument"
        );
    }

    #[test]
    fn int_parsing() {
        assert_eq!(ok(&["-b", " 5 "]).bitrate, Some(5));
        assert_eq!(ok(&["-b", "+5"]).bitrate, Some(5));
        assert_eq!(ok(&["-b", "007"]).bitrate, Some(7));
        assert_eq!(ok(&["-b", "1_0"]).bitrate, Some(10));
        assert_eq!(ok(&["-b=-5"]).bitrate, Some(-5));
        assert_eq!(
            ok(&["-b", "99999999999999999999999"]).bitrate,
            Some(i64::MAX)
        );
        assert_eq!(
            ok(&["-b", "-99999999999999999999999"]).bitrate,
            Some(i64::MIN)
        );
        for bad in [
            "abc", "", "5.0", "0x10", "1__0", "_1", "1_", "+-5", "-", "1 2",
        ] {
            assert_eq!(
                err(&["-b", bad]),
                format!("argument -b/--bitrate: invalid int value: '{bad}'"),
                "{bad:?}"
            );
        }
        assert_eq!(
            err(&["-ti", "1.5"]),
            "argument -ti/--track-index: invalid int value: '1.5'"
        );
        assert_eq!(
            err(&["-dm", "x"]),
            "argument -dm/--downmix: invalid int value: 'x'"
        );
        assert_eq!(
            err(&["-dn", "x"]),
            "argument -dn/--dialnorm: invalid int value: 'x'"
        );
    }

    #[test]
    fn help_and_version_act_immediately() {
        assert_eq!(p(&["-h"]).unwrap_err(), Outcome::Help);
        assert_eq!(p(&["--help", "-b", "abc"]).unwrap_err(), Outcome::Help);
        assert_eq!(p(&["-v"]).unwrap_err(), Outcome::Version);
        assert_eq!(p(&["-k", "-v", "-x"]).unwrap_err(), Outcome::Version);
        // an earlier error wins
        assert!(matches!(p(&["-b", "abc", "-h"]), Err(Outcome::Error(_))));
        // unrecognized arguments are only reported at the end
        assert_eq!(p(&["-x", "-h"]).unwrap_err(), Outcome::Help);
    }

    #[test]
    fn negative_number_matcher() {
        for s in ["-1", "-10", "-.5", "-1.5", "-0.0"] {
            assert!(negative_number_like(s), "{s}");
        }
        for s in ["-", "-.", "-1.", "-a", "-5ms", "--5", "5", "-1.2.3", "-1e3"] {
            assert!(!negative_number_like(s), "{s}");
        }
    }

    #[test]
    fn usage_block() {
        let u = usage_markup();
        assert!(u.ends_with("[[green]-gt[/green]]"));
        assert!(!u.contains("FLAGS"));
        assert!(u.contains(env!("CARGO_PKG_VERSION")));
    }
}
