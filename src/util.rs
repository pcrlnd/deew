//! Pure helpers ported from the Python implementation. Behaviour (including
//! quirks) deliberately mirrors Python.

use regex::Regex;
use std::sync::LazyLock;

/// Python's `min(max(v, lo), hi)`; unlike `f64::clamp` it never panics.
pub fn clamp_f(v: f64, lo: f64, hi: f64) -> f64 {
    v.max(lo).min(hi)
}

pub fn clamp_i(v: i64, lo: i64, hi: i64) -> i64 {
    v.max(lo).min(hi)
}

/// Value of `allowed` closest to `value`; the first one wins on ties.
pub fn find_closest_allowed(value: i64, allowed: &[u32]) -> u32 {
    *allowed
        .iter()
        .min_by_key(|&&a| (i64::from(a) - value).abs())
        .expect("allowed bitrate list is empty")
}

/// Byte offset where the file name starts (`os.path.split(path)[1]`). With
/// `windows` (`ntpath`) both `/` and `\` separate, and a leading drive
/// (`C:`) is not part of the name.
fn name_start(path: &str, windows: bool) -> usize {
    let is_sep = |c: char| c == '/' || (windows && c == '\\');
    let drive = if windows && path.as_bytes().get(1) == Some(&b':') {
        2
    } else {
        0
    };
    path[drive..].rfind(is_sep).map_or(drive, |i| drive + i + 1)
}

/// `os.path.basename` of this platform.
pub fn file_name(path: &str) -> &str {
    &path[name_start(path, cfg!(windows))..]
}

/// `os.path.splitext(...)[0]` followed by `os.path.basename`.
fn stem_basename(path: &str, windows: bool) -> &str {
    let sep = name_start(path, windows);
    let stem_end = match path.rfind('.') {
        // Leading dots of the file name do not start an extension.
        Some(dot) if dot >= sep && path[sep..dot].chars().any(|c| c != '.') => dot,
        _ => path.len(),
    };
    &path[sep..stem_end]
}

static DELAY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r" ?DELAY [-|+]?[0-9]+m?s").expect("valid regex"));

pub fn basename(path: &str, ext: &str, quote: bool, sanitize: bool, stripdelay: bool) -> String {
    basename_for(path, ext, quote, sanitize, stripdelay, cfg!(windows))
}

fn basename_for(
    path: &str,
    ext: &str,
    quote: bool,
    sanitize: bool,
    stripdelay: bool,
    windows: bool,
) -> String {
    let mut name = format!("{}.{ext}", stem_basename(path, windows));
    if stripdelay {
        name = DELAY_RE.replace_all(&name, "").into_owned();
    }
    if sanitize {
        name = name.replace(' ', "_");
    }
    if quote {
        name = format!("\"{name}\"");
    }
    name
}

fn version_parts(v: &str) -> Vec<u64> {
    v.split('.')
        .map(|c| {
            let digits: String = c.chars().take_while(char::is_ascii_digit).collect();
            digits.parse().unwrap_or(0)
        })
        .collect()
}

fn version_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let (mut a, mut b) = (version_parts(a), version_parts(b));
    let n = a.len().max(b.len());
    a.resize(n, 0);
    b.resize(n, 0);
    a.cmp(&b)
}

pub fn version_lt(a: &str, b: &str) -> bool {
    version_cmp(a, b).is_lt()
}

/// Python's `str(timedelta(seconds=x))`, including its float handling:
/// the fractional microseconds are rounded half-to-even on the total.
pub fn python_timedelta_str(seconds: f64) -> String {
    let whole = seconds.trunc();
    let micro = (seconds - whole) * 1e6;
    let micro_whole = micro.trunc();
    let leftover = micro - micro_whole;
    let mut us = whole as i128 * 1_000_000 + micro_whole as i128;
    let mut rounded = leftover.round();
    if (rounded - leftover).abs() == 0.5 {
        let odd = (us & 1) as f64;
        rounded = 2.0 * ((leftover + odd) / 2.0).round() - odd;
    }
    us += rounded as i128;

    let (secs, micros) = (us / 1_000_000, us % 1_000_000);
    let (days, secs) = (secs / 86_400, secs % 86_400);
    let mut s = String::new();
    if days != 0 {
        let plural = if days == 1 { "" } else { "s" };
        s.push_str(&format!("{days} day{plural}, "));
    }
    s.push_str(&format!(
        "{}:{:02}:{:02}",
        secs / 3600,
        secs / 60 % 60,
        secs % 60
    ));
    if micros != 0 {
        s.push_str(&format!(".{micros:06}"));
    }
    s
}

/// Python's `f'{v:g}'`.
pub fn format_g(v: f64) -> String {
    if !v.is_finite() {
        return py_non_finite(v);
    }
    // Format with 6 significant digits first to learn the (rounded) exponent.
    let sci = format!("{v:.5e}");
    let (mantissa, exp) = sci.split_once('e').expect("exponent present");
    let exp: i32 = exp.parse().expect("valid exponent");
    let strip = |s: &str| {
        if s.contains('.') {
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        } else {
            s.to_string()
        }
    };
    if (-4..6).contains(&exp) {
        strip(&format!("{v:.*}", (5 - exp) as usize))
    } else {
        let sign = if exp < 0 { '-' } else { '+' };
        format!("{}e{sign}{:02}", strip(mantissa), exp.abs())
    }
}

fn py_non_finite(v: f64) -> String {
    match (v.is_nan(), v.is_sign_negative()) {
        (true, _) => "nan".into(),
        (_, true) => "-inf".into(),
        _ => "inf".into(),
    }
}

/// Python's `format(v, f'.{prec}f')`.
fn format_fixed(v: f64, prec: usize) -> String {
    if v.is_finite() {
        format!("{v:.prec$}")
    } else {
        py_non_finite(v)
    }
}

/// Python's list repr for integers: `[48000, 44100]`.
pub fn py_int_list(v: &[i64]) -> String {
    let items: Vec<String> = v.iter().map(i64::to_string).collect();
    format!("[{}]", items.join(", "))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DelayMode {
    PrependSilenceDuration,
    Start,
}

/// Python's `re.sub(r'[^0-9\.]', '', s)`.
fn digits_and_dots(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_digit() || *c == '.')
        .collect()
}

/// Python's `float(s)` as far as it matters here (surrounding whitespace is
/// allowed; underscore digit separators are not supported).
fn py_float(s: &str) -> Result<f64, ()> {
    s.trim().parse().map_err(|_| ())
}

/// Port of `convert_delay_to_ms`: returns `(delay_print, delay_xml, mode)`.
/// Every input Python rejects (`print_exit('delay')`) or crashes on
/// (`ValueError`, `ZeroDivisionError`, ...) gives `Err`.
pub fn convert_delay_to_ms(inp: &str, compensate: u32) -> Result<(String, String, DelayMode), ()> {
    if !inp.starts_with(['-', '+']) {
        return Err(());
    }
    let inp = inp.replace(',', ".");
    let negative = inp.starts_with('-');

    let mut delay = if let Some((frames, fps)) = inp.split_once('@') {
        // `fps` is everything up to a possible second '@' (Python's split()[1]).
        let fps = fps.split('@').next().unwrap_or_default();
        let frames = py_float(&digits_and_dots(frames))?;
        if !frames.is_finite() {
            return Err(());
        }
        let frame = frames.round_ties_even();
        if frame == 0.0 {
            return Err(());
        }
        let fps = match fps {
            "ntsc" => 24000.0 / 1001.0,
            "pal" => 25.0,
            _ => match fps.split_once('/') {
                Some((dividend, divisor)) => {
                    let divisor = divisor.split('/').next().unwrap_or_default();
                    let (dividend, divisor) = (digits_and_dots(dividend), digits_and_dots(divisor));
                    if dividend.is_empty() || divisor.is_empty() {
                        return Err(());
                    }
                    let divisor = py_float(&divisor)?;
                    if divisor == 0.0 {
                        return Err(());
                    }
                    py_float(&dividend)? / divisor
                }
                None => py_float(fps)?,
            },
        };
        if fps == 0.0 {
            return Err(());
        }
        frame / fps * 1000.0
    } else {
        if !inp.ends_with('s') || inp.matches('.').count() >= 2 {
            return Err(());
        }
        let value = py_float(&digits_and_dots(&inp))?;
        if inp.ends_with("ms") {
            value
        } else {
            value * 1000.0
        }
    };

    // Python builds `float(f'-{delay}')`, which is a ValueError for a delay
    // that is already negative (e.g. a negative fps).
    if delay.is_sign_negative() && !delay.is_nan() {
        return Err(());
    }
    if negative {
        delay = -delay;
    }
    let delay_print = format!("{} ms", format_fixed(delay, 3));

    if compensate != 0 {
        delay -= f64::from(compensate) / 48.0; // samples at 48 kHz to ms
    }

    if delay < 0.0 {
        let secs = delay.abs() / 1000.0;
        // timedelta raises OverflowError beyond 999999999 days.
        if !secs.is_finite() || secs >= 999_999_999.0 * 86_400.0 {
            return Err(());
        }
        let mut xml = python_timedelta_str(secs);
        if !xml.contains('.') {
            xml.push_str(".0");
        }
        Ok((delay_print, xml, DelayMode::Start))
    } else {
        let xml = format_fixed(delay / 1000.0, 6);
        Ok((delay_print, xml, DelayMode::PrependSilenceDuration))
    }
}

/// Seconds of a Python `timedelta` string like `1 day, 3:46:40.005333`.
fn parse_timedelta(text: &str) -> Option<f64> {
    let (days, clock) = match text.split_once(", ") {
        Some((days, clock)) => (days.split(' ').next()?.parse::<f64>().ok()?, clock),
        None => (0.0, text),
    };
    let mut parts = clock.split(':');
    let hours: f64 = parts.next()?.parse().ok()?;
    let minutes: f64 = parts.next()?.parse().ok()?;
    let seconds: f64 = parts.next()?.parse().ok()?;
    Some(days * 86400.0 + hours * 3600.0 + minutes * 60.0 + seconds)
}

/// The delay as DEE takes it: how to apply it (`Start` trims the beginning,
/// `PrependSilenceDuration` adds silence) and for how many seconds (negative
/// zero for `-0ms` without compensation).
pub fn delay_seconds(inp: &str, compensate: u32) -> Result<(DelayMode, f64), ()> {
    let (_, text, mode) = convert_delay_to_ms(inp, compensate)?;
    let seconds = match mode {
        DelayMode::Start => parse_timedelta(&text).ok_or(())?,
        DelayMode::PrependSilenceDuration => text.parse::<f64>().map_err(|_| ())?,
    };
    if seconds.is_finite() {
        Ok((mode, seconds))
    } else {
        Err(())
    }
}

/// Decimal seconds with at most 6 decimals and no trailing zeros.
fn seconds_text(seconds: f64) -> String {
    let text = format_fixed(seconds, 6);
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// `--start S` or `--add-silence P:0` for a delay; empty if the (compensated)
/// delay is zero.
pub fn delay_options(inp: &str, compensate: u32) -> Result<Vec<String>, ()> {
    let (mode, seconds) = delay_seconds(inp, compensate)?;
    let text = seconds_text(seconds);
    if text == "0" || text == "-0" {
        return Ok(Vec::new());
    }
    Ok(match mode {
        DelayMode::Start => vec!["--start".to_string(), text],
        DelayMode::PrependSilenceDuration => vec!["--add-silence".to_string(), format!("{text}:0")],
    })
}

/// An argument as the shell of the platform needs it, for the command lines
/// that are printed (`-la`, deew-log.html).
pub fn shell_quote(arg: &str) -> String {
    shell_quote_for(arg, cfg!(windows))
}

/// `shell_quote` for a given style: `windows` is cmd (double quotes), else
/// a POSIX shell (single quotes).
fn shell_quote_for(arg: &str, windows: bool) -> String {
    if windows {
        // Paths are shown as they are: `\` and `:` are plain characters.
        let special = arg.is_empty()
            || arg
                .chars()
                .any(|c| c.is_whitespace() || "\"'`$&|;<>()*?!#~^%".contains(c));
        if special {
            format!("\"{}\"", arg.replace('"', "\\\""))
        } else {
            arg.to_string()
        }
    } else {
        let special = arg.is_empty()
            || arg
                .chars()
                .any(|c| c.is_whitespace() || "\"'`$&|;<>()*?!#~^%[]{}\\".contains(c));
        if special {
            format!("'{}'", arg.replace('\'', "'\\''"))
        } else {
            arg.to_string()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn static_regexes_compile() {
        LazyLock::force(&DELAY_RE);
    }

    #[test]
    fn shell_quote_unix() {
        let q = |a: &str| shell_quote_for(a, false);
        assert_eq!(q("/tmp/a.wav"), "/tmp/a.wav");
        assert_eq!(q("X:\\a:b"), "'X:\\a:b'");
        assert_eq!(q("0:a:0"), "0:a:0");
        assert_eq!(q("ő 日本 –"), "'ő 日本 –'");
        assert_eq!(q("ő日本–"), "ő日本–");
        assert_eq!(q(""), "''");
        assert_eq!(q("a b"), "'a b'");
        assert_eq!(q("it's"), "'it'\\''s'");
        assert_eq!(q("$(x)`y`"), "'$(x)`y`'");
        assert_eq!(q("[a:0]pan"), "'[a:0]pan'");
        assert_eq!(q("a & b; $(x) 'q'.wav"), "'a & b; $(x) '\\''q'\\''.wav'");
    }

    #[test]
    fn shell_quote_windows() {
        let q = |a: &str| shell_quote_for(a, true);
        assert_eq!(q("X:\\My Music\\a.thd"), "\"X:\\My Music\\a.thd\"");
        assert_eq!(q("X:\\deew\\t.exe"), "X:\\deew\\t.exe");
        assert_eq!(q(""), "\"\"");
        assert_eq!(q("x\"y"), "\"x\\\"y\"");
        assert_eq!(q("a&b"), "\"a&b\"");
        assert_eq!(q("[a:0]pan"), "[a:0]pan");
    }

    // The tables below were generated by running the original Python
    // functions (see `scratchpad/gen_util_tests.py` in the task report).

    type DelayCase = (
        &'static str,
        u32,
        Option<(&'static str, &'static str, DelayMode)>,
    );

    #[rustfmt::skip]
    const DELAY_CASES: &[DelayCase] = &[
        ("+0ms", 256, Some(("0.000 ms", "0:00:00.005333", DelayMode::Start))),
        ("+0ms", 0, Some(("0.000 ms", "0.000000", DelayMode::PrependSilenceDuration))),
        ("-0ms", 256, Some(("-0.000 ms", "0:00:00.005333", DelayMode::Start))),
        ("-0ms", 0, Some(("-0.000 ms", "-0.000000", DelayMode::PrependSilenceDuration))),
        ("-5.1ms", 256, Some(("-5.100 ms", "0:00:00.010433", DelayMode::Start))),
        ("-5.1ms", 0, Some(("-5.100 ms", "0:00:00.005100", DelayMode::Start))),
        ("+5.1ms", 256, Some(("5.100 ms", "0:00:00.000233", DelayMode::Start))),
        ("+5.1ms", 0, Some(("5.100 ms", "0.005100", DelayMode::PrependSilenceDuration))),
        ("+1,52s", 256, Some(("1520.000 ms", "1.514667", DelayMode::PrependSilenceDuration))),
        ("+1,52s", 0, Some(("1520.000 ms", "1.520000", DelayMode::PrependSilenceDuration))),
        ("-1,52s", 256, Some(("-1520.000 ms", "0:00:01.525333", DelayMode::Start))),
        ("-1,52s", 0, Some(("-1520.000 ms", "0:00:01.520000", DelayMode::Start))),
        ("-24@pal", 256, Some(("-960.000 ms", "0:00:00.965333", DelayMode::Start))),
        ("-24@pal", 0, Some(("-960.000 ms", "0:00:00.960000", DelayMode::Start))),
        ("+10@24000/1001", 256, Some(("417.083 ms", "0.411750", DelayMode::PrependSilenceDuration))),
        ("+10@24000/1001", 0, Some(("417.083 ms", "0.417083", DelayMode::PrependSilenceDuration))),
        ("+3@ntsc", 256, Some(("125.125 ms", "0.119792", DelayMode::PrependSilenceDuration))),
        ("+3@ntsc", 0, Some(("125.125 ms", "0.125125", DelayMode::PrependSilenceDuration))),
        ("+100ms", 256, Some(("100.000 ms", "0.094667", DelayMode::PrependSilenceDuration))),
        ("+100ms", 0, Some(("100.000 ms", "0.100000", DelayMode::PrependSilenceDuration))),
        ("-100ms", 256, Some(("-100.000 ms", "0:00:00.105333", DelayMode::Start))),
        ("-100ms", 0, Some(("-100.000 ms", "0:00:00.100000", DelayMode::Start))),
        ("+2.5@25", 256, Some(("80.000 ms", "0.074667", DelayMode::PrependSilenceDuration))),
        ("+2.5@25", 0, Some(("80.000 ms", "0.080000", DelayMode::PrependSilenceDuration))),
        ("-3700s", 256, Some(("-3700000.000 ms", "1:01:40.005333", DelayMode::Start))),
        ("-3700s", 0, Some(("-3700000.000 ms", "1:01:40.0", DelayMode::Start))),
        ("+3.5@25", 256, Some(("160.000 ms", "0.154667", DelayMode::PrependSilenceDuration))),
        ("+3.5@25", 0, Some(("160.000 ms", "0.160000", DelayMode::PrependSilenceDuration))),
        ("+1000ms", 256, Some(("1000.000 ms", "0.994667", DelayMode::PrependSilenceDuration))),
        ("+1000ms", 0, Some(("1000.000 ms", "1.000000", DelayMode::PrependSilenceDuration))),
        ("-5ms", 256, Some(("-5.000 ms", "0:00:00.010333", DelayMode::Start))),
        ("-5ms", 0, Some(("-5.000 ms", "0:00:00.005000", DelayMode::Start))),
        ("+5.3333ms", 256, Some(("5.333 ms", "0:00:00.0", DelayMode::Start))),
        ("+5.3333ms", 0, Some(("5.333 ms", "0.005333", DelayMode::PrependSilenceDuration))),
        ("+5.34ms", 256, Some(("5.340 ms", "0.000007", DelayMode::PrependSilenceDuration))),
        ("+5.34ms", 0, Some(("5.340 ms", "0.005340", DelayMode::PrependSilenceDuration))),
        ("+0s", 256, Some(("0.000 ms", "0:00:00.005333", DelayMode::Start))),
        ("+0s", 0, Some(("0.000 ms", "0.000000", DelayMode::PrependSilenceDuration))),
        ("-0s", 256, Some(("-0.000 ms", "0:00:00.005333", DelayMode::Start))),
        ("-0s", 0, Some(("-0.000 ms", "-0.000000", DelayMode::PrependSilenceDuration))),
        ("-0.0s", 256, Some(("-0.000 ms", "0:00:00.005333", DelayMode::Start))),
        ("-0.0s", 0, Some(("-0.000 ms", "-0.000000", DelayMode::PrependSilenceDuration))),
        ("-86400s", 256, Some(("-86400000.000 ms", "1 day, 0:00:00.005333", DelayMode::Start))),
        ("-86400s", 0, Some(("-86400000.000 ms", "1 day, 0:00:00.0", DelayMode::Start))),
        ("-100000s", 256, Some(("-100000000.000 ms", "1 day, 3:46:40.005333", DelayMode::Start))),
        ("-100000s", 0, Some(("-100000000.000 ms", "1 day, 3:46:40.0", DelayMode::Start))),
        ("+1.5@23.976", 256, Some(("83.417 ms", "0.078083", DelayMode::PrependSilenceDuration))),
        ("+1.5@23.976", 0, Some(("83.417 ms", "0.083417", DelayMode::PrependSilenceDuration))),
        ("+7@30000/1001", 256, Some(("233.567 ms", "0.228233", DelayMode::PrependSilenceDuration))),
        ("+7@30000/1001", 0, Some(("233.567 ms", "0.233567", DelayMode::PrependSilenceDuration))),
        ("+24@24 ", 256, Some(("1000.000 ms", "0.994667", DelayMode::PrependSilenceDuration))),
        ("+24@24 ", 0, Some(("1000.000 ms", "1.000000", DelayMode::PrependSilenceDuration))),
        ("+10@25@3", 256, Some(("400.000 ms", "0.394667", DelayMode::PrependSilenceDuration))),
        ("+10@25@3", 0, Some(("400.000 ms", "0.400000", DelayMode::PrependSilenceDuration))),
        ("+1.4@25", 256, Some(("40.000 ms", "0.034667", DelayMode::PrependSilenceDuration))),
        ("+1.4@25", 0, Some(("40.000 ms", "0.040000", DelayMode::PrependSilenceDuration))),
        ("+0.4@25", 256, None),
        ("+0.4@25", 0, None),
        ("+1e3ms", 256, Some(("13.000 ms", "0.007667", DelayMode::PrependSilenceDuration))),
        ("+1e3ms", 0, Some(("13.000 ms", "0.013000", DelayMode::PrependSilenceDuration))),
        ("+12abcms", 256, Some(("12.000 ms", "0.006667", DelayMode::PrependSilenceDuration))),
        ("+12abcms", 0, Some(("12.000 ms", "0.012000", DelayMode::PrependSilenceDuration))),
        ("-1,5ms", 256, Some(("-1.500 ms", "0:00:00.006833", DelayMode::Start))),
        ("-1,5ms", 0, Some(("-1.500 ms", "0:00:00.001500", DelayMode::Start))),
        ("+9999999999999ms", 256, Some(("9999999999999.000 ms", "9999999999.993666", DelayMode::PrependSilenceDuration))),
        ("+9999999999999ms", 0, Some(("9999999999999.000 ms", "9999999999.999001", DelayMode::PrependSilenceDuration))),
        ("+5@inf", 256, Some(("0.000 ms", "0:00:00.005333", DelayMode::Start))),
        ("+5@inf", 0, Some(("0.000 ms", "0.000000", DelayMode::PrependSilenceDuration))),
        ("+5@nan", 256, Some(("nan ms", "nan", DelayMode::PrependSilenceDuration))),
        ("+5@nan", 0, Some(("nan ms", "nan", DelayMode::PrependSilenceDuration))),
        ("-5@inf", 256, Some(("-0.000 ms", "0:00:00.005333", DelayMode::Start))),
        ("-5@inf", 0, Some(("-0.000 ms", "-0.000000", DelayMode::PrependSilenceDuration))),
        ("5ms", 256, None),
        ("5ms", 0, None),
        ("+0@pal", 256, None),
        ("+0@pal", 0, None),
        ("+1.2.3s", 256, None),
        ("+1.2.3s", 0, None),
        ("+5", 256, None),
        ("+5", 0, None),
        ("+5@abc", 256, None),
        ("+5@abc", 0, None),
        ("+5@1/0", 256, None),
        ("+5@1/0", 0, None),
        ("+@pal", 256, None),
        ("+@pal", 0, None),
        ("", 256, None),
        ("", 0, None),
        ("-", 256, None),
        ("-", 0, None),
        ("+ms", 256, None),
        ("+ms", 0, None),
        ("+.ms", 256, None),
        ("+.ms", 0, None),
        ("+5.ms", 256, Some(("5.000 ms", "0:00:00.000333", DelayMode::Start))),
        ("+5.ms", 0, Some(("5.000 ms", "0.005000", DelayMode::PrependSilenceDuration))),
        ("+5@-25", 256, None),
        ("+5@-25", 0, None),
        ("+5@0", 256, None),
        ("+5@0", 0, None),
        ("+5@/25", 256, None),
        ("+5@/25", 0, None),
        ("+5@25/", 256, None),
        ("+5@25/", 0, None),
        ("+5@1.2.3/4", 256, None),
        ("+5@1.2.3/4", 0, None),
        ("+1.2.3@25", 256, None),
        ("+1.2.3@25", 0, None),
        ("+5@25/0.0", 256, None),
        ("+5@25/0.0", 0, None),
        ("-1@0.5", 256, Some(("-2000.000 ms", "0:00:02.005333", DelayMode::Start))),
        ("-1@0.5", 0, Some(("-2000.000 ms", "0:00:02.0", DelayMode::Start))),
        ("+5@1/a", 256, None),
        ("+5@1/a", 0, None),
        ("+1e999s", 256, Some(("1999000.000 ms", "1998.994667", DelayMode::PrependSilenceDuration))),
        ("+1e999s", 0, Some(("1999000.000 ms", "1999.000000", DelayMode::PrependSilenceDuration))),
        ("-1e-9ms", 256, Some(("-19.000 ms", "0:00:00.024333", DelayMode::Start))),
        ("-1e-9ms", 0, Some(("-19.000 ms", "0:00:00.019000", DelayMode::Start))),
        ("-0.5ms", 256, Some(("-0.500 ms", "0:00:00.005833", DelayMode::Start))),
        ("-0.5ms", 0, Some(("-0.500 ms", "0:00:00.000500", DelayMode::Start))),
        ("+0.5ms", 256, Some(("0.500 ms", "0:00:00.004833", DelayMode::Start))),
        ("+0.5ms", 0, Some(("0.500 ms", "0.000500", DelayMode::PrependSilenceDuration))),
        ("+0.0000001ms", 256, Some(("0.000 ms", "0:00:00.005333", DelayMode::Start))),
        ("+0.0000001ms", 0, Some(("0.000 ms", "0.000000", DelayMode::PrependSilenceDuration))),
    ];

    #[rustfmt::skip]
    const TIMEDELTA_CASES: &[(f64, &str)] = &[
        (0.0, "0:00:00"),
        (0.0053333333333333, "0:00:00.005333"),
        (0.005333333333333333, "0:00:00.005333"),
        (1.0, "0:00:01"),
        (3700.5, "1:01:40.500000"),
        (5e-07, "0:00:00"),
        (1.5e-06, "0:00:00.000002"),
        (2.5e-06, "0:00:00.000002"),
        (59.9999995, "0:01:00"),
        (86399.9999994, "23:59:59.999999"),
        (0.1, "0:00:00.100000"),
        (2.675, "0:00:02.675000"),
        (100000.25, "1 day, 3:46:40.250000"),
        (3600.0, "1:00:00"),
        (1.234567891, "0:00:01.234568"),
        (1e-06, "0:00:00.000001"),
        (4.999e-07, "0:00:00"),
        (12345.6789, "3:25:45.678900"),
    ];

    #[rustfmt::skip]
    const BASENAME_CASES: &[(&str, &str, bool, bool, bool, &str)] = &[
        ("/a/b/My Movie DELAY -24ms.dts", "ec3", false, false, false, "My Movie DELAY -24ms.ec3"),
        ("/a/b/My Movie DELAY -24ms.dts", "ec3", false, false, true, "My Movie.ec3"),
        ("/a/b/My Movie DELAY -24ms.dts", "ec3", false, true, false, "My_Movie_DELAY_-24ms.ec3"),
        ("/a/b/My Movie DELAY -24ms.dts", "ec3", true, true, true, "\"My_Movie.ec3\""),
        ("/a/b/My Movie DELAY -24ms.dts", "ec3", true, false, false, "\"My Movie DELAY -24ms.ec3\""),
        ("/a/b/My Movie DELAY +1500ms.dts", "wav", false, false, false, "My Movie DELAY +1500ms.wav"),
        ("/a/b/My Movie DELAY +1500ms.dts", "wav", false, false, true, "My Movie.wav"),
        ("/a/b/My Movie DELAY +1500ms.dts", "wav", false, true, false, "My_Movie_DELAY_+1500ms.wav"),
        ("/a/b/My Movie DELAY +1500ms.dts", "wav", true, true, true, "\"My_Movie.wav\""),
        ("/a/b/My Movie DELAY +1500ms.dts", "wav", true, false, false, "\"My Movie DELAY +1500ms.wav\""),
        ("Éléphant ünïcode.wav", "wav", false, false, false, "Éléphant ünïcode.wav"),
        ("Éléphant ünïcode.wav", "wav", false, false, true, "Éléphant ünïcode.wav"),
        ("Éléphant ünïcode.wav", "wav", false, true, false, "Éléphant_ünïcode.wav"),
        ("Éléphant ünïcode.wav", "wav", true, true, true, "\"Éléphant_ünïcode.wav\""),
        ("Éléphant ünïcode.wav", "wav", true, false, false, "\"Éléphant ünïcode.wav\""),
        ("noext", "wav", false, false, false, "noext.wav"),
        ("noext", "wav", false, false, true, "noext.wav"),
        ("noext", "wav", false, true, false, "noext.wav"),
        ("noext", "wav", true, true, true, "\"noext.wav\""),
        ("noext", "wav", true, false, false, "\"noext.wav\""),
        ("a.b.c", "wav", false, false, false, "a.b.wav"),
        ("a.b.c", "wav", false, false, true, "a.b.wav"),
        ("a.b.c", "wav", false, true, false, "a.b.wav"),
        ("a.b.c", "wav", true, true, true, "\"a.b.wav\""),
        ("a.b.c", "wav", true, false, false, "\"a.b.wav\""),
        (".bashrc", "wav", false, false, false, ".bashrc.wav"),
        (".bashrc", "wav", false, false, true, ".bashrc.wav"),
        (".bashrc", "wav", false, true, false, ".bashrc.wav"),
        (".bashrc", "wav", true, true, true, "\".bashrc.wav\""),
        (".bashrc", "wav", true, false, false, "\".bashrc.wav\""),
        ("/x/.hidden.mkv", "wav", false, false, false, ".hidden.wav"),
        ("/x/.hidden.mkv", "wav", false, false, true, ".hidden.wav"),
        ("/x/.hidden.mkv", "wav", false, true, false, ".hidden.wav"),
        ("/x/.hidden.mkv", "wav", true, true, true, "\".hidden.wav\""),
        ("/x/.hidden.mkv", "wav", true, false, false, "\".hidden.wav\""),
        ("..a", "wav", false, false, false, "..a.wav"),
        ("..a", "wav", false, false, true, "..a.wav"),
        ("..a", "wav", false, true, false, "..a.wav"),
        ("..a", "wav", true, true, true, "\"..a.wav\""),
        ("..a", "wav", true, false, false, "\"..a.wav\""),
        ("/x/dir.d/file", "wav", false, false, false, "file.wav"),
        ("/x/dir.d/file", "wav", false, false, true, "file.wav"),
        ("/x/dir.d/file", "wav", false, true, false, "file.wav"),
        ("/x/dir.d/file", "wav", true, true, true, "\"file.wav\""),
        ("/x/dir.d/file", "wav", true, false, false, "\"file.wav\""),
        ("/x/dir.d/", "wav", false, false, false, ".wav"),
        ("/x/dir.d/", "wav", false, false, true, ".wav"),
        ("/x/dir.d/", "wav", false, true, false, ".wav"),
        ("/x/dir.d/", "wav", true, true, true, "\".wav\""),
        ("/x/dir.d/", "wav", true, false, false, "\".wav\""),
        ("x/", "wav", false, false, false, ".wav"),
        ("x/", "wav", false, false, true, ".wav"),
        ("x/", "wav", false, true, false, ".wav"),
        ("x/", "wav", true, true, true, "\".wav\""),
        ("x/", "wav", true, false, false, "\".wav\""),
        ("/a/b/file DELAY 10s.thd", "wav", false, false, false, "file DELAY 10s.wav"),
        ("/a/b/file DELAY 10s.thd", "wav", false, false, true, "file.wav"),
        ("/a/b/file DELAY 10s.thd", "wav", false, true, false, "file_DELAY_10s.wav"),
        ("/a/b/file DELAY 10s.thd", "wav", true, true, true, "\"file.wav\""),
        ("/a/b/file DELAY 10s.thd", "wav", true, false, false, "\"file DELAY 10s.wav\""),
        ("file DELAY -5ms DELAY +6ms.mkv", "wav", false, false, false, "file DELAY -5ms DELAY +6ms.wav"),
        ("file DELAY -5ms DELAY +6ms.mkv", "wav", false, false, true, "file.wav"),
        ("file DELAY -5ms DELAY +6ms.mkv", "wav", false, true, false, "file_DELAY_-5ms_DELAY_+6ms.wav"),
        ("file DELAY -5ms DELAY +6ms.mkv", "wav", true, true, true, "\"file.wav\""),
        ("file DELAY -5ms DELAY +6ms.mkv", "wav", true, false, false, "\"file DELAY -5ms DELAY +6ms.wav\""),
        ("/a/[Group] Name (2020) 1.5.mkv", "wav", false, false, false, "[Group] Name (2020) 1.5.wav"),
        ("/a/[Group] Name (2020) 1.5.mkv", "wav", false, false, true, "[Group] Name (2020) 1.5.wav"),
        ("/a/[Group] Name (2020) 1.5.mkv", "wav", false, true, false, "[Group]_Name_(2020)_1.5.wav"),
        ("/a/[Group] Name (2020) 1.5.mkv", "wav", true, true, true, "\"[Group]_Name_(2020)_1.5.wav\""),
        ("/a/[Group] Name (2020) 1.5.mkv", "wav", true, false, false, "\"[Group] Name (2020) 1.5.wav\""),
        ("a..b", "wav", false, false, false, "a..wav"),
        ("a..b", "wav", false, false, true, "a..wav"),
        ("a..b", "wav", false, true, false, "a..wav"),
        ("a..b", "wav", true, true, true, "\"a..wav\""),
        ("a..b", "wav", true, false, false, "\"a..wav\""),
        ("...", "wav", false, false, false, "....wav"),
        ("...", "wav", false, false, true, "....wav"),
        ("...", "wav", false, true, false, "....wav"),
        ("...", "wav", true, true, true, "\"....wav\""),
        ("...", "wav", true, false, false, "\"....wav\""),
        ("File.name.with.dots.ac3", "wav", false, false, false, "File.name.with.dots.wav"),
        ("File.name.with.dots.ac3", "wav", false, false, true, "File.name.with.dots.wav"),
        ("File.name.with.dots.ac3", "wav", false, true, false, "File.name.with.dots.wav"),
        ("File.name.with.dots.ac3", "wav", true, true, true, "\"File.name.with.dots.wav\""),
        ("File.name.with.dots.ac3", "wav", true, false, false, "\"File.name.with.dots.wav\""),
        ("/mnt/c/Movie Ünï.mkv", "wav", false, false, false, "Movie Ünï.wav"),
        ("/mnt/c/Movie Ünï.mkv", "wav", false, false, true, "Movie Ünï.wav"),
        ("/mnt/c/Movie Ünï.mkv", "wav", false, true, false, "Movie_Ünï.wav"),
        ("/mnt/c/Movie Ünï.mkv", "wav", true, true, true, "\"Movie_Ünï.wav\""),
        ("/mnt/c/Movie Ünï.mkv", "wav", true, false, false, "\"Movie Ünï.wav\""),
    ];

    #[rustfmt::skip]
    const CLOSEST_CASES: &[(i64, &[u32], u32)] = &[
        (0, &[96, 112, 128, 160, 192, 224, 256, 320, 384, 448, 512, 576, 640], 96),
        (96, &[96, 112, 128, 160, 192, 224, 256, 320, 384, 448, 512, 576, 640], 96),
        (100, &[96, 112, 128, 160, 192, 224, 256, 320, 384, 448, 512, 576, 640], 96),
        (104, &[96, 112, 128, 160, 192, 224, 256, 320, 384, 448, 512, 576, 640], 96),
        (105, &[96, 112, 128, 160, 192, 224, 256, 320, 384, 448, 512, 576, 640], 112),
        (136, &[96, 112, 128, 160, 192, 224, 256, 320, 384, 448, 512, 576, 640], 128),
        (144, &[96, 112, 128, 160, 192, 224, 256, 320, 384, 448, 512, 576, 640], 128),
        (145, &[96, 112, 128, 160, 192, 224, 256, 320, 384, 448, 512, 576, 640], 160),
        (300, &[96, 112, 128, 160, 192, 224, 256, 320, 384, 448, 512, 576, 640], 320),
        (352, &[96, 112, 128, 160, 192, 224, 256, 320, 384, 448, 512, 576, 640], 320),
        (1000, &[96, 112, 128, 160, 192, 224, 256, 320, 384, 448, 512, 576, 640], 640),
        (-50, &[96, 112, 128, 160, 192, 224, 256, 320, 384, 448, 512, 576, 640], 96),
        (640, &[96, 112, 128, 160, 192, 224, 256, 320, 384, 448, 512, 576, 640], 640),
        (641, &[96, 112, 128, 160, 192, 224, 256, 320, 384, 448, 512, 576, 640], 640),
        (10, &[5, 15], 5),
        (10, &[15, 5], 15),
        (7, &[3, 11, 9], 9),
    ];

    #[rustfmt::skip]
    const G_CASES: &[(f64, &str)] = &[
        (0.0, "0"),
        (1.0, "1"),
        (2.0, "2"),
        (0.5, "0.5"),
        (1.5, "1.5"),
        (12.0, "12"),
        (4.25, "4.25"),
        (100.0, "100"),
        (1234.5, "1234.5"),
        (9999.0, "9999"),
        (0.1, "0.1"),
        (0.0001, "0.0001"),
        (3.141595, "3.1416"),
        (123456.0, "123456"),
        (1234567.0, "1.23457e+06"),
        (1.234e-05, "1.234e-05"),
        (1e-07, "1e-07"),
        (999999.5, "1e+06"),
        (99999.95, "99999.9"),
        (25000000000.0, "2.5e+10"),
        (0.3333333333333333, "0.333333"),
        (3.3333333333333335, "3.33333"),
        (8.0, "8"),
        (0.30000000000000004, "0.3"),
        (-0.0, "-0"),
        (-2.5, "-2.5"),
    ];
    #[test]
    fn delay_matches_python() {
        for &(inp, comp, ref expected) in DELAY_CASES {
            let got = convert_delay_to_ms(inp, comp);
            match expected {
                Some((p, x, m)) => {
                    assert_eq!(
                        got,
                        Ok((p.to_string(), x.to_string(), *m)),
                        "{inp:?} {comp}"
                    )
                }
                None => assert_eq!(got, Err(()), "{inp:?} {comp}"),
            }
        }
        // Python: OverflowError (timedelta) / float inf formatting.
        let huge = format!("-{}s", "9".repeat(400));
        assert_eq!(convert_delay_to_ms(&huge, 256), Err(()));
        assert_eq!(convert_delay_to_ms(&huge, 0), Err(()));
    }

    #[test]
    fn delay_seconds_for_dee() {
        let secs = |inp, comp| delay_seconds(inp, comp).map(|(m, s)| (m, format!("{s:.6}")));
        // trimmed: DDP minus 16/3 ms
        assert_eq!(
            secs("+0ms", 256),
            Ok((DelayMode::Start, "0.005333".to_string()))
        );
        assert_eq!(
            secs("+100ms", 256),
            Ok((DelayMode::PrependSilenceDuration, "0.094667".to_string()))
        );
        assert_eq!(
            secs("-24ms", 0),
            Ok((DelayMode::Start, "0.024000".to_string()))
        );
        assert_eq!(
            secs("-3700s", 0),
            Ok((DelayMode::Start, "3700.000000".to_string()))
        );
        assert_eq!(
            secs("-100000s", 256),
            Ok((DelayMode::Start, "100000.005333".to_string()))
        );
        assert_eq!(
            secs("-0ms", 0),
            Ok((DelayMode::PrependSilenceDuration, "-0.000000".to_string()))
        );
        assert_eq!(secs("100ms", 256), Err(()));
        assert_eq!(secs("+5@nan", 256), Err(()));
    }

    #[test]
    fn dee_delay_options() {
        let opt = |inp, comp| delay_options(inp, comp).unwrap();
        assert_eq!(opt("+0ms", 256), ["--start", "0.005333"]);
        assert_eq!(opt("+100ms", 256), ["--add-silence", "0.094667:0"]);
        assert_eq!(opt("-24ms", 0), ["--start", "0.024"]);
        assert_eq!(opt("+1s", 0), ["--add-silence", "1:0"]);
        // nothing to do
        assert!(opt("+0ms", 0).is_empty());
        assert!(opt("-0ms", 0).is_empty());
        assert!(opt("+5.3333ms", 256).is_empty());
        assert_eq!(delay_options("x", 256), Err(()));
    }

    #[test]
    fn delay_options_other_encoder_delays() {
        let opt = |inp, comp| delay_options(inp, comp).unwrap();
        assert_eq!(opt("+0ms", 2048), ["--start", "0.042667"]);
        assert_eq!(opt("+0ms", 1600), ["--start", "0.033333"]);
        assert_eq!(opt("+100ms", 1600), ["--add-silence", "0.066667:0"]);
        assert!(opt("+0ms", 0).is_empty());
    }

    #[test]
    fn timedelta_parsing() {
        assert_eq!(parse_timedelta("0:00:00.005333"), Some(0.005333));
        assert_eq!(parse_timedelta("1:01:40.0"), Some(3700.0));
        assert_eq!(
            parse_timedelta("1 day, 3:46:40.005333"),
            Some(100000.005333)
        );
        assert_eq!(parse_timedelta("2 days, 0:00:00.0"), Some(172800.0));
        assert_eq!(parse_timedelta("nonsense"), None);
    }

    #[test]
    fn timedelta_matches_python() {
        for &(secs, expected) in TIMEDELTA_CASES {
            assert_eq!(python_timedelta_str(secs), expected, "{secs:e}");
        }
    }

    #[test]
    fn basename_matches_python() {
        for &(path, ext, quote, sanitize, strip, expected) in BASENAME_CASES {
            assert_eq!(
                basename(path, ext, quote, sanitize, strip),
                expected,
                "{path:?}"
            );
        }
    }

    #[test]
    fn ntpath_names() {
        for (path, name, stem) in [
            ("C:\\dir\\file.wav", "file.wav", "file"),
            ("C:/dir/file.wav", "file.wav", "file"),
            ("C:\\dir/sub\\a.b.mkv", "a.b.mkv", "a.b"),
            ("C:file.wav", "file.wav", "file"),
            ("C:", "", ""),
            ("C:\\", "", ""),
            ("C:\\dir.d\\file", "file", "file"),
            ("C:\\dir\\.hidden", ".hidden", ".hidden"),
            ("\\\\srv\\share\\x.flac", "x.flac", "x"),
            ("file.wav", "file.wav", "file"),
            ("\u{e9}:\\x.wav", "x.wav", "x"),
        ] {
            assert_eq!(&path[name_start(path, true)..], name, "{path:?}");
            assert_eq!(stem_basename(path, true), stem, "{path:?}");
            assert_eq!(
                basename_for(path, "xml", false, false, false, true),
                format!("{stem}.xml"),
                "{path:?}"
            );
        }
        // Not Windows: a backslash and a drive letter are ordinary characters.
        assert_eq!(&"C:\\a.wav"[name_start("C:\\a.wav", false)..], "C:\\a.wav");
        assert_eq!(stem_basename("C:/dir/f.wav", false), "f");
    }

    #[cfg(windows)]
    #[test]
    fn native_names_on_windows() {
        assert_eq!(file_name("C:\\dir\\file.wav"), "file.wav");
        assert_eq!(file_name("C:/dir/file.wav"), "file.wav");
        assert_eq!(
            basename("C:\\dir\\a b.wav", "xml", true, true, false),
            "\"a_b.xml\""
        );
        assert_eq!(
            basename("C:/dir/a DELAY -5ms.mkv", "ac3", false, false, true),
            "a.ac3"
        );
    }

    #[cfg(not(windows))]
    #[test]
    fn native_names_on_unix() {
        assert_eq!(file_name("/a/b/c.mkv"), "c.mkv");
        assert_eq!(file_name("a\\b.mkv"), "a\\b.mkv");
    }

    #[test]
    fn closest_allowed_matches_python() {
        for &(v, allowed, expected) in CLOSEST_CASES {
            assert_eq!(find_closest_allowed(v, allowed), expected, "{v}");
        }
    }

    #[test]
    fn format_g_matches_python() {
        for &(v, expected) in G_CASES {
            assert_eq!(format_g(v), expected, "{v:e}");
        }
    }

    #[test]
    fn clamps() {
        assert_eq!(clamp_i(5, 0, 3), 3);
        assert_eq!(clamp_i(-5, 0, 3), 0);
        // lo > hi: Python's min(max(v, lo), hi) yields hi.
        assert_eq!(clamp_i(5, 10, 3), 3);
        assert_eq!(clamp_f(5.0, 10.0, 3.0), 3.0);
        assert_eq!(clamp_f(0.5, 0.0, 1.0), 0.5);
    }

    #[test]
    fn versions() {
        assert!(version_lt("3.2.2", "3.2.10"));
        assert!(version_lt("3.2", "3.2.1"));
        assert!(!version_lt("3.2.0", "3.2"));
        assert!(version_lt("v1", "2")); // non-numeric component counts as 0
    }

    #[test]
    fn int_list() {
        assert_eq!(py_int_list(&[48000, 44100]), "[48000, 44100]");
        assert_eq!(py_int_list(&[]), "[]");
        assert_eq!(py_int_list(&[-1]), "[-1]");
    }
}
