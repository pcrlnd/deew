//! A block of live lines at the bottom of stdout, redrawn in place.
//!
//! Every line is drawn one column narrower than the terminal and ends with
//! a real newline, so a terminal that re-wraps lines on resize wraps each
//! line on its own. Before a redraw the cursor goes up by the rows the old
//! lines take at the *current* width.
//!
//! A redraw overwrites the old rows in place and clears to the end of the
//! screen last. Stdout is a `LineWriter`, so the terminal can see the text up
//! to the last newline before the rest; clearing first would let it show a
//! blank last line in between (a flicker).
//!
//! Each old row is erased whole (`\x1b[2K`) before it is written. A row the
//! terminal wrapped on a narrowing resize is marked as continuing on the
//! next row, and tmux keeps that mark when the row is only overwritten, so
//! a later widening resize would join it with the row below and the frame
//! would take fewer rows than counted (stale rows above it). Erasing the
//! whole row drops the mark.

use std::io::{IsTerminal, Write};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

/// The lines are redrawn this often (8 Hz).
const TICK: Duration = Duration::from_millis(125);

/// The lines to show for a drawable width.
type Frame = Box<dyn FnMut(usize) -> Vec<String> + Send>;

/// What is on screen.
struct State {
    frame: Frame,
    /// Display widths of the lines on screen.
    widths: Vec<usize>,
    /// The last drawn lines and the width they were drawn at.
    last: Option<(Vec<String>, usize)>,
}

struct Shared {
    hidden: bool,
    state: Mutex<State>,
    /// Set to stop the ticker.
    stop: Mutex<bool>,
    wake: Condvar,
}

/// The width lines are drawn at: one column less than the terminal, so the
/// cursor never sits in the pending-wrap state at the right margin.
fn drawable_width(terminal: usize) -> usize {
    terminal.saturating_sub(1).max(1)
}

/// The rows the lines drawn earlier take now.
///
/// This assumes a terminal that re-wraps lines on resize (Windows Terminal,
/// conhost, xterm.js, VTE, kitty, iTerm2, WezTerm, Alacritty, tmux). On one
/// that does not (xterm) a narrowing resize erases a few lines above the
/// frame.
fn rows_on_screen(widths: &[usize], terminal: usize) -> usize {
    let terminal = terminal.max(1);
    widths.iter().map(|&w| w.div_ceil(terminal).max(1)).sum()
}

/// The bytes that redraw the frame over the old one, and the display widths
/// of the written lines. The cursor stays at the end of the last line.
///
/// The old rows are overwritten, never cleared first: the cursor goes back
/// to the top of the old frame, `before` (text printed above the frame, not
/// cut) and the lines are written, each erasing its old row first with
/// `\x1b[2K` (when there is an old frame) and clearing the leftover tail of a
/// wrapped `before` part with `\x1b[K`, and `\x1b[J` comes last to clear what
/// is left below. Stdout is
/// a `LineWriter`, so the terminal may get the text up to the last newline
/// before the rest. With the clear last, it shows either the old or the new
/// last line in between, never a blank row. Returns an empty string when
/// there is nothing to do.
fn frame_bytes(
    prev_rows: usize,
    before: Option<&str>,
    lines: &[String],
    width: usize,
) -> (String, Vec<usize>) {
    if prev_rows == 0 && before.is_none() && lines.is_empty() {
        return (String::new(), Vec::new());
    }
    let mut out = String::new();
    if prev_rows > 1 {
        out.push_str(&format!("\x1b[{}A", prev_rows - 1));
    }
    if prev_rows > 0 {
        out.push('\r');
    }
    // Erase the old rows whole (see the module docs); the first frame keeps
    // whatever is on the cursor's row.
    let erase = if prev_rows > 0 { "\x1b[2K" } else { "" };
    if let Some(before) = before {
        for part in before.split('\n') {
            out.push_str(erase);
            out.push_str(part);
            out.push_str("\x1b[K\n");
        }
    }
    let mut widths = Vec::with_capacity(lines.len());
    for (i, line) in lines.iter().enumerate() {
        if i > 0 {
            out.push_str("\x1b[K\n");
        }
        out.push_str(erase);
        let cut = console::truncate_str(line, width, "");
        out.push_str(&cut);
        if cut.contains('\x1b') {
            out.push_str("\x1b[0m");
        }
        widths.push(console::measure_text_width(&cut));
    }
    out.push_str("\x1b[J");
    (out, widths)
}

fn write_out(text: &str) {
    let mut out = std::io::stdout().lock();
    let _ = out.write_all(text.as_bytes());
    let _ = out.flush();
}

impl Shared {
    /// The current frame, cut to what fits above the cursor.
    fn lines(state: &mut State, width: usize) -> Vec<String> {
        let mut lines = (state.frame)(width);
        if let Some((rows, _)) = console::Term::stdout().size_checked() {
            lines.truncate(usize::from(rows).saturating_sub(1).max(1));
        }
        lines
    }

    /// Draw the frame over the old one; `before` is written above it.
    fn draw(&self, before: Option<&str>) {
        if self.hidden {
            return;
        }
        let mut state = self.state.lock().expect("live state");
        let terminal = crate::term::console_width();
        let width = drawable_width(terminal);
        let lines = Self::lines(&mut state, width);
        let prev_rows = rows_on_screen(&state.widths, terminal);
        if before.is_none()
            && state
                .last
                .as_ref()
                .is_some_and(|(l, w)| *l == lines && *w == width)
        {
            return;
        }
        let (text, widths) = frame_bytes(prev_rows, before, &lines, width);
        state.widths = widths;
        state.last = Some((lines, width));
        write_out(&text);
    }
}

/// A block of live lines at the bottom of stdout.
pub struct Live {
    shared: Arc<Shared>,
    ticker: Option<JoinHandle<()>>,
}

impl Live {
    /// `frame` returns the lines to show (they may contain colour codes)
    /// for a drawable width. Nothing is drawn when stdout is not a terminal.
    pub fn new(frame: impl FnMut(usize) -> Vec<String> + Send + 'static) -> Live {
        let terminal = std::io::stdout().is_terminal();
        // Escape codes are needed for the cursor even without colour.
        #[cfg(windows)]
        let terminal = terminal && console::Term::stdout().features().colors_supported();
        let hidden = !terminal;
        let shared = Arc::new(Shared {
            hidden,
            state: Mutex::new(State {
                frame: Box::new(frame),
                widths: Vec::new(),
                last: None,
            }),
            stop: Mutex::new(false),
            wake: Condvar::new(),
        });
        let ticker = (!hidden).then(|| {
            let shared = shared.clone();
            std::thread::spawn(move || {
                loop {
                    {
                        let stop = shared.stop.lock().expect("live stop");
                        let (stop, _) = shared
                            .wake
                            .wait_timeout_while(stop, TICK, |stop| !*stop)
                            .expect("live stop");
                        if *stop {
                            break;
                        }
                    }
                    shared.draw(None);
                }
            })
        });
        Live { shared, ticker }
    }

    /// Whether nothing is drawn (stdout is not a terminal).
    pub fn is_hidden(&self) -> bool {
        self.shared.hidden
    }

    /// Draw the frame now.
    pub fn redraw(&self) {
        self.shared.draw(None);
    }

    /// Print a line above the frame.
    pub fn println(&self, text: &str) {
        print_above(&self.shared, text);
    }

    /// A function that prints like [`Live::println`], for other threads.
    pub fn printer(&self) -> impl Fn(&str) + Send + 'static {
        let shared = self.shared.clone();
        move |text| print_above(&shared, text)
    }

    fn stop_ticker(&mut self) {
        if let Some(ticker) = self.ticker.take() {
            *self.shared.stop.lock().expect("live stop") = true;
            self.shared.wake.notify_all();
            let _ = ticker.join();
        }
    }

    /// Stop the ticker. With `keep` the final frame stays on screen and the
    /// cursor goes to the line below it; otherwise the frame is erased.
    pub fn finish(mut self, keep: bool) {
        self.stop_ticker();
        if self.shared.hidden {
            return;
        }
        if keep {
            self.shared.draw(None);
            let mut state = self.shared.state.lock().expect("live state");
            if !state.widths.is_empty() {
                // the rows are on screen for good: nothing to erase later
                state.widths.clear();
                state.last = None;
                write_out("\n");
            }
        } else {
            let mut state = self.shared.state.lock().expect("live state");
            let rows = rows_on_screen(&state.widths, crate::term::console_width());
            let (text, widths) = frame_bytes(rows, None, &[], 1);
            state.widths = widths;
            state.last = None;
            write_out(&text);
        }
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        self.stop_ticker();
    }
}

fn print_above(shared: &Shared, text: &str) {
    if shared.hidden {
        println!("{text}");
    } else {
        shared.draw(Some(text));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_follow_the_terminal_width() {
        assert_eq!(rows_on_screen(&[50, 80], 100), 2);
        assert_eq!(rows_on_screen(&[99, 99], 60), 4);
        assert_eq!(rows_on_screen(&[0], 80), 1);
        assert_eq!(rows_on_screen(&[], 80), 0);
        assert!(rows_on_screen(&[120], 0) > 0);
    }

    fn v(l: &[&str]) -> Vec<String> {
        l.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn first_frame_has_no_escape_before_the_line() {
        let (text, widths) = frame_bytes(0, None, &v(&["a", "b"]), 10);
        assert_eq!(text, "a\x1b[K\nb\x1b[J");
        assert_eq!(widths, vec![1, 1]);
    }

    #[test]
    fn one_row_returns_without_moving_up() {
        let (text, _) = frame_bytes(1, None, &v(&["a", "b"]), 10);
        assert_eq!(text, "\r\x1b[2Ka\x1b[K\n\x1b[2Kb\x1b[J");
    }

    #[test]
    fn several_rows_move_up_first() {
        let (text, _) = frame_bytes(3, None, &v(&["a"]), 10);
        assert_eq!(text, "\x1b[2A\r\x1b[2Ka\x1b[J");
    }

    #[test]
    fn no_lines_only_erase() {
        let (text, widths) = frame_bytes(2, None, &[], 10);
        assert_eq!(text, "\x1b[1A\r\x1b[J");
        assert!(widths.is_empty());
    }

    #[test]
    fn nothing_to_do_is_empty() {
        let (text, widths) = frame_bytes(0, None, &[], 10);
        assert_eq!(text, "");
        assert!(widths.is_empty());
    }

    #[test]
    fn text_above_is_written_over_the_old_rows() {
        let (text, _) = frame_bytes(2, Some("x\ny"), &v(&["a"]), 10);
        assert_eq!(
            text,
            "\x1b[1A\r\x1b[2Kx\x1b[K\n\x1b[2Ky\x1b[K\n\x1b[2Ka\x1b[J"
        );
        let (text, _) = frame_bytes(1, Some("x"), &[], 10);
        assert_eq!(text, "\r\x1b[2Kx\x1b[K\n\x1b[J");
    }

    #[test]
    fn plain_lines_are_cut_to_the_width() {
        let (text, widths) = frame_bytes(0, None, &v(&["abcdefgh"]), 5);
        assert_eq!(text, "abcde\x1b[J");
        assert_eq!(widths, vec![5]);
    }

    #[test]
    fn coloured_lines_are_cut_by_display_width() {
        let lines = v(&["\x1b[31mabcdefgh", "b"]);
        let (text, widths) = frame_bytes(0, None, &lines, 4);
        assert_eq!(text, "\x1b[31mabcd\x1b[0m\x1b[K\nb\x1b[J");
        assert_eq!(widths, vec![4, 1]);
        let (text, _) = frame_bytes(0, None, &lines[..1], 4);
        assert_eq!(text, "\x1b[31mabcd\x1b[0m\x1b[J");
    }

    #[test]
    fn lines_are_one_column_narrower_than_the_terminal() {
        assert_eq!(drawable_width(80), 79);
        assert_eq!(drawable_width(1), 1);
        assert_eq!(drawable_width(0), 1);
    }
}
