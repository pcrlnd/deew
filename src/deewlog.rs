//! The `.deew-log.html` report of `-kl`: one standalone HTML file per job
//! with the settings, the command lines and the output of every stage.

use regex::Regex;
use std::sync::LazyLock;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const TEMPLATE: &str = include_str!("templates/deew_log.html");
const LOGO: &str = include_str!("templates/deew_logo.svg");

/// Lines kept per stream, the rest is dropped (a marker says so).
const MAX_LINES: usize = 20_000;

static ANSI: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\x1b\[[0-9;?]*[ -/]*[@-~]").expect("valid regex"));
static DEE_BAR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\[[=> ]*\]\s*\d+ %").expect("valid regex"));

pub fn strip_ansi(text: &str) -> String {
    ANSI.replace_all(text, "").into_owned()
}

/// A line of progress output (DEE's bar, ffmpeg's statistics), of which only
/// the last state is kept.
pub fn is_progress(line: &str) -> bool {
    DEE_BAR.is_match(line)
        || (line.contains("time=") && (line.contains("size=") || line.contains("frame=")))
}

/// One output stream of a tool.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stream {
    pub label: String,
    pub lines: Vec<String>,
    dropped: usize,
    /// The last line without its digits, the first line of the run of
    /// similar lines it belongs to, and the length of that run.
    key: String,
    first: String,
    run: usize,
}

impl Stream {
    pub fn new(label: &str) -> Stream {
        Stream {
            label: label.to_string(),
            ..Stream::default()
        }
    }

    /// Add a line: without colors, and a progress line replaces the one
    /// before it.
    pub fn push(&mut self, line: &str) {
        let line = strip_ansi(line);
        let line = line.trim_end();
        if line.is_empty() {
            return;
        }
        if is_progress(line) && self.lines.last().is_some_and(|last| is_progress(last)) {
            *self.lines.last_mut().expect("not empty") = line.to_string();
            self.run = 0;
            return;
        }
        // lines that differ in their numbers only (ffmpeg warns for every
        // packet) are shown once, with their count
        let key: String = line.chars().filter(|c| !c.is_ascii_digit()).collect();
        if key == self.key && self.run > 0 && !self.lines.is_empty() {
            self.run += 1;
            let first = self.first.clone();
            *self.lines.last_mut().expect("not empty") =
                format!("{first}  (+{} similar lines)", self.run - 1);
        } else if self.lines.len() < MAX_LINES {
            self.lines.push(line.to_string());
            self.key = key;
            self.first = line.to_string();
            self.run = 1;
        } else {
            self.dropped += 1;
        }
    }

    fn text(&self) -> String {
        let mut text = self.lines.join("\n");
        if self.dropped > 0 {
            text.push_str(&format!("\n... {} more lines dropped", self.dropped));
        }
        text
    }
}

/// What happened in one stage (ffmpeg, decode, measure, encode).
#[derive(Debug, Clone)]
pub struct StageLog {
    pub name: &'static str,
    /// The long form of the command line(s).
    pub command: String,
    started: SystemTime,
    pub duration: f64,
    pub exit: Option<i32>,
    pub streams: Vec<Stream>,
    pub measured: Option<i64>,
    pub warnings: Vec<String>,
    /// Without `-kl` nothing is recorded.
    pub enabled: bool,
}

impl StageLog {
    /// `commands` are piped into each other when there are several.
    pub fn new(name: &'static str, commands: &[&[String]], enabled: bool) -> StageLog {
        let command = commands
            .iter()
            .map(|args| command_line(args))
            .collect::<Vec<_>>()
            .join(" | ");
        StageLog {
            name,
            command,
            started: SystemTime::now(),
            duration: 0.0,
            exit: None,
            streams: Vec::new(),
            measured: None,
            warnings: Vec::new(),
            enabled,
        }
    }

    /// Index of the stream `label`, created on first use.
    pub fn stream(&mut self, label: &str) -> usize {
        match self.streams.iter().position(|s| s.label == label) {
            Some(i) => i,
            None => {
                self.streams.push(Stream::new(label));
                self.streams.len() - 1
            }
        }
    }

    pub fn line(&mut self, stream: usize, line: &str) {
        if self.enabled {
            self.streams[stream].push(line);
        }
    }

    pub fn finish(&mut self) {
        self.duration = self.started.elapsed().unwrap_or_default().as_secs_f64();
    }
}

/// What a job knows for its report before it runs (`-kl`).
#[derive(Debug, Clone)]
pub struct LogSpec {
    /// Where the report is written.
    pub path: String,
    pub versions: Vec<(String, String)>,
    pub input: Vec<(String, String)>,
    pub settings: Vec<(String, String)>,
    /// The notices that were shown before the encode.
    pub notices: Vec<String>,
}

/// Everything the report shows.
#[derive(Debug, Clone)]
pub struct LogData {
    pub file_name: String,
    pub ok: bool,
    /// Stopped by Ctrl-C (`ok` is false then).
    pub interrupted: bool,
    pub started: SystemTime,
    pub duration: f64,
    pub deew_version: String,
    pub versions: Vec<(String, String)>,
    pub input: Vec<(String, String)>,
    pub settings: Vec<(String, String)>,
    pub stages: Vec<StageLog>,
    pub output: Vec<(String, String)>,
    pub notices: Vec<String>,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

pub fn escape_html(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

pub fn command_line(args: &[String]) -> String {
    args.iter()
        .map(|a| crate::util::shell_quote(a))
        .collect::<Vec<_>>()
        .join(" ")
}

/// `2026-11-02 10:15:30 UTC`
pub fn format_utc(time: SystemTime) -> String {
    let secs = time
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs() as i64;
    let (days, rest) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    // civil from days (Howard Hinnant)
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!(
        "{year:04}-{month:02}-{day:02} {:02}:{:02}:{:02} UTC",
        rest / 3600,
        rest / 60 % 60,
        rest % 60
    )
}

pub fn format_duration(seconds: f64) -> String {
    if seconds < 60.0 {
        format!("{seconds:.1} s")
    } else {
        let total = seconds.round() as u64;
        format!("{} min {:02} s", total / 60, total % 60)
    }
}

pub fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KiB", "MiB", "GiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.2} {} ({bytes} bytes)", UNITS[unit])
    }
}

fn icon(name: &str) -> String {
    format!("<svg class=\"i\" aria-hidden=\"true\"><use href=\"#i-{name}\"/></svg>")
}

fn table(rows: &[(String, String)]) -> String {
    let mut html = String::from("<table>");
    for (key, value) in rows {
        html.push_str(&format!(
            "<tr><td>{}</td><td class=\"mono\">{}</td></tr>",
            escape_html(key),
            escape_html(value)
        ));
    }
    html.push_str("</table>");
    html
}

fn card(title: &str, icon_name: &str, content: &str) -> String {
    format!(
        "<section class=\"card\"><h2>{}{}</h2>{content}</section>",
        icon(icon_name),
        escape_html(title)
    )
}

fn alerts(messages: &[String], error: bool) -> String {
    let (class, icon_name) = if error {
        ("err", "fail")
    } else {
        ("warn", "warn")
    };
    messages
        .iter()
        .map(|m| {
            format!(
                "<div class=\"alert {class}\">{}<span>{}</span></div>",
                icon(icon_name),
                escape_html(m)
            )
        })
        .collect()
}

fn stage_html(index: usize, stage: &StageLog) -> String {
    let exit = match stage.exit {
        Some(0) => "<span class=\"badge ok\">exit 0</span>".to_string(),
        Some(code) => format!("<span class=\"badge fail\">exit {code}</span>"),
        None => "<span class=\"badge fail\">no exit code</span>".to_string(),
    };
    let mut html = format!(
        "<details class=\"stage\" open><summary>{}<span class=\"stage-name\">{}</span>{exit}\
<span class=\"muted\">{}</span><span class=\"muted\">{}</span></summary><div class=\"inner\">",
        icon("down").replace("class=\"i\"", "class=\"i chev\""),
        escape_html(stage.name),
        format_duration(stage.duration),
        escape_html(&format_utc(stage.started_time())),
    );
    html.push_str(&format!(
        "<div class=\"label\">{}command<button class=\"copy\" type=\"button\" data-target=\"cmd{index}\">{}<span class=\"t\">copy</span></button></div>\
<pre id=\"cmd{index}\">{}</pre>",
        icon("term"),
        icon("copy"),
        escape_html(&stage.command)
    ));
    if let Some(db) = stage.measured {
        html.push_str(&format!(
            "<div class=\"label\"><span class=\"badge sec\">measured loudness {db} dB</span></div>"
        ));
    }
    for stream in &stage.streams {
        if stream.lines.is_empty() {
            continue;
        }
        html.push_str(&format!(
            "<div class=\"label\">{}</div><pre class=\"scroll\">{}</pre>",
            escape_html(&stream.label),
            escape_html(&stream.text())
        ));
    }
    html.push_str("</div></details>");
    html
}

impl StageLog {
    fn started_time(&self) -> SystemTime {
        self.started
    }
}

/// The report as a standalone HTML page.
pub fn render(data: &LogData) -> String {
    let badge = if data.ok {
        format!("<span class=\"badge ok\">{}done</span>", icon("ok"))
    } else if data.interrupted {
        format!(
            "<span class=\"badge fail\">{}interrupted</span>",
            icon("fail")
        )
    } else {
        format!("<span class=\"badge fail\">{}failed</span>", icon("fail"))
    };
    let mut body = format!(
        "<header class=\"card\">{LOGO}<div class=\"grow\"><h1>{}</h1><div class=\"sub\">\
<span>{}{}</span><span>{}{}</span><span class=\"mono\">deew {}</span></div></div>{badge}</header>",
        escape_html(&data.file_name),
        icon("cal"),
        escape_html(&format_utc(data.started)),
        icon("clock"),
        format_duration(data.duration),
        escape_html(&data.deew_version),
    );
    body.push_str(&alerts(&data.errors, true));
    body.push_str(&alerts(&data.warnings, false));
    body.push_str(&alerts(&data.notices, false));
    body.push_str("<div class=\"grid\">");
    body.push_str(&card("Input", "audio", &table(&data.input)));
    body.push_str(&card("Settings", "sliders", &table(&data.settings)));
    body.push_str("</div>");
    if !data.stages.is_empty() {
        let stages: String = data
            .stages
            .iter()
            .enumerate()
            .map(|(i, s)| stage_html(i, s))
            .collect();
        body.push_str(&card("Stages", "checks", &stages));
    }
    if !data.output.is_empty() {
        body.push_str(&card("Output", "ok", &table(&data.output)));
    }
    body.push_str(&card("Versions", "pkg", &table(&data.versions)));
    TEMPLATE
        .replace(
            "{{TITLE}}",
            &format!("{} - deew log", escape_html(&data.file_name)),
        )
        .replace("{{VERSION}}", &escape_html(&data.deew_version))
        .replace("{{BODY}}", &body)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> LogData {
        let mut stage = StageLog::new(
            "encode",
            &[&[
                "dee_ddp_encoder".to_string(),
                "-i".into(),
                "a <b>.wav".into(),
            ][..]],
            true,
        );
        let out = stage.stream("stdout");
        stage.line(out, "\x1b[31mred\x1b[0m & <tag>");
        stage.exit = Some(1);
        stage.measured = Some(-27);
        LogData {
            file_name: "a <b> & \"c\".wav".into(),
            ok: false,
            interrupted: false,
            started: UNIX_EPOCH + Duration::from_secs(1_793_614_530),
            duration: 3.5,
            deew_version: "5.0.0".into(),
            versions: vec![("DEE version".into(), "6.7.0".into())],
            input: vec![("Path".into(), "/in/<x>.wav".into())],
            settings: vec![("Format".into(), "DDP".into())],
            stages: vec![stage],
            output: vec![("Path".into(), "/out/a.ec3".into())],
            notices: vec!["a note".into()],
            warnings: vec!["clipping <!>".into()],
            errors: vec!["dee failed: \"x\" <script>".into()],
        }
    }

    #[test]
    fn everything_is_escaped() {
        let html = render(&data());
        assert!(!html.contains("<script>alert") && !html.contains("<b>"));
        assert!(html.contains("a &lt;b&gt; &amp; &quot;c&quot;.wav"));
        assert!(html.contains("/in/&lt;x&gt;.wav"));
        assert!(html.contains("clipping &lt;!&gt;"));
        assert!(html.contains("&lt;script&gt;"));
        // the command and the output, without the color codes
        let quote = if cfg!(windows) { "&quot;" } else { "&#39;" };
        assert!(html.contains(&format!("dee_ddp_encoder -i {quote}a &lt;b&gt;.wav{quote}")));
        assert!(html.contains("red &amp; &lt;tag&gt;"));
        assert!(!html.contains('\x1b'));
        assert_eq!(escape_html("<&>\"'"), "&lt;&amp;&gt;&quot;&#39;");
    }

    #[test]
    fn nothing_is_loaded_from_outside() {
        let html = render(&data());
        for needle in [
            "<link",
            "src=\"http",
            "src='http",
            "@import",
            "url(http",
            "href=\"http",
            "<img",
            "<iframe",
            "xmlns",
        ] {
            assert!(!html.contains(needle), "{needle}");
        }
        assert!(!html.contains("http://") && !html.contains("https://"));
        assert!(html.contains("prefers-color-scheme:dark"));
        assert!(html.starts_with("<!DOCTYPE html>"));
    }

    #[test]
    fn sections_have_their_own_icons() {
        let html = render(&data());
        for (title, icon) in [
            ("Input", "audio"),
            ("Settings", "sliders"),
            ("Stages", "checks"),
            ("Output", "ok"),
            ("Versions", "pkg"),
        ] {
            assert!(
                html.contains(&format!("<use href=\"#i-{icon}\"/></svg>{title}</h2>")),
                "{title}"
            );
            assert!(html.contains(&format!("<symbol id=\"i-{icon}\"")), "{icon}");
        }
    }

    #[test]
    fn copy_button_falls_back_and_works_without_js() {
        let html = render(&data());
        // hidden until the script ran, and big enough for a finger
        assert!(html.contains("button.copy{display:none"));
        assert!(html.contains("html.js button.copy{display:inline-flex}"));
        assert!(html.contains("min-height:2rem"));
        assert!(html.contains("<span class=\"t\">copy</span>"));
        // secure context API, then execCommand, then selecting the text
        assert!(html.contains("window.isSecureContext&&navigator.clipboard"));
        assert!(html.contains("execCommand(\"copy\")"));
        assert!(html.contains("createRange"));
        assert!(html.contains("select + copy") && html.contains("\"copied\""));
        // the plain text of the command, not HTML
        assert!(html.contains("pre.textContent"));
    }

    #[test]
    fn content_of_the_page() {
        let html = render(&data());
        assert!(html.contains("badge fail"));
        assert!(html.contains("exit 1"));
        assert!(html.contains("measured loudness -27 dB"));
        assert!(html.contains("2026-11-02 10:15:30 UTC"));
        let mut ok = data();
        ok.ok = true;
        ok.errors.clear();
        let html = render(&ok);
        assert!(html.contains(
            "badge ok\"><svg class=\"i\" aria-hidden=\"true\"><use href=\"#i-ok\"/></svg>done"
        ));
        assert!(!html.contains("alert err"));
        let mut stopped = data();
        stopped.interrupted = true;
        stopped.errors = vec!["interrupted".into()];
        let html = render(&stopped);
        assert!(html.contains("badge fail\"><svg class=\"i\" aria-hidden=\"true\"><use href=\"#i-fail\"/></svg>interrupted"));
    }

    #[test]
    fn progress_collapses_to_the_last_state() {
        let mut s = Stream::default();
        for line in [
            "Step 1/2 : Loudness Measurement",
            "[>          ] 0 %",
            "[=====>     ] 50 %",
            "[==========>] 100 %",
            "measured_loudness=-23.4",
            "size=  256kB time=00:00:01.00 bitrate=1",
            "size=  512kB time=00:00:02.00 bitrate=1",
            "",
        ] {
            s.push(line);
        }
        assert_eq!(
            s.lines,
            [
                "Step 1/2 : Loudness Measurement",
                "[==========>] 100 %",
                "measured_loudness=-23.4",
                "size=  512kB time=00:00:02.00 bitrate=1"
            ]
        );
    }

    #[test]
    fn similar_lines_are_counted() {
        let mut s = Stream::default();
        for i in 0..5 {
            s.push(&format!(
                "[truehd @ 0x5c] non monotonically increasing dts: {i} >= {i}"
            ));
        }
        s.push("something else");
        s.push("something else");
        assert_eq!(
            s.lines,
            [
                "[truehd @ 0x5c] non monotonically increasing dts: 0 >= 0  (+4 similar lines)",
                "something else  (+1 similar lines)"
            ]
        );
    }

    #[test]
    fn long_output_is_cut() {
        let mut s = Stream::default();
        for i in 0..MAX_LINES + 5 {
            s.push(&format!(
                "line {}",
                "ab".repeat(i % 7 + 1) + &"x".repeat(i % 11)
            ));
        }
        assert_eq!(s.lines.len(), MAX_LINES);
        assert!(s.text().ends_with("... 5 more lines dropped"));
    }

    #[test]
    fn disabled_logs_record_nothing() {
        let mut stage = StageLog::new("ffmpeg", &[&["ffmpeg".to_string()][..]], false);
        let i = stage.stream("stderr");
        stage.line(i, "hello");
        assert!(stage.streams[i].lines.is_empty());
    }

    #[test]
    fn command_lines_are_quoted() {
        let args: Vec<String> = ["ffmpeg", "-i", "a b.wav", "x\"y", "-map", "0:a:0", ""]
            .map(String::from)
            .to_vec();
        assert_eq!(
            command_line(&args),
            if cfg!(windows) {
                "ffmpeg -i \"a b.wav\" \"x\\\"y\" -map 0:a:0 \"\""
            } else {
                "ffmpeg -i 'a b.wav' 'x\"y' -map 0:a:0 ''"
            }
        );
        // Windows paths keep their single backslashes (cmd style only)
        #[cfg(windows)]
        {
            let win: Vec<String> = [
                "X:\\deew_tools\\windows\\truehdd.exe",
                "decode",
                "--output-path",
                "X:\\deew_tools\\windows\\temp\\vecB_obj",
                "X:\\deew_test\\test\\vecB_obj.mlp",
                "X:\\My Music\\a.thd",
                "[a:0]pan=5.1|c0=c0",
                "a&b",
            ]
            .map(String::from)
            .to_vec();
            assert_eq!(
                command_line(&win),
                "X:\\deew_tools\\windows\\truehdd.exe decode --output-path X:\\deew_tools\\windows\\temp\\vecB_obj X:\\deew_test\\test\\vecB_obj.mlp \"X:\\My Music\\a.thd\" \"[a:0]pan=5.1|c0=c0\" \"a&b\""
            );
        }
        let a = vec!["ffmpeg".to_string(), "-".into()];
        let b = vec!["truehdd".to_string(), "decode".into()];
        assert_eq!(
            StageLog::new("decode", &[&a, &b], true).command,
            "ffmpeg - | truehdd decode"
        );
    }

    #[test]
    fn dates_and_sizes() {
        assert_eq!(format_utc(UNIX_EPOCH), "1970-01-01 00:00:00 UTC");
        assert_eq!(
            format_utc(UNIX_EPOCH + Duration::from_secs(951_782_400 + 86_399)),
            "2000-02-29 23:59:59 UTC"
        );
        assert_eq!(
            format_utc(UNIX_EPOCH + Duration::from_secs(1_793_614_530)),
            "2026-11-02 10:15:30 UTC"
        );
        assert_eq!(format_duration(3.04), "3.0 s");
        assert_eq!(format_duration(125.0), "2 min 05 s");
        assert_eq!(format_size(512), "512 B");
        assert_eq!(format_size(1_282_048), "1.22 MiB (1282048 bytes)");
    }

    #[test]
    fn ansi_is_stripped() {
        assert_eq!(strip_ansi("\x1b[1;36mdeew\x1b[0m \x1b[?25l!"), "deew !");
    }
}
