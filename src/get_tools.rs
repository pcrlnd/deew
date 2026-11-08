//! `-gt`: download the latest ffmpeg, ffprobe, truehdd and dee_shim next to
//! the deew executable.

use crate::error::escape;
use crate::interrupt;
use crate::markup::color_enabled;
use crate::rprintln;
use crate::term::console_width;
use crate::ui::{bar_width, download_bar_text, download_percent_text};
use indicatif::{MultiProgress, ProgressBar, ProgressDrawTarget, ProgressState, ProgressStyle};
use sha2::{Digest, Sha256};
use std::fmt::Write as _;
use std::fs::File;
use std::io::{self, BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use ureq::ResponseExt;

const GITHUB_API: &str = "https://api.github.com/repos";
const BTBN_REPO: &str = "BtbN/FFmpeg-Builds";
const TRUEHDD_REPO: &str = "truehdd/truehdd";
const DEE_SHIM_REPO: &str = "truehdd/dee-shim";
const MACOS_FFMPEG: &str = "https://ffmpeg.martin-riedl.de/redirect/latest/macos";

/// Width of the bar, narrower than the encoding bar.
const BAR_CELLS: usize = 20;
/// Width of the tool name column.
const NAME_WIDTH: usize = 8;
/// Width of the status column (`downloading...`).
const STATUS_WIDTH: usize = 14;
/// Positions of a bar are hundredths of a percent.
const SCALE: u64 = 10_000;
/// Share of the bar that is the download, the rest is the unpacking.
const DOWNLOAD_SHARE: f64 = 0.9;
/// A transfer that gets no data for this long fails.
const IDLE_TIMEOUT: Duration = Duration::from_secs(30);
const CHUNK: usize = 64 * 1024;

// --- platform and asset selection ---

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Os {
    Linux,
    Windows,
    Darwin,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Arch {
    X86_64,
    Aarch64,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Platform {
    pub os: Os,
    pub arch: Arch,
}

impl Platform {
    fn current() -> Platform {
        let os = if cfg!(target_os = "linux") {
            Os::Linux
        } else if cfg!(target_os = "windows") {
            Os::Windows
        } else if cfg!(target_os = "macos") {
            Os::Darwin
        } else {
            Os::Other
        };
        let arch = if cfg!(target_arch = "x86_64") {
            Arch::X86_64
        } else if cfg!(target_arch = "aarch64") {
            Arch::Aarch64
        } else {
            Arch::Other
        };
        Platform { os, arch }
    }

    fn describe(self) -> String {
        let os = match self.os {
            Os::Linux => "Linux",
            Os::Windows => "Windows",
            Os::Darwin => "macOS",
            Os::Other => "this OS",
        };
        let arch = match self.arch {
            Arch::X86_64 => "x86_64",
            Arch::Aarch64 => "aarch64",
            Arch::Other => "this CPU",
        };
        format!("{os} {arch}")
    }
}

/// `ffmpeg-n9.0-latest-linux64-gpl-9.0.tar.xz` style platform tag.
fn btbn_tag(platform: Platform) -> Option<&'static str> {
    match (platform.os, platform.arch) {
        (Os::Linux, Arch::X86_64) => Some("linux64"),
        (Os::Windows, Arch::X86_64) => Some("win64"),
        _ => None,
    }
}

/// The version of `ffmpeg-nX.Y-latest-<tag>-gpl-X.Y.<tar.xz|zip>`, if `name`
/// is exactly such a stable build (no master, lgpl, shared or other arch).
fn btbn_version(name: &str, tag: &str) -> Option<Vec<u64>> {
    let rest = name.strip_prefix("ffmpeg-n")?;
    let (version, rest) = rest.split_once("-latest-")?;
    let rest = rest.strip_prefix(tag)?.strip_prefix("-gpl-")?;
    let ext = rest.strip_prefix(version)?.strip_prefix('.')?;
    if ext != "tar.xz" && ext != "zip" {
        return None;
    }
    let parts: Option<Vec<u64>> = version.split('.').map(|p| p.parse().ok()).collect();
    parts.filter(|p| p.len() >= 2)
}

/// The stable GPL static build with the highest version.
fn pick_btbn<'a>(names: &[&'a str], tag: &str) -> Option<&'a str> {
    names
        .iter()
        .filter_map(|&n| btbn_version(n, tag).map(|v| (v, n)))
        .max_by(|a, b| a.0.cmp(&b.0))
        .map(|(_, n)| n)
}

/// Suffix of the truehdd asset (`truehdd-0.6.2-<suffix>`).
fn truehdd_suffix(platform: Platform) -> Option<&'static str> {
    match (platform.os, platform.arch) {
        (Os::Linux, Arch::X86_64) => Some("-x86_64-unknown-linux-musl.tar.gz"),
        (Os::Windows, Arch::X86_64) => Some("-x86_64-pc-windows-msvc.zip"),
        (Os::Darwin, Arch::X86_64 | Arch::Aarch64) => Some("-universal-macOS.zip"),
        _ => None,
    }
}

/// Suffix of the dee_shim asset. Only x86_64 is built, on Apple silicon it
/// runs under Rosetta.
fn dee_shim_suffix(platform: Platform) -> Option<&'static str> {
    match (platform.os, platform.arch) {
        (Os::Linux, Arch::X86_64) => Some("-linux-x86_64.tar.gz"),
        (Os::Windows, Arch::X86_64) => Some("-windows-x86_64.zip"),
        (Os::Darwin, Arch::X86_64 | Arch::Aarch64) => Some("-macos-x86_64.tar.gz"),
        _ => None,
    }
}

fn pick_suffix<'a>(names: &[&'a str], suffix: &str) -> Option<&'a str> {
    names.iter().copied().find(|n| n.ends_with(suffix))
}

/// The macOS ffmpeg/ffprobe redirect URL of the architecture deew runs on.
fn macos_url(platform: Platform, tool: &str) -> Option<String> {
    let arch = match platform.arch {
        Arch::Aarch64 => "arm64",
        Arch::X86_64 => "amd64",
        Arch::Other => return None,
    };
    Some(format!("{MACOS_FFMPEG}/{arch}/release/{tool}.zip"))
}

// --- checksums ---

fn is_sha256(s: &str) -> bool {
    s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The SHA256 out of the text of a `.sha256` file: `<hex>  name`, a bare
/// `<hex>` or a list of those (then the line of `name`).
fn parse_sha256(text: &str, name: &str) -> Option<String> {
    for line in text.lines() {
        let mut tokens = line.split_whitespace();
        let Some(hash) = tokens.next().filter(|h| is_sha256(h)) else {
            continue;
        };
        match tokens.next() {
            None => return Some(hash.to_ascii_lowercase()),
            Some(file) => {
                let file = file.trim_start_matches('*').trim_start_matches("./");
                if file == name {
                    return Some(hash.to_ascii_lowercase());
                }
            }
        }
    }
    None
}

/// The last path segment of a URL, without any `?query` or `#fragment`.
fn url_file_name(url: &str) -> &str {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    path.rsplit('/').next().unwrap_or(path)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().fold(String::new(), |mut s, b| {
        let _ = write!(s, "{b:02x}");
        s
    })
}

// --- tools, rows and progress ---

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    Downloading,
    Unpacking,
    Done,
    Failed,
}

impl Status {
    fn text(self) -> &'static str {
        match self {
            Status::Downloading => "downloading...",
            Status::Unpacking => "unpacking...",
            Status::Done => "done",
            Status::Failed => "failed",
        }
    }
}

/// Position of a bar (0 to 1) while `done` of `total` bytes are processed:
/// the download is the first 90 %, the unpacking the rest. `None` while the
/// total is unknown.
pub fn overall_fraction(status: Status, done: u64, total: Option<u64>) -> Option<f64> {
    let part = |total: u64| {
        if total == 0 {
            1.0
        } else {
            (done as f64 / total as f64).clamp(0.0, 1.0)
        }
    };
    match status {
        Status::Downloading => total.map(|t| part(t) * DOWNLOAD_SHARE),
        Status::Unpacking => total.map(|t| DOWNLOAD_SHARE + part(t) * (1.0 - DOWNLOAD_SHARE)),
        Status::Done => Some(1.0),
        Status::Failed => total.map(|t| part(t) * DOWNLOAD_SHARE),
    }
}

/// `truehdd  | unpacking...  `
fn label(name: &str, status: Status) -> String {
    format!(
        "{name:<NAME_WIDTH$} | {:<STATUS_WIDTH$}",
        status.text(),
        NAME_WIDTH = NAME_WIDTH,
        STATUS_WIDTH = STATUS_WIDTH
    )
}

const LABEL_WIDTH: usize = NAME_WIDTH + 3 + STATUS_WIDTH;

fn bar_cells() -> usize {
    bar_width(console_width(), LABEL_WIDTH).min(BAR_CELLS)
}

/// A whole row as printed without a terminal (and by the tests).
fn row_text(
    name: &str,
    status: Status,
    fraction: Option<f64>,
    color: bool,
    cells: usize,
) -> String {
    format!(
        "[ {} ] {} {}",
        label(name, status),
        download_bar_text(fraction, color, 0.0, cells),
        download_percent_text(fraction, color)
    )
}

fn style() -> ProgressStyle {
    let color = color_enabled();
    let fraction = |s: &ProgressState| s.len().map(|len| s.pos() as f64 / len as f64);
    ProgressStyle::with_template("[ {msg} ] {deewbar} {deewpct}")
        .expect("valid template")
        .with_key(
            "deewbar",
            move |s: &ProgressState, w: &mut dyn std::fmt::Write| {
                let _ = w.write_str(&download_bar_text(
                    fraction(s),
                    color,
                    s.elapsed().as_secs_f64(),
                    bar_cells(),
                ));
            },
        )
        .with_key(
            "deewpct",
            move |s: &ProgressState, w: &mut dyn std::fmt::Write| {
                let _ = w.write_str(&download_percent_text(fraction(s), color));
            },
        )
}

/// One tool and its progress line.
struct Row {
    name: &'static str,
    file: String,
    pb: ProgressBar,
    state: Mutex<(Status, Option<f64>)>,
}

impl Row {
    fn set(&self, status: Status, fraction: Option<f64>) {
        // only a finished tool shows 100 %
        let fraction = fraction.map(|f| {
            if status == Status::Done {
                f
            } else {
                f.min(0.9999)
            }
        });
        *self.state.lock().expect("row state") = (status, fraction);
        self.pb.set_message(label(self.name, status));
        match fraction {
            Some(f) => {
                if self.pb.length().is_none() {
                    self.pb.set_length(SCALE);
                }
                self.pb
                    .set_position((f.clamp(0.0, 1.0) * SCALE as f64).round() as u64);
            }
            None => self.pb.unset_length(),
        }
    }

    fn progress(&self, status: Status, done: u64, total: Option<u64>) {
        self.set(status, overall_fraction(status, done, total));
    }

    fn finish(&self, ok: bool) {
        if ok {
            self.set(Status::Done, Some(1.0));
        } else {
            let fraction = self.state.lock().expect("row state").1;
            self.set(Status::Failed, fraction.or(Some(0.0)));
        }
        self.pb.disable_steady_tick();
        self.pb.abandon();
    }
}

// --- files ---

/// A file that is removed when dropped, unless it was renamed into place.
struct TempFile {
    path: PathBuf,
}

impl TempFile {
    fn new(dir: &Path, tag: &str, ext: &str) -> TempFile {
        TempFile {
            path: dir.join(format!(".deew-gt-{}-{tag}.{ext}", std::process::id())),
        }
    }
}

impl Drop for TempFile {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

fn check_interrupt() -> Result<(), String> {
    if interrupt::is_interrupted() {
        Err("interrupted".to_string())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn make_executable(path: &Path) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> io::Result<()> {
    Ok(())
}

// --- download ---

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        .accept_encoding("identity")
        .build()
        .into()
}

fn get(agent: &ureq::Agent, url: &str) -> Result<ureq::http::Response<ureq::Body>, String> {
    agent
        .get(url)
        .header("User-Agent", format!("deew/{}", env!("CARGO_PKG_VERSION")))
        .header("Accept", "application/vnd.github+json, */*")
        .call()
        .map_err(|e| e.to_string())
}

fn get_text(agent: &ureq::Agent, url: &str) -> Result<String, String> {
    get(agent, url)?
        .body_mut()
        .read_to_string()
        .map_err(|e| e.to_string())
}

#[derive(Debug, Clone)]
struct Asset {
    name: String,
    url: String,
    digest: Option<String>,
}

fn release_assets(agent: &ureq::Agent, repo: &str) -> Result<Vec<Asset>, String> {
    let text = get_text(agent, &format!("{GITHUB_API}/{repo}/releases/latest"))
        .map_err(|e| format!("cannot read the latest release of {repo}: {e}"))?;
    let json: serde_json::Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    let assets = json
        .get("assets")
        .and_then(|a| a.as_array())
        .ok_or_else(|| format!("the latest release of {repo} has no assets"))?;
    let field =
        |a: &serde_json::Value, key: &str| a.get(key).and_then(|v| v.as_str()).map(str::to_string);
    Ok(assets
        .iter()
        .filter_map(|a| {
            Some(Asset {
                name: field(a, "name")?,
                url: field(a, "browser_download_url")?,
                digest: field(a, "digest"),
            })
        })
        .collect())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Archive {
    TarGz,
    TarXz,
    Zip,
}

impl Archive {
    fn of(name: &str) -> Option<Archive> {
        if name.ends_with(".tar.gz") {
            Some(Archive::TarGz)
        } else if name.ends_with(".tar.xz") {
            Some(Archive::TarXz)
        } else if name.ends_with(".zip") {
            Some(Archive::Zip)
        } else {
            None
        }
    }
}

/// Where the expected SHA256 of an archive comes from.
enum Checksum {
    None,
    Known(String),
    /// A text file with the hash.
    Url {
        url: String,
        name: String,
    },
    /// `<final URL after the redirects>.sha256`
    NextToFinalUrl,
}

struct Plan {
    url: String,
    archive: Archive,
    checksum: Checksum,
}

/// The plan for an asset of a GitHub release: a `.sha256` file next to it,
/// else the checksums file of the release, else the digest GitHub computed.
fn plan_for_asset(assets: &[Asset], asset: &Asset, list: Option<&str>) -> Result<Plan, String> {
    let archive =
        Archive::of(&asset.name).ok_or_else(|| format!("unknown archive type: {}", asset.name))?;
    let find = |name: &str| assets.iter().find(|a| a.name == name);
    let checksum = if let Some(side) = find(&format!("{}.sha256", asset.name)) {
        Checksum::Url {
            url: side.url.clone(),
            name: asset.name.clone(),
        }
    } else if let Some(list) = list.and_then(find) {
        Checksum::Url {
            url: list.url.clone(),
            name: asset.name.clone(),
        }
    } else if let Some(hash) = asset
        .digest
        .as_deref()
        .and_then(|d| d.strip_prefix("sha256:"))
        .filter(|h| is_sha256(h))
    {
        Checksum::Known(hash.to_ascii_lowercase())
    } else {
        Checksum::None
    };
    Ok(Plan {
        url: asset.url.clone(),
        archive,
        checksum,
    })
}

/// A download that was started: its body and the final URL.
struct Transfer {
    body: ureq::Body,
    total: Option<u64>,
    final_url: String,
}

fn start(agent: &ureq::Agent, url: &str) -> Result<Transfer, String> {
    let response = get(agent, url)?;
    let final_url = response.get_uri().to_string();
    let total = response.body().content_length();
    Ok(Transfer {
        body: response.into_parts().1,
        total,
        final_url,
    })
}

/// Read the body in a thread so a stalled connection can be given up on.
fn spawn_reader(body: ureq::Body) -> mpsc::Receiver<io::Result<Vec<u8>>> {
    let (tx, rx) = mpsc::sync_channel(8);
    std::thread::spawn(move || {
        let mut reader = body.into_reader();
        loop {
            let mut buf = vec![0u8; CHUNK];
            match reader.read(&mut buf) {
                Ok(0) => return,
                Ok(n) => {
                    buf.truncate(n);
                    if tx.send(Ok(buf)).is_err() {
                        return;
                    }
                }
                Err(e) => {
                    let _ = tx.send(Err(e));
                    return;
                }
            }
        }
    });
    rx
}

/// Download the plan's archive into `dest`, verify its SHA256.
fn fetch_archive(
    agent: &ureq::Agent,
    plan: &Plan,
    dest: &Path,
    progress: &dyn Fn(u64, Option<u64>),
) -> Result<(), String> {
    let transfer = start(agent, &plan.url)?;
    let expected = match &plan.checksum {
        Checksum::None => None,
        Checksum::Known(hash) => Some(hash.clone()),
        Checksum::Url { url, name } => {
            let text =
                get_text(agent, url).map_err(|e| format!("cannot read the checksum: {e}"))?;
            Some(
                parse_sha256(&text, name)
                    .ok_or("no SHA256 for the archive in the checksum file")?,
            )
        }
        Checksum::NextToFinalUrl => {
            let url = format!("{}.sha256", transfer.final_url);
            let text =
                get_text(agent, &url).map_err(|e| format!("cannot read the checksum: {e}"))?;
            let name = url_file_name(&transfer.final_url);
            Some(parse_sha256(&text, name).ok_or("the checksum file has no SHA256")?)
        }
    };
    let total = transfer.total;
    progress(0, total);
    let rx = spawn_reader(transfer.body);
    let mut out = BufWriter::with_capacity(1 << 20, File::create(dest).map_err(|e| e.to_string())?);
    let mut hasher = Sha256::new();
    let mut received = 0u64;
    let mut idle = std::time::Instant::now();
    loop {
        check_interrupt()?;
        match rx.recv_timeout(Duration::from_millis(200)) {
            Ok(Ok(chunk)) => {
                hasher.update(&chunk);
                out.write_all(&chunk)
                    .map_err(|e| format!("cannot write {}: {e}", dest.display()))?;
                received += chunk.len() as u64;
                progress(received, total);
                idle = std::time::Instant::now();
            }
            Ok(Err(e)) => return Err(format!("download failed: {e}")),
            Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => {
                if idle.elapsed() >= IDLE_TIMEOUT {
                    return Err("download stalled (no data for 30 seconds)".to_string());
                }
            }
        }
    }
    out.flush()
        .map_err(|e| format!("cannot write {}: {e}", dest.display()))?;
    if let Some(total) = total
        && received != total
    {
        return Err(format!("download incomplete ({received} of {total} bytes)"));
    }
    if let Some(expected) = expected {
        let actual = hex(&hasher.finalize());
        if actual != expected {
            return Err("SHA256 mismatch, the download is corrupt".to_string());
        }
    }
    Ok(())
}

// --- unpacking ---

/// A file to take out of an archive.
struct Target<'a> {
    row: &'a Row,
    dir: &'a Path,
}

fn base_name(path: &str) -> &str {
    path.rsplit(['/', '\\']).next().unwrap_or(path)
}

/// Copy `size` bytes of `reader` to the temp name next to the destination,
/// then rename it into place.
fn write_entry(reader: &mut dyn Read, size: u64, target: &Target) -> Result<(), String> {
    let dest = target.dir.join(&target.row.file);
    let part = TempFile::new(target.dir, &target.row.file, "part");
    let fail = |e: io::Error| format!("cannot write {}: {e}", part.path.display());
    let mut out = BufWriter::with_capacity(1 << 20, File::create(&part.path).map_err(fail)?);
    let mut buf = vec![0u8; CHUNK];
    let mut written = 0u64;
    target.row.progress(Status::Unpacking, 0, Some(size));
    loop {
        check_interrupt()?;
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("cannot unpack {}: {e}", target.row.file))?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n]).map_err(fail)?;
        written += n as u64;
        target.row.progress(Status::Unpacking, written, Some(size));
    }
    out.flush().map_err(fail)?;
    drop(out);
    make_executable(&part.path).map_err(fail)?;
    std::fs::rename(&part.path, &dest)
        .map_err(|e| format!("cannot replace {}: {e}", dest.display()))?;
    // renamed: nothing left to remove
    std::mem::forget(part);
    Ok(())
}

fn not_found(target: &Target) -> String {
    format!("{} not found in the archive", target.row.file)
}

fn unpack_tar(reader: impl Read, targets: &[Target]) -> Vec<Result<(), String>> {
    let mut results: Vec<Option<Result<(), String>>> = targets.iter().map(|_| None).collect();
    let mut archive = tar::Archive::new(reader);
    let entries = match archive.entries() {
        Ok(entries) => entries,
        Err(e) => return fail_all(targets, &format!("cannot read the archive: {e}")),
    };
    for entry in entries {
        if results.iter().all(Option::is_some) {
            break;
        }
        let mut entry = match entry {
            Ok(entry) => entry,
            Err(e) => {
                let message = format!("cannot read the archive: {e}");
                for r in results.iter_mut().filter(|r| r.is_none()) {
                    *r = Some(Err(message.clone()));
                }
                break;
            }
        };
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let Ok(path) = entry.path() else { continue };
        let name = base_name(&path.to_string_lossy()).to_string();
        let size = entry.size();
        if let Some(i) = targets
            .iter()
            .enumerate()
            .position(|(i, t)| results[i].is_none() && t.row.file == name)
        {
            results[i] = Some(write_entry(&mut entry, size, &targets[i]));
        }
    }
    results
        .into_iter()
        .zip(targets)
        .map(|(r, t)| r.unwrap_or_else(|| Err(not_found(t))))
        .collect()
}

fn unpack_zip(path: &Path, targets: &[Target]) -> Vec<Result<(), String>> {
    let open = || -> Result<zip::ZipArchive<BufReader<File>>, String> {
        let file = File::open(path).map_err(|e| e.to_string())?;
        zip::ZipArchive::new(BufReader::new(file)).map_err(|e| e.to_string())
    };
    let mut archive = match open() {
        Ok(a) => a,
        Err(e) => return fail_all(targets, &format!("cannot read the archive: {e}")),
    };
    let mut names: Vec<(usize, String)> = Vec::new();
    for i in 0..archive.len() {
        if let Ok(entry) = archive.by_index_raw(i)
            && entry.is_file()
            && let Ok(name) = entry.name()
        {
            names.push((i, base_name(&name).to_string()));
        }
    }
    targets
        .iter()
        .map(|t| {
            let (index, _) = names
                .iter()
                .find(|(_, n)| *n == t.row.file)
                .ok_or_else(|| not_found(t))?;
            let mut entry = archive.by_index(*index).map_err(|e| e.to_string())?;
            let size = entry.size();
            write_entry(&mut entry, size, t)
        })
        .collect()
}

fn fail_all(targets: &[Target], message: &str) -> Vec<Result<(), String>> {
    targets.iter().map(|_| Err(message.to_string())).collect()
}

fn unpack(archive: Archive, path: &Path, targets: &[Target]) -> Vec<Result<(), String>> {
    let open = || File::open(path).map_err(|e| e.to_string());
    match archive {
        Archive::Zip => unpack_zip(path, targets),
        Archive::TarGz => match open() {
            Ok(file) => unpack_tar(flate2::read::GzDecoder::new(BufReader::new(file)), targets),
            Err(e) => fail_all(targets, &e),
        },
        Archive::TarXz => {
            let workers = std::thread::available_parallelism().map_or(2, |n| n.get().min(8)) as u32;
            let reader = open().and_then(|file| {
                lzma_rust2::XzReaderMt::new(file, false, workers).map_err(|e| e.to_string())
            });
            match reader {
                Ok(reader) => unpack_tar(reader, targets),
                Err(e) => fail_all(targets, &format!("cannot read the archive: {e}")),
            }
        }
    }
}

// --- jobs ---

/// How a job finds its archive.
enum Source {
    /// Latest release of a GitHub repo: the asset is picked by suffix.
    Suffix {
        repo: &'static str,
        suffix: Option<&'static str>,
    },
    /// BtbN's ffmpeg builds.
    Btbn { tag: Option<&'static str> },
    /// A fixed URL (the final URL has the checksum next to it).
    Url(Option<String>),
}

fn resolve(agent: &ureq::Agent, source: &Source, platform: Platform) -> Result<Plan, String> {
    let unsupported = || format!("not available for {}", platform.describe());
    match source {
        Source::Suffix { repo, suffix } => {
            let suffix = suffix.ok_or_else(unsupported)?;
            let assets = release_assets(agent, repo)?;
            let names: Vec<&str> = assets.iter().map(|a| a.name.as_str()).collect();
            let name = pick_suffix(&names, suffix)
                .ok_or_else(|| format!("no {suffix} asset in the latest release of {repo}"))?;
            let asset = assets
                .iter()
                .find(|a| a.name == name)
                .expect("picked asset");
            plan_for_asset(&assets, asset, None)
        }
        Source::Btbn { tag } => {
            let tag = tag.ok_or_else(unsupported)?;
            let assets = release_assets(agent, BTBN_REPO)?;
            let names: Vec<&str> = assets.iter().map(|a| a.name.as_str()).collect();
            let name = pick_btbn(&names, tag).ok_or_else(|| {
                format!("no stable {tag} GPL build in the latest {BTBN_REPO} release")
            })?;
            let asset = assets
                .iter()
                .find(|a| a.name == name)
                .expect("picked asset");
            plan_for_asset(&assets, asset, Some("checksums.sha256"))
        }
        Source::Url(url) => Ok(Plan {
            url: url.clone().ok_or_else(unsupported)?,
            archive: Archive::Zip,
            checksum: Checksum::NextToFinalUrl,
        }),
    }
}

/// Download one archive and take the files of `rows` out of it.
fn run_job(
    exe_dir: &Path,
    platform: Platform,
    source: &Source,
    rows: &[&Row],
) -> Vec<Result<(), String>> {
    let message = |rows: &[&Row], e: String| -> Vec<Result<(), String>> {
        rows.iter().map(|_| Err(e.clone())).collect()
    };
    let agent = agent();
    let archive_tmp = TempFile::new(exe_dir, rows[0].name, "download");
    // fail early if the folder is not writable
    if let Err(e) = File::create(&archive_tmp.path) {
        return message(rows, format!("cannot write to {}: {e}", exe_dir.display()));
    }
    let plan = match resolve(&agent, source, platform) {
        Ok(plan) => plan,
        Err(e) => return message(rows, e),
    };
    let progress = |done: u64, total: Option<u64>| {
        for row in rows {
            row.progress(Status::Downloading, done, total);
        }
    };
    if let Err(e) = fetch_archive(&agent, &plan, &archive_tmp.path, &progress) {
        return message(rows, e);
    }
    for row in rows {
        row.set(Status::Unpacking, Some(DOWNLOAD_SHARE));
    }
    let targets: Vec<Target> = rows
        .iter()
        .map(|&row| Target { row, dir: exe_dir })
        .collect();
    unpack(plan.archive, &archive_tmp.path, &targets)
}

struct Job {
    source: Source,
    /// Indexes of the rows the job fills.
    rows: Vec<usize>,
}

fn jobs(platform: Platform) -> Vec<Job> {
    let ffmpeg_jobs = if btbn_tag(platform).is_some() || platform.os != Os::Darwin {
        vec![Job {
            source: Source::Btbn {
                tag: btbn_tag(platform),
            },
            rows: vec![2, 3],
        }]
    } else {
        vec![
            Job {
                source: Source::Url(macos_url(platform, "ffmpeg")),
                rows: vec![2],
            },
            Job {
                source: Source::Url(macos_url(platform, "ffprobe")),
                rows: vec![3],
            },
        ]
    };
    let mut all = vec![
        Job {
            source: Source::Suffix {
                repo: TRUEHDD_REPO,
                suffix: truehdd_suffix(platform),
            },
            rows: vec![0],
        },
        Job {
            source: Source::Suffix {
                repo: DEE_SHIM_REPO,
                suffix: dee_shim_suffix(platform),
            },
            rows: vec![1],
        },
    ];
    all.extend(ffmpeg_jobs);
    all
}

fn exe(name: &str) -> String {
    if cfg!(windows) {
        format!("{name}.exe")
    } else {
        name.to_string()
    }
}

/// `-gt`: download the tools into `exe_dir` and exit.
pub fn run(exe_dir: &Path) -> ! {
    let platform = Platform::current();
    let multi = MultiProgress::with_draw_target(ProgressDrawTarget::stdout_with_hz(8));
    let rows: Vec<Arc<Row>> = ["truehdd", "dee_shim", "ffmpeg", "ffprobe"]
        .iter()
        .map(|&name| {
            let pb = multi.add(ProgressBar::new(SCALE));
            pb.set_style(style());
            pb.enable_steady_tick(Duration::from_millis(125));
            let row = Row {
                name,
                file: exe(name),
                pb,
                state: Mutex::new((Status::Downloading, None)),
            };
            row.set(Status::Downloading, None);
            Arc::new(row)
        })
        .collect();

    let printer = multi.clone();
    interrupt::jobs_started(move |markup| {
        let text = crate::markup::render(markup, color_enabled());
        if printer.is_hidden() {
            println!("{text}");
        } else {
            let _ = printer.println(text);
        }
    });

    let outcomes: Vec<Mutex<Option<Result<(), String>>>> =
        rows.iter().map(|_| Mutex::new(None)).collect();
    std::thread::scope(|scope| {
        for job in jobs(platform) {
            let (rows, outcomes) = (&rows, &outcomes);
            scope.spawn(move || {
                let job_rows: Vec<&Row> = job.rows.iter().map(|&i| rows[i].as_ref()).collect();
                let results = run_job(exe_dir, platform, &job.source, &job_rows);
                for ((&i, row), result) in job.rows.iter().zip(&job_rows).zip(results) {
                    row.finish(result.is_ok());
                    *outcomes[i].lock().expect("outcome") = Some(result);
                }
            });
        }
    });
    interrupt::jobs_ended();

    // Finish the display before anything else is printed: dropping the
    // MultiProgress (and the bars) stops the tickers and writes the newline
    // that ends the last bar line. `process::exit` runs no destructors, so
    // without this the cursor stays at the end of the last bar line.
    let hidden = multi.is_hidden();
    let names: Vec<&'static str> = rows.iter().map(|r| r.name).collect();
    let finals: Vec<_> = rows
        .iter()
        .map(|r| *r.state.lock().expect("row state"))
        .collect();
    drop(rows);
    drop(multi);
    if !hidden {
        // indicatif leaves the cursor at the end of the last bar line, which
        // is as wide as the terminal (pending wrap). Print the line break
        // now, with no colour active: otherwise the first character of the
        // next line triggers the wrap (and the scroll) with the red
        // background of `ERROR:` set, and the terminal fills the new line
        // with it.
        println!();
    }
    let _ = std::io::stdout().flush();

    if interrupt::is_interrupted() {
        std::process::exit(interrupt::EXIT_CODE);
    }

    // Without a terminal nothing was drawn: print the final lines.
    if hidden {
        let cells = bar_cells();
        for (name, (status, fraction)) in names.iter().zip(&finals) {
            println!("{}", row_text(name, *status, *fraction, false, cells));
        }
    }

    let mut failed = false;
    let mut saved = Vec::new();
    for (&name, outcome) in names.iter().zip(&outcomes) {
        match outcome.lock().expect("outcome").take() {
            Some(Ok(())) => saved.push(name),
            Some(Err(reason)) => {
                failed = true;
                rprintln!(
                    "[color(231) on red]ERROR:[/color(231) on red] failed to get [bold yellow]{}[/bold yellow]: {}",
                    name,
                    escape(&reason)
                );
            }
            None => failed = true,
        }
    }
    if !saved.is_empty() {
        rprintln!(
            "[bold cyan]Tools saved to:[/bold cyan] {}",
            escape(&exe_dir.display().to_string())
        );
    }
    std::process::exit(i32::from(failed));
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINUX: Platform = Platform {
        os: Os::Linux,
        arch: Arch::X86_64,
    };

    #[test]
    fn btbn_picks_the_highest_stable_gpl_build() {
        let names = [
            "checksums.sha256",
            "ffmpeg-master-latest-linux64-gpl.tar.xz",
            "ffmpeg-master-latest-win64-gpl.zip",
            "ffmpeg-n8.1-latest-linux64-gpl-8.1.tar.xz",
            "ffmpeg-n8.1-latest-win64-gpl-8.1.zip",
            "ffmpeg-n9.0-latest-linux64-gpl-9.0.tar.xz",
            "ffmpeg-n9.0-latest-linux64-lgpl-9.0.tar.xz",
            "ffmpeg-n9.0-latest-linux64-gpl-shared-9.0.tar.xz",
            "ffmpeg-n9.0-latest-linuxarm64-gpl-9.0.tar.xz",
            "ffmpeg-n9.0-latest-win64-gpl-9.0.zip",
            "ffmpeg-n9.0-latest-win64-gpl-shared-9.0.zip",
            "ffmpeg-n9.0-latest-winarm64-gpl-9.0.zip",
            "ffmpeg-n10.0-latest-win64-lgpl-10.0.zip",
        ];
        assert_eq!(
            pick_btbn(&names, "linux64"),
            Some("ffmpeg-n9.0-latest-linux64-gpl-9.0.tar.xz")
        );
        assert_eq!(
            pick_btbn(&names, "win64"),
            Some("ffmpeg-n9.0-latest-win64-gpl-9.0.zip")
        );
        // numeric comparison: 10.0 beats 9.0
        let more = [
            "ffmpeg-n9.0-latest-win64-gpl-9.0.zip",
            "ffmpeg-n10.0-latest-win64-gpl-10.0.zip",
            "ffmpeg-n10.0-latest-linux64-gpl-10.0.tar.xz",
        ];
        assert_eq!(
            pick_btbn(&more, "win64"),
            Some("ffmpeg-n10.0-latest-win64-gpl-10.0.zip")
        );
        assert_eq!(
            pick_btbn(&["ffmpeg-master-latest-win64-gpl.zip"], "win64"),
            None
        );
        assert_eq!(pick_btbn(&[], "win64"), None);
    }

    #[test]
    fn btbn_platform_tags() {
        assert_eq!(btbn_tag(LINUX), Some("linux64"));
        let win = Platform {
            os: Os::Windows,
            arch: Arch::X86_64,
        };
        assert_eq!(btbn_tag(win), Some("win64"));
        let arm = Platform {
            os: Os::Linux,
            arch: Arch::Aarch64,
        };
        assert_eq!(btbn_tag(arm), None);
        let mac = Platform {
            os: Os::Darwin,
            arch: Arch::Aarch64,
        };
        assert_eq!(btbn_tag(mac), None);
    }

    #[test]
    fn truehdd_and_dee_shim_assets_per_platform() {
        let truehdd = [
            "truehdd-0.6.2-aarch64-unknown-linux-musl.tar.gz",
            "truehdd-0.6.2-universal-macOS.zip",
            "truehdd-0.6.2-universal-macOS.zip.sha256",
            "truehdd-0.6.2-x86_64-pc-windows-msvc.zip",
            "truehdd-0.6.2-x86_64-pc-windows-msvc.zip.sha256",
            "truehdd-0.6.2-x86_64-unknown-linux-gnu.tar.gz",
            "truehdd-0.6.2-x86_64-unknown-linux-musl.tar.gz",
            "truehdd-0.6.2-x86_64-unknown-linux-musl.tar.gz.sha256",
        ];
        let shim = [
            "dee-shim-0.1.0-294c9d1-linux-x86_64.tar.gz",
            "dee-shim-0.1.0-294c9d1-macos-x86_64.tar.gz",
            "dee-shim-0.1.0-294c9d1-windows-x86_64.zip",
        ];
        let win = Platform {
            os: Os::Windows,
            arch: Arch::X86_64,
        };
        let mac_arm = Platform {
            os: Os::Darwin,
            arch: Arch::Aarch64,
        };
        let mac_intel = Platform {
            os: Os::Darwin,
            arch: Arch::X86_64,
        };
        let pick = |names: &[&'static str], suffix: Option<&str>| {
            suffix.and_then(|s| pick_suffix(names, s))
        };
        assert_eq!(
            pick(&truehdd, truehdd_suffix(LINUX)),
            Some("truehdd-0.6.2-x86_64-unknown-linux-musl.tar.gz")
        );
        assert_eq!(
            pick(&truehdd, truehdd_suffix(win)),
            Some("truehdd-0.6.2-x86_64-pc-windows-msvc.zip")
        );
        for mac in [mac_arm, mac_intel] {
            assert_eq!(
                pick(&truehdd, truehdd_suffix(mac)),
                Some("truehdd-0.6.2-universal-macOS.zip")
            );
            assert_eq!(
                pick(&shim, dee_shim_suffix(mac)),
                Some("dee-shim-0.1.0-294c9d1-macos-x86_64.tar.gz")
            );
        }
        assert_eq!(
            pick(&shim, dee_shim_suffix(LINUX)),
            Some("dee-shim-0.1.0-294c9d1-linux-x86_64.tar.gz")
        );
        assert_eq!(
            pick(&shim, dee_shim_suffix(win)),
            Some("dee-shim-0.1.0-294c9d1-windows-x86_64.zip")
        );
        let other = Platform {
            os: Os::Linux,
            arch: Arch::Aarch64,
        };
        assert_eq!(truehdd_suffix(other), None);
        assert_eq!(dee_shim_suffix(other), None);
    }

    #[test]
    fn macos_urls_follow_the_architecture() {
        let arm = Platform {
            os: Os::Darwin,
            arch: Arch::Aarch64,
        };
        let intel = Platform {
            os: Os::Darwin,
            arch: Arch::X86_64,
        };
        assert_eq!(
            macos_url(arm, "ffmpeg").as_deref(),
            Some("https://ffmpeg.martin-riedl.de/redirect/latest/macos/arm64/release/ffmpeg.zip")
        );
        assert_eq!(
            macos_url(intel, "ffprobe").as_deref(),
            Some("https://ffmpeg.martin-riedl.de/redirect/latest/macos/amd64/release/ffprobe.zip")
        );
    }

    #[test]
    fn progress_maps_download_to_90_and_unpacking_to_the_rest() {
        let f = |s, d, t| overall_fraction(s, d, t).unwrap();
        assert_eq!(f(Status::Downloading, 0, Some(200)), 0.0);
        assert!((f(Status::Downloading, 100, Some(200)) - 0.45).abs() < 1e-12);
        assert!((f(Status::Downloading, 200, Some(200)) - 0.9).abs() < 1e-12);
        assert!((f(Status::Unpacking, 0, Some(50)) - 0.9).abs() < 1e-12);
        assert!((f(Status::Unpacking, 25, Some(50)) - 0.95).abs() < 1e-12);
        assert!((f(Status::Unpacking, 50, Some(50)) - 1.0).abs() < 1e-12);
        assert!((f(Status::Unpacking, 0, Some(0)) - 1.0).abs() < 1e-12);
        assert_eq!(f(Status::Done, 0, None), 1.0);
        // more bytes than announced never passes the end of the phase
        assert!((f(Status::Downloading, 300, Some(200)) - 0.9).abs() < 1e-12);
        // unknown total: pulsing bar
        assert_eq!(overall_fraction(Status::Downloading, 5, None), None);
    }

    #[test]
    fn rows_without_color() {
        let bar = |n: usize| "━".repeat(n);
        assert_eq!(
            row_text("truehdd", Status::Unpacking, Some(0.95), false, 20),
            format!("[ truehdd  | unpacking...   ] {}{} 95.00% ", bar(19), " ")
        );
        assert_eq!(
            row_text("ffmpeg", Status::Downloading, Some(0.75), false, 20),
            format!(
                "[ ffmpeg   | downloading... ] {}{} 75.00% ",
                bar(15),
                " ".repeat(5)
            )
        );
        assert_eq!(
            row_text("dee_shim", Status::Done, Some(1.0), false, 20),
            format!("[ dee_shim | done           ] {} 100%   ", bar(20))
        );
        assert_eq!(
            row_text("ffprobe", Status::Failed, Some(0.0), false, 20),
            format!("[ ffprobe  | failed         ] {} 0.00%  ", " ".repeat(20))
        );
        assert_eq!(
            label("ffmpeg", Status::Downloading).chars().count(),
            LABEL_WIDTH
        );
        assert_eq!(label("dee_shim", Status::Done).chars().count(), LABEL_WIDTH);
    }

    #[test]
    fn url_file_names() {
        let url = "https://ffmpeg.martin-riedl.de/download/macos/arm64/1789931890_9.0.2/ffmpeg.zip";
        assert_eq!(url_file_name(url), "ffmpeg.zip");
        assert_eq!(url_file_name(&format!("{url}?x=a/b#frag")), "ffmpeg.zip");
        assert_eq!(url_file_name("ffmpeg.zip"), "ffmpeg.zip");
    }

    #[test]
    fn sha256_next_to_final_url() {
        let url = "https://ffmpeg.martin-riedl.de/download/macos/arm64/1789931890_9.0.2/ffmpeg.zip";
        let hash = "c8ed4c4e6978a03c485edbfe4e0a5dc2380f8a30bba5150531b31b094492d924";
        let text = format!("{hash}  ffmpeg.zip\n");
        assert_eq!(parse_sha256(&text, url_file_name(url)), Some(hash.into()));
        assert_eq!(parse_sha256(&text, ""), None);
    }

    #[test]
    fn sha256_files() {
        let hash = "01631f714e0083fe70b9b4d7e274e3f7da4c082566b4883204da34b1098c65d3";
        assert_eq!(parse_sha256(hash, "x.zip").as_deref(), Some(hash));
        assert_eq!(
            parse_sha256(&format!("{hash}\n"), "x.zip").as_deref(),
            Some(hash)
        );
        assert_eq!(
            parse_sha256(&format!("{hash}  x.zip\n"), "x.zip").as_deref(),
            Some(hash)
        );
        assert_eq!(
            parse_sha256(&format!("{hash} *x.zip"), "x.zip").as_deref(),
            Some(hash)
        );
        let upper = hash.to_ascii_uppercase();
        assert_eq!(
            parse_sha256(&format!("{upper}  x.zip"), "x.zip").as_deref(),
            Some(hash)
        );
        let other = "f".repeat(64);
        let list = format!("{other}  a.zip\n{hash}  b.zip\n");
        assert_eq!(parse_sha256(&list, "b.zip").as_deref(), Some(hash));
        assert_eq!(
            parse_sha256(&list, "a.zip").as_deref(),
            Some(other.as_str())
        );
        assert_eq!(parse_sha256(&list, "c.zip"), None);
        assert_eq!(parse_sha256("not a hash  x.zip", "x.zip"), None);
        assert_eq!(parse_sha256("", "x.zip"), None);
    }

    #[test]
    fn archive_types() {
        assert_eq!(Archive::of("a.tar.gz"), Some(Archive::TarGz));
        assert_eq!(Archive::of("a.tar.xz"), Some(Archive::TarXz));
        assert_eq!(Archive::of("a.zip"), Some(Archive::Zip));
        assert_eq!(Archive::of("a.7z"), None);
    }

    #[test]
    fn asset_plan_prefers_the_sidecar_checksum() {
        let hash = "a".repeat(64);
        let asset = |name: &str, digest: Option<String>| Asset {
            name: name.to_string(),
            url: format!("https://x/{name}"),
            digest,
        };
        let assets = vec![
            asset("t.zip", Some(format!("sha256:{hash}"))),
            asset("t.zip.sha256", None),
            asset("u.zip", Some(format!("sha256:{hash}"))),
            asset("v.zip", None),
        ];
        let plan = plan_for_asset(&assets, &assets[0], None).unwrap();
        assert!(matches!(plan.checksum, Checksum::Url { .. }));
        let plan = plan_for_asset(&assets, &assets[2], None).unwrap();
        assert!(matches!(&plan.checksum, Checksum::Known(h) if *h == hash));
        let plan = plan_for_asset(&assets, &assets[3], None).unwrap();
        assert!(matches!(plan.checksum, Checksum::None));
    }
}
