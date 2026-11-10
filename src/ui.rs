//! Progress display: one line per running job (two lines on a narrow
//! terminal), a queue line at the bottom, and static `finished:` / `failed:`
//! lines printed above them.
//!
//! The layout is a pure function ([`render_rows`]): it gets the width of the
//! terminal and the state of all live lines and returns the rendered lines.
//! The live renderer ([`crate::live::Live`]) only draws what it returns.

use crate::events;
use crate::live::Live;
use crate::markup::{color_enabled, render};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

pub use crate::events::Stage;

/// The bar is at most this wide.
const BAR_MAX: usize = 30;
/// The bar is not narrower than this unless the console is tiny.
const BAR_MIN: usize = 10;
/// `99.99%`, reserved for every line.
const PERCENT_RESERVED: usize = 6;
/// ` - ~9:59:59 left`, reserved for every line.
const TIME_RESERVED: usize = 16;
/// ` │ ` between description and bar.
const SEPARATOR_WIDTH: usize = 3;
/// Everything but description and bar of a one-line row: the separator, the
/// space before the percentage and the reserved percentage and time.
const FIXED_WIDTH: usize = SEPARATOR_WIDTH + 1 + PERCENT_RESERVED + TIME_RESERVED;
/// Below this console width every job gets two lines.
const TWO_LINE_BELOW: usize = 80;
/// The remaining time is shown after the step ran this long.
const TIME_AFTER: f64 = 5.0;
/// Estimates above this many seconds (9:59:59) are shown as `??`.
const TIME_MAX: u64 = 9 * 3600 + 59 * 60 + 59;

// 256-color codes.
const COLOR_DECODE: u8 = 39;
const COLOR_MEASURE: u8 = 214;
const COLOR_ENCODE: u8 = 197;
const COLOR_BACK: u8 = 59;
const COLOR_SEPARATOR: u8 = 240;
const COLOR_GREY: u8 = 245;
const BOLD_CYAN: &str = "\x1b[1;36m";
const BOLD_GREEN: &str = "\x1b[1;32m";
const BOLD_RED: &str = "\x1b[1;31m";
const MAGENTA: &str = "\x1b[35m";
const RESET: &str = "\x1b[0m";

fn fg(code: u8) -> String {
    format!("\x1b[38;5;{code}m")
}

/// Number of pulse positions per second, like rich.
const PULSE_SPEED: f64 = 15.0;
const PULSE_SIZE: usize = 20;

/// RGB of a color of the 6x6x6 cube of the 256-color palette.
fn cube_rgb(code: u8) -> (u8, u8, u8) {
    const LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];
    let n = usize::from(code.saturating_sub(16));
    (LEVELS[n / 36 % 6], LEVELS[n / 6 % 6], LEVELS[n % 6])
}

/// The cube color closest to `rgb`.
fn nearest_cube(rgb: (f64, f64, f64)) -> u8 {
    const LEVELS: [f64; 6] = [0.0, 95.0, 135.0, 175.0, 215.0, 255.0];
    let level = |v: f64| {
        (0..6)
            .min_by(|&a, &b| (LEVELS[a] - v).abs().total_cmp(&(LEVELS[b] - v).abs()))
            .unwrap_or(0) as u8
    };
    16 + 36 * level(rgb.0) + 6 * level(rgb.1) + level(rgb.2)
}

/// Color `ratio` (1 is `color`, 0 is the grey of the unfilled bar).
fn mix(color: u8, ratio: f64) -> u8 {
    let (r, g, b) = cube_rgb(color);
    let (br, bg, bb) = cube_rgb(COLOR_BACK);
    let m = |a: u8, b: u8| f64::from(b) + (f64::from(a) - f64::from(b)) * ratio;
    nearest_cube((m(r, br), m(g, bg), m(b, bb)))
}

/// Color of the pulse at `index` of its cycle: `color` fading to grey.
fn pulse_color(index: usize, color: u8) -> String {
    let angle = 2.0 * std::f64::consts::PI * index as f64 / PULSE_SIZE as f64;
    let ratio = (1.0 + angle.cos()) / 2.0;
    fg(if ratio > 0.75 {
        color
    } else if ratio > 0.4 {
        mix(color, 0.6)
    } else if ratio > 0.15 {
        mix(color, 0.3)
    } else {
        COLOR_BACK
    })
}

/// The bar, `width` cells wide, in the 256-color `code`. `fraction` is
/// `None` for an unknown total (a pulsing bar, `time` in seconds moves it).
pub fn bar_text(fraction: Option<f64>, color: bool, time: f64, width: usize, code: u8) -> String {
    let mut out = String::new();
    match fraction {
        Some(fraction) => {
            let fraction = fraction.clamp(0.0, 1.0);
            let halves = (width as f64 * 2.0 * fraction) as usize;
            let (full, half) = (halves / 2, halves % 2);
            if color {
                out.push_str(&fg(code));
            }
            out.push_str(&"━".repeat(full));
            out.push_str(&"╸".repeat(half));
            let mut remaining = width - full - half;
            if color && remaining > 0 {
                out.push_str(&fg(COLOR_BACK));
                if half == 0 && full > 0 {
                    out.push('╺');
                    remaining -= 1;
                }
            }
            if color {
                out.push_str(&"━".repeat(remaining));
                out.push_str(RESET);
            } else {
                out.push_str(&" ".repeat(remaining));
            }
        }
        None => {
            let offset = (-(time * PULSE_SPEED)).rem_euclid(PULSE_SIZE as f64) as usize;
            for i in 0..width {
                let index = (i + offset) % PULSE_SIZE;
                if color {
                    out.push_str(&pulse_color(index, code));
                }
                out.push('━');
            }
            if color {
                out.push_str(RESET);
            }
        }
    }
    out
}

/// Color of a finished bar of the download display.
const COLOR_FINISHED: u8 = 70;

/// Whether a console `width` wide gets two lines per row.
pub fn two_lines(width: usize) -> bool {
    width < TWO_LINE_BELOW
}

/// Width of the bar of a two-line row (it has a line of its own).
pub fn two_line_bar(width: usize) -> usize {
    width
        .saturating_sub(PERCENT_RESERVED + TIME_RESERVED + 1)
        .clamp(BAR_MIN, BAR_MAX)
}

/// ` │ ` between the columns of a row.
pub fn separator(color: bool) -> String {
    if color {
        format!(" {}│{RESET} ", fg(COLOR_SEPARATOR))
    } else {
        " │ ".to_string()
    }
}

/// The name of a tool, bold cyan.
pub fn tool_text(tool: &str, color: bool) -> String {
    if color {
        format!("{BOLD_CYAN}{tool}{RESET}")
    } else {
        tool.to_string()
    }
}

/// Bar, then (unless it pulses) the percentage and the remaining time.
/// `secs` is how long the step ran: it moves a pulsing bar and is the base
/// of the estimate.
pub fn tail_text(fraction: Option<f64>, secs: f64, width: usize, code: u8, color: bool) -> String {
    let mut out = bar_text(fraction, color, secs, width, code);
    if fraction.is_some() {
        out.push(' ');
        out.push_str(&percent_text(fraction, color));
        out.push_str(&time_text(fraction, secs));
    }
    out
}

/// [`tail_text`] of the download display: pink, green when finished.
pub fn download_tail_text(fraction: Option<f64>, secs: f64, width: usize, color: bool) -> String {
    let code = if fraction.is_some_and(|f| f >= 1.0) {
        COLOR_FINISHED
    } else {
        COLOR_ENCODE
    };
    tail_text(fraction, secs, width, code, color)
}

/// `{task.percentage:.2f}%` in magenta, not padded. A finished bar shows
/// `100%`, an unfinished one at most `99.99%`.
pub fn percent_text(fraction: Option<f64>, color: bool) -> String {
    let percent = fraction.unwrap_or(0.0) * 100.0;
    let text = if percent >= 100.0 {
        "100%".to_string()
    } else {
        format!("{:.2}%", percent.min(99.99))
    };
    if color {
        format!("{MAGENTA}{text}{RESET}")
    } else {
        text
    }
}

/// `m:ss`, or `h:mm:ss` from one hour on.
pub fn clock(seconds: u64) -> String {
    let (h, m, s) = (seconds / 3600, seconds / 60 % 60, seconds % 60);
    if h > 0 {
        format!("{h}:{m:02}:{s:02}")
    } else {
        format!("{m}:{s:02}")
    }
}

/// ` - ~1:12 left`: shown once the step ran for 5 seconds. The estimate is
/// linear within the step. Nothing for a pulsing bar, at 0 and at 100 %.
pub fn time_text(fraction: Option<f64>, step_secs: f64) -> String {
    let Some(f) = fraction.filter(|f| *f > 0.0 && *f < 1.0) else {
        return String::new();
    };
    if step_secs < TIME_AFTER {
        return String::new();
    }
    let left = (step_secs * (1.0 - f) / f).round();
    if !left.is_finite() || left > TIME_MAX as f64 {
        " - ?? left".to_string()
    } else {
        format!(" - ~{} left", clock(left as u64))
    }
}

/// Cut `name` to at most `max` columns: the start, `...`, the extension
/// without its dot (`really_really...thd`). Without an extension just `...`.
pub fn cut_name(name: &str, max: usize) -> String {
    if name.width() <= max {
        return name.to_string();
    }
    let ext = name
        .rfind('.')
        .filter(|&i| i > 0)
        .map_or("", |i| &name[i + 1..]);
    let ext = if ext.width() + 4 > max { "" } else { ext };
    let budget = max.saturating_sub(3 + ext.width());
    let mut head = String::new();
    let mut used = 0;
    for c in name.chars() {
        let w = c.width().unwrap_or(0);
        if used + w > budget {
            break;
        }
        head.push(c);
        used += w;
    }
    format!("{head}...{ext}")
}

/// `+3 files in queue`.
pub fn queue_text(count: usize, color: bool) -> String {
    let text = format!(
        "+{count} file{} in queue",
        if count == 1 { "" } else { "s" }
    );
    if color {
        format!("{}{text}{RESET}", fg(COLOR_GREY))
    } else {
        text
    }
}

/// The static line of a finished job.
pub fn finished_text(name: &str, db: Option<i64>, took: u64, color: bool) -> String {
    let db = db.map_or(String::new(), |db| format!(" ({db} dB)"));
    let took = format!(" - took {}", clock(took));
    if color {
        format!(
            "{BOLD_GREEN}finished:{RESET} {name}{}{db}{took}{RESET}",
            fg(COLOR_GREY)
        )
    } else {
        format!("finished: {name}{db}{took}")
    }
}

/// The static line of a failed job.
pub fn failed_text(name: &str, error: &str, color: bool) -> String {
    if color {
        format!("{BOLD_RED}failed:{RESET} {name} - {error}")
    } else {
        format!("failed: {name} - {error}")
    }
}

/// One live line as the layout sees it.
#[derive(Debug, Clone)]
pub struct Row<'a> {
    pub stage: Stage,
    pub name: &'a str,
    pub db: Option<i64>,
    /// 0 to 1, `None` for an unknown total (a pulsing bar).
    pub fraction: Option<f64>,
    /// Seconds since the current step began.
    pub step_secs: f64,
}

impl Row<'_> {
    /// Tool, verb and the color of the step; `None` before the first step.
    fn step(&self) -> Option<(&'static str, &'static str, u8)> {
        match self.stage {
            Stage::Starting => None,
            Stage::Ffmpeg => Some(("ffmpeg", "decode", COLOR_DECODE)),
            Stage::Decode => Some(("truehdd", "decode", COLOR_DECODE)),
            Stage::Measure => Some(("DEE", "measure", COLOR_MEASURE)),
            Stage::Encode => Some(("DEE", "encode", COLOR_ENCODE)),
        }
    }

    fn code(&self) -> u8 {
        self.step().map_or(COLOR_ENCODE, |(_, _, code)| code)
    }

    fn suffix(&self) -> String {
        self.db.map_or(String::new(), |db| format!(" ({db} dB)"))
    }

    /// The description, the filename cut to at most `max_name` columns.
    /// Returns the text and its visible width.
    fn desc(&self, max_name: usize, color: bool) -> (String, usize) {
        let Some((tool, verb, code)) = self.step() else {
            let text = "starting...";
            let out = if color {
                format!("{BOLD_CYAN}starting{RESET}...")
            } else {
                text.to_string()
            };
            return (out, text.len());
        };
        let name = cut_name(self.name, max_name);
        let suffix = self.suffix();
        let width = self.fixed_desc_width() + name.width();
        let out = if color {
            let grey = if suffix.is_empty() {
                String::new()
            } else {
                format!("{}{suffix}{RESET}", fg(COLOR_GREY))
            };
            format!(
                "{} {}{verb}{RESET}: {name}{grey}",
                tool_text(tool, true),
                fg(code)
            )
        } else {
            format!("{tool} {verb}: {name}{suffix}")
        };
        (out, width)
    }

    /// Width of the description without the filename.
    fn fixed_desc_width(&self) -> usize {
        match self.step() {
            None => "starting...".len(),
            Some((tool, verb, _)) => tool.width() + 1 + verb.width() + 2 + self.suffix().width(),
        }
    }

    /// Bar, percentage and time.
    fn tail(&self, bar: usize, color: bool) -> String {
        tail_text(self.fraction, self.step_secs, bar, self.code(), color)
    }
}

/// Render the live lines for a console `width` wide: one line per row
/// (descriptions aligned on the `│`), or two lines per row (description,
/// then bar) below 80 columns. The result has one entry per row, each with
/// one or two lines.
pub fn render_rows(width: usize, rows: &[Row], color: bool) -> Vec<Vec<String>> {
    if two_lines(width) {
        let bar = two_line_bar(width);
        return rows
            .iter()
            .map(|row| {
                let max_name = width.saturating_sub(row.fixed_desc_width());
                vec![row.desc(max_name, color).0, row.tail(bar, color)]
            })
            .collect();
    }
    let natural = rows
        .iter()
        .map(|row| row.desc(usize::MAX, false).1)
        .max()
        .unwrap_or(0);
    // the description column, shrunk (by cutting filenames) only if the
    // smallest bar would not fit
    let column = if natural + FIXED_WIDTH + BAR_MIN <= width {
        natural
    } else {
        width - FIXED_WIDTH - BAR_MIN
    };
    let descs: Vec<(String, usize)> = rows
        .iter()
        .map(|row| {
            let max_name = column.saturating_sub(row.fixed_desc_width());
            row.desc(max_name, color)
        })
        .collect();
    let column = descs.iter().map(|(_, w)| *w).max().unwrap_or(0);
    let bar = width.saturating_sub(FIXED_WIDTH + column);
    let bar = bar.clamp(BAR_MIN, BAR_MAX);
    let separator = separator(color);
    rows.iter()
        .zip(descs)
        .map(|(row, (desc, desc_width))| {
            let pad = " ".repeat(column - desc_width);
            vec![format!("{desc}{pad}{separator}{}", row.tail(bar, color))]
        })
        .collect()
}

/// Minimum time between two percent events of one bar.
const EVENT_INTERVAL: Duration = Duration::from_millis(100);

/// Whether a percent change is written as an event: always for 0, 100 and
/// the first one after a stage change, else at most every `EVENT_INTERVAL`.
fn should_emit(pct: f64, fresh: bool, last: Option<Instant>, now: Instant) -> bool {
    fresh
        || pct <= 0.0
        || pct >= 100.0
        || last.is_none_or(|last| now.saturating_duration_since(last) >= EVENT_INTERVAL)
}

/// What was last written as an event of a bar.
struct EventState {
    stage: Option<Stage>,
    db: Option<i64>,
    /// `None` while the total is unknown.
    percent: Option<f64>,
    /// A stage change was not followed by a percent event yet.
    fresh: bool,
    last: Option<Instant>,
}

/// The state of a live line.
struct Job {
    name: String,
    stage: Stage,
    db: Option<i64>,
    fraction: Option<f64>,
    step_started: Instant,
}

type LiveMap = Arc<Mutex<BTreeMap<usize, Job>>>;

/// A progress line of one job.
pub struct Bar {
    index: usize,
    name: String,
    started: Instant,
    live: LiveMap,
    events: Mutex<EventState>,
}

impl Bar {
    fn with_live(&self, f: impl FnOnce(&mut Job)) {
        if let Some(live) = self.live.lock().expect("live lines").get_mut(&self.index) {
            f(live);
        }
    }

    /// Set the step; `db` is the measured loudness, once known. A new step
    /// starts at 0 and restarts the step timer.
    pub fn describe(&self, stage: Stage, db: Option<i64>) {
        let mut new_step = false;
        self.with_live(|live| {
            new_step = live.stage != stage;
            if new_step {
                live.step_started = Instant::now();
                live.fraction = Some(0.0);
            }
            live.stage = stage;
            live.db = db;
        });
        if !events::enabled() {
            return;
        }
        let mut state = self.events.lock().expect("event state");
        if state.stage != Some(stage) || state.db != db {
            if state.stage != Some(stage) {
                state.percent = Some(0.0);
            }
            state.stage = Some(stage);
            state.db = db;
            state.fresh = true;
            state.last = Some(Instant::now());
            events::progress(self.index, stage, state.percent, db);
        }
    }

    /// Write a progress event for the current state, if a stage is known.
    fn emit(&self, state: &mut EventState, now: Instant) {
        if let Some(stage) = state.stage {
            state.fresh = false;
            state.last = Some(now);
            events::progress(self.index, stage, state.percent, state.db);
        }
    }

    /// Set the progress of the step, 0 to 100.
    pub fn percent(&self, pct: f64) {
        let pct = if pct.is_nan() {
            0.0
        } else {
            pct.clamp(0.0, 100.0)
        };
        self.with_live(|live| live.fraction = Some(pct / 100.0));
        if !events::enabled() {
            return;
        }
        let mut state = self.events.lock().expect("event state");
        state.percent = Some(pct);
        let now = Instant::now();
        if should_emit(pct, state.fresh, state.last, now) {
            self.emit(&mut state, now);
        }
    }

    /// Unknown total: show a pulsing bar.
    pub fn indeterminate(&self) {
        self.with_live(|live| live.fraction = None);
        if !events::enabled() {
            return;
        }
        let mut state = self.events.lock().expect("event state");
        if state.percent.is_some() {
            state.percent = None;
            self.emit(&mut state, Instant::now());
        }
    }
}

/// How a job ended.
pub enum End<'a> {
    Done,
    /// The message of the failed step.
    Failed(&'a str),
    /// Stopped by Ctrl-C: nothing is printed.
    Stopped,
}

/// The text of the queue line, `None` when there is none.
type QueueText = Arc<Mutex<Option<String>>>;

/// All progress lines plus a way to print messages above them.
pub struct Ui {
    screen: Live,
    live: LiveMap,
    total: usize,
    /// How many jobs were started.
    started: Mutex<usize>,
    queue: QueueText,
}

impl Ui {
    /// `total` is the number of jobs.
    pub fn new(total: usize) -> Ui {
        // The queue line exists from the start, as long as there is more
        // than the first job to wait for.
        let queue: QueueText = Arc::new(Mutex::new(
            (total > 1).then(|| queue_text(total - 1, color_enabled())),
        ));
        let live: LiveMap = Arc::new(Mutex::new(BTreeMap::new()));
        let screen = {
            let (live, queue) = (live.clone(), queue.clone());
            let color = color_enabled();
            Live::new(move |width| {
                let jobs = live.lock().expect("live lines");
                let rows: Vec<Row> = jobs
                    .values()
                    .map(|l| Row {
                        stage: l.stage,
                        name: &l.name,
                        db: l.db,
                        fraction: l.fraction,
                        step_secs: l.step_started.elapsed().as_secs_f64(),
                    })
                    .collect();
                let mut lines: Vec<String> = render_rows(width, &rows, color)
                    .into_iter()
                    .flatten()
                    .collect();
                drop(jobs);
                if let Some(text) = queue.lock().expect("queue").as_ref() {
                    lines.push(text.clone());
                }
                lines
            })
        };
        Ui {
            screen,
            live,
            total,
            started: Mutex::new(0),
            queue,
        }
    }

    /// Add a progress line for the job `index` above the queue line.
    pub fn add_bar(&self, index: usize, name: &str) -> Bar {
        let mut started = self.started.lock().expect("started");
        self.live.lock().expect("live lines").insert(
            index,
            Job {
                name: name.to_string(),
                stage: Stage::Starting,
                db: None,
                fraction: Some(0.0),
                step_started: Instant::now(),
            },
        );
        *started += 1;
        let waiting = self.total.saturating_sub(*started);
        let mut queue = self.queue.lock().expect("queue");
        if queue.is_some() {
            *queue = (waiting > 0).then(|| queue_text(waiting, color_enabled()));
        }
        Bar {
            index,
            name: name.to_string(),
            started: Instant::now(),
            live: self.live.clone(),
            events: Mutex::new(EventState {
                stage: None,
                db: None,
                percent: Some(0.0),
                fresh: false,
                last: None,
            }),
        }
    }

    /// The job is over: remove its live line and print the result above
    /// the others.
    pub fn finish(&self, bar: Bar, end: End) {
        let db = self
            .live
            .lock()
            .expect("live lines")
            .remove(&bar.index)
            .and_then(|job| job.db);
        events::done(bar.index);
        let color = color_enabled();
        let line = match end {
            End::Done => Some(finished_text(
                &bar.name,
                db,
                bar.started.elapsed().as_secs(),
                color,
            )),
            End::Failed(error) => Some(failed_text(&bar.name, error, color)),
            End::Stopped => None,
        };
        match line {
            Some(line) => self.say_raw(&line),
            None => self.screen.redraw(),
        }
    }

    /// Print a line above the progress lines. `markup` is rendered, plain
    /// text has to be escaped by the caller.
    pub fn say(&self, markup: &str) {
        let text = render(markup, color_enabled());
        self.say_raw(&text);
    }

    /// A function that prints like [`Ui::say`], for other threads.
    pub fn printer(&self) -> impl Fn(&str) + Send + 'static {
        let print = self.screen.printer();
        move |markup| print(&render(markup, color_enabled()))
    }

    /// Print text that is not parsed as markup.
    pub fn say_raw(&self, text: &str) {
        self.screen.println(text);
    }

    /// All jobs are over: remove what is left of the queue line (jobs that
    /// were never started after Ctrl-C) and the live lines.
    pub fn flush(self) {
        *self.queue.lock().expect("queue") = None;
        self.screen.finish(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markup::strip;

    fn row(stage: Stage, name: &str) -> Row<'_> {
        Row {
            stage,
            name,
            db: None,
            fraction: Some(0.0),
            step_secs: 0.0,
        }
    }

    fn plain(s: &str) -> String {
        let mut out = String::new();
        let mut chars = s.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                for c in chars.by_ref() {
                    if c == 'm' {
                        break;
                    }
                }
            } else {
                out.push(c);
            }
        }
        out
    }

    #[test]
    fn bar_without_color() {
        let bar = |f, w| bar_text(Some(f), false, 0.0, w, 197);
        assert_eq!(bar(1.0, 30), "━".repeat(30));
        assert_eq!(bar(0.0, 30), " ".repeat(30));
        // 45.5%: 13 full cells and a half
        assert_eq!(
            bar(0.455, 30),
            format!("{}╸{}", "━".repeat(13), " ".repeat(16))
        );
        assert_eq!(
            bar(0.5, 30),
            format!("{}{}", "━".repeat(15), " ".repeat(15))
        );
        for f in [0.0, 0.3, 0.999, 1.0, 7.0, -1.0] {
            assert_eq!(strip(&bar(f, 30)).chars().count(), 30);
            assert_eq!(
                plain(&bar_text(Some(f), true, 0.0, 30, 39)).chars().count(),
                30
            );
        }
        assert_eq!(bar_text(None, false, 0.0, 7, 197).chars().count(), 7);
    }

    #[test]
    fn pulsing_bar_is_full_width_and_moves() {
        let a = bar_text(None, true, 0.0, 30, 214);
        let b = bar_text(None, true, 0.2, 30, 214);
        assert_ne!(a, b);
        assert_eq!(bar_text(None, false, 3.0, 30, 214), "━".repeat(30));
        assert!(a.contains("\x1b[38;5;214m"));
        assert!(a.contains("\x1b[38;5;59m"));
        // the fade stays between the step color and grey
        for code in [39, 214, 197] {
            for i in 0..PULSE_SIZE {
                assert!(pulse_color(i, code).starts_with("\x1b[38;5;"));
            }
        }
    }

    #[test]
    fn colored_bar_uses_the_step_color_and_back_style() {
        for code in [39u8, 214, 197] {
            let text = bar_text(Some(0.5), true, 0.0, 30, code);
            assert!(text.starts_with(&fg(code)));
            assert!(text.contains(&fg(COLOR_BACK)));
            assert!(text.contains('╺'));
            assert!(bar_text(Some(1.0), true, 0.0, 30, code).starts_with(&fg(code)));
        }
    }

    #[test]
    fn percent_events_are_throttled() {
        let t0 = Instant::now();
        let later = |ms| t0 + Duration::from_millis(ms);
        assert!(should_emit(5.0, false, None, t0));
        assert!(!should_emit(5.0, false, Some(t0), later(50)));
        assert!(should_emit(5.0, false, Some(t0), later(100)));
        // 0, 100 and the first one after a stage change are never dropped
        assert!(should_emit(0.0, false, Some(t0), later(1)));
        assert!(should_emit(100.0, false, Some(t0), later(1)));
        assert!(should_emit(5.0, true, Some(t0), later(1)));
    }

    #[test]
    fn percentage() {
        assert_eq!(percent_text(Some(0.4555), false), "45.55%");
        assert_eq!(percent_text(Some(0.7), false), "70.00%");
        assert_eq!(percent_text(Some(1.0), false), "100%");
        assert_eq!(percent_text(Some(0.99996), false), "99.99%");
        assert_eq!(percent_text(None, false), "0.00%");
        assert_eq!(percent_text(Some(0.0), true), "\x1b[35m0.00%\x1b[0m");
    }

    #[test]
    fn clocks() {
        assert_eq!(clock(0), "0:00");
        assert_eq!(clock(65), "1:05");
        assert_eq!(clock(615), "10:15");
        assert_eq!(clock(3600), "1:00:00");
        assert_eq!(clock(4540), "1:15:40");
    }

    #[test]
    fn remaining_time() {
        // hidden before 5 seconds, without percentage and at 0 and 100 %
        assert_eq!(time_text(Some(0.5), 4.9), "");
        assert_eq!(time_text(None, 100.0), "");
        assert_eq!(time_text(Some(0.0), 100.0), "");
        assert_eq!(time_text(Some(1.0), 100.0), "");
        // linear: 5 s for 50 % leaves 5 s
        assert_eq!(time_text(Some(0.5), 5.0), " - ~0:05 left");
        assert_eq!(time_text(Some(0.124), 15.0), " - ~1:46 left");
        assert_eq!(time_text(Some(0.1), 60.0), " - ~9:00 left");
        assert_eq!(time_text(Some(0.01), 60.0), " - ~1:39:00 left");
        // the same step at 70 %: 3600 s elapsed
        assert_eq!(time_text(Some(0.7), 3000.0), " - ~21:26 left");
        // above 9:59:59
        assert_eq!(time_text(Some(0.001), 60.0), " - ?? left");
        assert_eq!(time_text(Some(0.00001), 60.0), " - ?? left");
        // exactly at the limit still shows the time
        assert_eq!(time_text(Some(0.5), 35999.0), " - ~9:59:59 left");
        assert_eq!(time_text(Some(0.5), 36001.0), " - ?? left");
    }

    #[test]
    fn names_are_cut_before_the_extension() {
        assert_eq!(cut_name("a.thd", 10), "a.thd");
        assert_eq!(
            cut_name("really_really_really_really_really_long.thd", 33),
            "really_really_really_really...thd"
        );
        assert_eq!(cut_name("abcdefghijkl.mlp", 10), "abcd...mlp");
        assert_eq!(cut_name("abcdefghijkl", 10), "abcdefg...");
        assert_eq!(cut_name(".abcdefghijkl", 8), ".abcd...");
        assert_eq!(cut_name("abcdefghijkl.mlp", 10).chars().count(), 10);
        assert_eq!(cut_name("a.verylongextension", 8), "a.ver...");
        assert_eq!(cut_name("日本語日本語日本語.thd", 12), "日本語...thd");
    }

    #[test]
    fn queue_lines() {
        assert_eq!(queue_text(1, false), "+1 file in queue");
        assert_eq!(queue_text(3, false), "+3 files in queue");
        assert_eq!(plain(&queue_text(12, true)), "+12 files in queue");
    }

    #[test]
    fn static_lines() {
        assert_eq!(
            finished_text("a.mlp", Some(-25), 615, false),
            "finished: a.mlp (-25 dB) - took 10:15"
        );
        assert_eq!(
            finished_text("a.mlp", None, 3725, false),
            "finished: a.mlp - took 1:02:05"
        );
        assert_eq!(
            failed_text("b.mlp", "DEE: oops", false),
            "failed: b.mlp - DEE: oops"
        );
        let colored = finished_text("a.mlp", Some(-25), 5, true);
        assert!(colored.starts_with("\x1b[1;32mfinished:"));
        assert_eq!(plain(&colored), "finished: a.mlp (-25 dB) - took 0:05");
        assert!(failed_text("b", "x", true).starts_with("\x1b[1;31mfailed:"));
    }

    #[test]
    fn one_line_layout_aligns_the_separators() {
        let mut a = row(Stage::Encode, "another_track.mlp");
        a.db = Some(-27);
        a.fraction = Some(0.7);
        a.step_secs = 100.0;
        let mut b = row(Stage::Decode, "movie_atmos.thd");
        b.fraction = Some(0.124);
        b.step_secs = 2.0;
        let mut c = row(Stage::Ffmpeg, "s01e01.mkv");
        c.fraction = Some(0.124);
        let lines = render_rows(120, &[a, b, c], false);
        let lines: Vec<&str> = lines.iter().map(|l| l[0].as_str()).collect();
        assert_eq!(
            lines,
            [
                "DEE encode: another_track.mlp (-27 dB) │ ━━━━━━━━━━━━━━━━━━━━━          70.00% - ~0:43 left",
                "truehdd decode: movie_atmos.thd        │ ━━━╸                           12.40%",
                "ffmpeg decode: s01e01.mkv              │ ━━━╸                           12.40%",
            ]
        );
        let columns: Vec<usize> = lines.iter().map(|l| l.find('│').unwrap()).collect();
        assert!(columns.windows(2).all(|w| w[0] == w[1]));
    }

    #[test]
    fn description_column_follows_the_live_lines() {
        let long = row(Stage::Encode, "a_rather_long_name.mlp");
        let short = row(Stage::Encode, "a.mlp");
        let both = render_rows(120, &[long.clone(), short.clone()], false);
        let alone = render_rows(120, &[short], false);
        assert!(both[1][0].find('│') > alone[0][0].find('│'));
        assert_eq!(alone[0][0].find('│'), Some("DEE encode: a.mlp ".len()));
    }

    #[test]
    fn starting_placeholder() {
        let lines = render_rows(100, &[row(Stage::Starting, "x.wav")], false);
        assert!(lines[0][0].starts_with("starting... │ "), "{}", lines[0][0]);
    }

    #[test]
    fn bar_is_30_wide_when_there_is_room() {
        let mut r = row(Stage::Encode, "a.mlp");
        r.fraction = Some(1.0);
        // "DEE encode: a.mlp" is 17 wide: 17 + 26 + 30 = 73
        for w in [80, 120, 300] {
            let l = &render_rows(w, &[r.clone()], false)[0][0];
            assert_eq!(l.chars().filter(|&c| c == '━').count(), 30, "{w}");
        }
    }

    #[test]
    fn bar_shrinks_then_names_are_cut() {
        let name = "x".repeat(60) + ".thd";
        let mut r = row(Stage::Encode, &name);
        r.db = Some(-27);
        r.fraction = Some(1.0);
        // D = 11 + 64 + 9 = 84; 84 + 26 + 10 > 100: names are cut
        let line = render_rows(100, &[r.clone()], false)[0][0].clone();
        assert!(line.contains("...thd (-27 dB) │ "), "{line}");
        assert_eq!(line.chars().filter(|&c| c == '━').count(), 10, "{line}");
        assert_eq!(
            line.chars().count(),
            100 - PERCENT_RESERVED - TIME_RESERVED + 4
        );
        // room for a bar of 20: nothing is cut
        let short = "x".repeat(20) + ".thd";
        let mut s = row(Stage::Encode, &short);
        s.fraction = Some(1.0);
        // D = 12 + 24 = 36; 80 - 26 - 36 = 18
        let line = render_rows(80, &[s], false)[0][0].clone();
        assert_eq!(line.chars().filter(|&c| c == '━').count(), 18, "{line}");
        assert!(!line.contains("..."));
        // no line is wider than the console (100% shows no time)
        for w in 80..140 {
            for line in render_rows(w, &[r.clone()], false) {
                assert!(line[0].chars().count() <= w, "{w}: {}", line[0]);
            }
        }
    }

    #[test]
    fn widest_line_fits_the_console() {
        let mut r = row(Stage::Encode, "a.mlp");
        r.fraction = Some(0.0001);
        r.step_secs = 1.0e9;
        r.db = Some(-27);
        for w in 80..200 {
            let line = &render_rows(w, &[r.clone()], false)[0][0];
            assert!(line.chars().count() <= w, "{w}: {line}");
        }
    }

    #[test]
    fn narrow_consoles_get_two_lines() {
        let name = "really_really_really_really_really_really_long_name.thd";
        let mut r = row(Stage::Encode, name);
        r.db = Some(-27);
        r.fraction = Some(0.204);
        r.step_secs = 100.0;
        let lines = render_rows(60, &[r.clone()], false);
        assert_eq!(lines[0].len(), 2);
        assert_eq!(
            lines[0][0],
            "DEE encode: really_really_really_really_reall...thd (-27 dB)"
        );
        assert_eq!(lines[0][0].chars().count(), 60);
        // 60 - 23 = 37: the bar stops at 30
        assert_eq!(
            lines[0][1],
            format!("{}{}20.40% - ~6:30 left", "━".repeat(6), " ".repeat(25))
        );
        for w in [20, 40, 79] {
            let lines = render_rows(w, &[r.clone()], false);
            assert_eq!(lines[0].len(), 2);
            assert!(lines[0][0].chars().count() <= w.max(30), "{w}");
        }
        // at 80 columns it is one line again
        assert_eq!(render_rows(80, &[r], false)[0].len(), 1);
    }

    #[test]
    fn narrow_bar_is_never_below_ten() {
        let mut r = row(Stage::Decode, "a.thd");
        r.fraction = Some(1.0);
        let line = &render_rows(30, &[r], false)[0][1];
        assert_eq!(line.chars().filter(|&c| c == '━').count(), 10);
    }

    #[test]
    fn pulsing_rows_have_no_percentage() {
        let mut r = row(Stage::Measure, "a.thd");
        r.fraction = None;
        r.step_secs = 100.0;
        let line = &render_rows(100, &[r], false)[0][0];
        assert!(line.ends_with(&"━".repeat(30)), "{line}");
    }

    #[test]
    fn colors_of_the_steps() {
        let mut r = row(Stage::Measure, "a.wav");
        r.fraction = Some(0.5);
        let line = &render_rows(100, &[r], true)[0][0];
        assert!(line.contains("\x1b[1;36mDEE\x1b[0m \x1b[38;5;214mmeasure\x1b[0m"));
        assert!(line.contains(&format!("{}│", fg(COLOR_SEPARATOR))));
        assert!(line.contains("\x1b[35m50.00%"));
        let r = row(Stage::Ffmpeg, "a.wav");
        assert!(render_rows(100, &[r], true)[0][0].contains("\x1b[38;5;39mdecode"));
    }
}
