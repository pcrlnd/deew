//! `-u [all]`: update deew, and deew-gui and deew-web next to it, from the
//! latest GitHub release (`all` also installs the missing ones).

use crate::error::{error_line, escape};
use crate::get_tools::{
    Arch, Archive, Asset, Checksum, DOWNLOAD_SHARE, Os, Plan, Platform, Release, Row, Status,
    TempFile, agent, check_interrupt, fetch_archive, get_text, hex, is_sha256, latest_release,
    make_executable, with_live_rows,
};
use crate::markup::color_enabled;
use crate::rprintln;
use sha2::{Digest, Sha256};
use std::cmp::Ordering;
use std::ffi::OsString;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, Mutex};

const REPO: &str = "pcrlnd/deew";
const RELEASES_URL: &str = "https://github.com/pcrlnd/deew/releases/latest";
/// The executable of the app bundle: it has to exist in a good extraction.
const APP_BINARY: &str = "Contents/MacOS/deew-gui";

// --- components and platform names ---

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Comp {
    Deew,
    Gui,
    Web,
}

impl Comp {
    fn name(self) -> &'static str {
        match self {
            Comp::Deew => "deew",
            Comp::Gui => "deew-gui",
            Comp::Web => "deew-web",
        }
    }
}

/// The asset names of a platform.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Names {
    deew: &'static str,
    gui: &'static str,
    web: &'static str,
    /// The GUI is a zipped `deew-gui.app` (macOS).
    app: bool,
}

fn platform_names(platform: Platform) -> Option<Names> {
    match (platform.os, platform.arch) {
        (Os::Linux, Arch::X86_64) => Some(Names {
            deew: "deew",
            gui: "deew-gui",
            web: "deew-web",
            app: false,
        }),
        (Os::Windows, Arch::X86_64) => Some(Names {
            deew: "deew.exe",
            gui: "deew-gui.exe",
            web: "deew-web.exe",
            app: false,
        }),
        (Os::Darwin, Arch::X86_64 | Arch::Aarch64) => Some(Names {
            deew: "deew-macos",
            gui: "deew-gui-macos.zip",
            web: "deew-web-macos",
            app: true,
        }),
        _ => None,
    }
}

impl Names {
    /// The name of the release asset.
    fn asset(&self, comp: Comp) -> &'static str {
        match comp {
            Comp::Deew => self.deew,
            Comp::Gui => self.gui,
            Comp::Web => self.web,
        }
    }

    /// The name in the executable folder. New installs use the asset names,
    /// except the macOS GUI, the app extracted from the zip.
    fn local(&self, comp: Comp) -> &'static str {
        if comp == Comp::Gui && self.app {
            "deew-gui.app"
        } else {
            self.asset(comp)
        }
    }
}

/// The folder the tools live in: next to the executable.
fn install_dir(exe: &Path) -> Option<PathBuf> {
    exe.parent().map(Path::to_path_buf)
}

// --- leftovers ---

/// Names of the files the swap leaves behind.
const LEFTOVER_FILES: &[&str] = &[
    "deew.old",
    "deew-gui.old",
    "deew-web.old",
    "deew.exe.old",
    "deew-gui.exe.old",
    "deew-web.exe.old",
    "deew-macos.old",
    "deew-gui-macos.old",
    "deew-web-macos.old",
];

/// Delete what a previous update left behind in `dir`. Silent, best effort.
fn remove_leftovers(dir: &Path) {
    for name in LEFTOVER_FILES {
        let _ = std::fs::remove_file(dir.join(name));
    }
    let _ = std::fs::remove_dir_all(dir.join("deew-gui.app.old"));
}

/// Delete a leftover `deew-gui.app.old` in an Applications folder (only that
/// exact name). Silent, best effort.
fn remove_app_leftover(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir.join("deew-gui.app.old"));
}

/// Startup cleanup of the leftovers of the last update.
pub fn cleanup_leftovers() {
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| install_dir(&e)) {
        remove_leftovers(&dir);
    }
    let dirs = AppDirs::current();
    remove_app_leftover(&dirs.system);
    if let Some(user) = &dirs.user {
        remove_app_leftover(user);
    }
}

// --- where the macOS app lives ---

/// The macOS folders that can hold `deew-gui.app` besides the deew folder.
/// Empty (and unused) on the other systems.
#[derive(Debug, Clone, PartialEq, Eq)]
struct AppDirs {
    /// `/Applications`.
    system: PathBuf,
    /// `~/Applications`.
    user: Option<PathBuf>,
    /// `~/Library/Application Support/deew`.
    support: Option<PathBuf>,
}

impl AppDirs {
    fn current() -> AppDirs {
        let home = if cfg!(target_os = "macos") {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .filter(|h| h.is_absolute())
        } else {
            None
        };
        AppDirs {
            system: PathBuf::from("/Applications"),
            user: home.as_ref().map(|h| h.join("Applications")),
            support: home.map(|h| h.join("Library/Application Support/deew")),
        }
    }
}

fn same_dir(a: &Path, b: &Path) -> bool {
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

/// The folder holding `deew-gui.app`, or where `-u all` installs it. Only
/// the macOS app moves; everything else lives in `exe_dir`.
fn gui_dir(
    names: &Names,
    exe_dir: &Path,
    dirs: &AppDirs,
    writable: &dyn Fn(&Path) -> bool,
) -> PathBuf {
    if !names.app {
        return exe_dir.to_path_buf();
    }
    // the portable folder wins, with the bare binary of the old releases
    if exe_dir.join("deew-gui.app").is_dir() || exe_dir.join("deew-gui-macos").is_file() {
        return exe_dir.to_path_buf();
    }
    for dir in std::iter::once(&dirs.system).chain(dirs.user.as_ref()) {
        if dir.join("deew-gui.app").is_dir() {
            return dir.clone();
        }
    }
    // missing: install next to deew, or in Applications when deew lives in
    // Application Support
    let in_support = dirs
        .support
        .as_deref()
        .is_some_and(|s| same_dir(exe_dir, s));
    if in_support {
        if writable(&dirs.system) {
            return dirs.system.clone();
        }
        if let Some(user) = &dirs.user {
            return user.clone();
        }
    }
    exe_dir.to_path_buf()
}

// --- local state ---

#[derive(Debug, PartialEq, Eq)]
enum Local {
    Missing,
    /// A bare binary, or the running deew. For deew-gui and deew-web, the
    /// version found in the file's marker, when there is one.
    Binary(Option<String>),
    /// `deew-gui.app` with its version, when it could be read.
    App(Option<String>),
    /// The bare `deew-gui-macos` of the old releases.
    OldBare,
}

/// `CFBundleShortVersionString` out of the text of an `Info.plist`.
fn plist_version(text: &str) -> Option<String> {
    let rest = text.split_once("<key>CFBundleShortVersionString</key>")?.1;
    let rest = rest.split_once("<string>")?.1;
    let version = rest.split_once("</string>")?.0.trim();
    (!version.is_empty()).then(|| version.to_string())
}

fn local_state(names: &Names, comp: Comp, dir: &Path) -> Local {
    let local = dir.join(names.local(comp));
    if comp == Comp::Gui && names.app {
        if local.is_dir() {
            let plist = std::fs::read_to_string(local.join("Contents/Info.plist")).ok();
            return Local::App(plist.as_deref().and_then(plist_version));
        }
        if dir.join("deew-gui-macos").is_file() {
            return Local::OldBare;
        }
        return Local::Missing;
    }
    if local.is_file() {
        Local::Binary(match comp {
            Comp::Deew => None,
            _ => binary_marker_version(&local, comp),
        })
    } else {
        Local::Missing
    }
}

/// The version a deew-gui / deew-web binary carries in its `VERSION_MARK`
/// (`\0deew-gui-version=1.0.1\0`). The file is only read, never run.
fn binary_marker_version(path: &Path, comp: Comp) -> Option<String> {
    let prefix: &[u8] = match comp {
        Comp::Gui => b"\0deew-gui-version=",
        Comp::Web => b"\0deew-web-version=",
        Comp::Deew => return None,
    };
    scan_marker(File::open(path).ok()?, prefix, 256 * 1024)
}

/// Scans `reader` for `prefix` followed by a version and a NUL byte, in chunks
/// of `chunk` bytes (a marker may span two of them).
fn scan_marker(mut reader: impl Read, prefix: &[u8], chunk: usize) -> Option<String> {
    const MAX_VERSION: usize = 64;
    let mut buf: Vec<u8> = Vec::new();
    let mut block = vec![0u8; chunk];
    loop {
        let n = reader.read(&mut block).ok()?;
        if n == 0 {
            return None;
        }
        buf.extend_from_slice(&block[..n]);
        if let Some(pos) = buf.windows(prefix.len()).position(|w| w == prefix) {
            let rest = &buf[pos + prefix.len()..];
            if let Some(end) = rest.iter().position(|&b| b == 0) {
                let version = std::str::from_utf8(&rest[..end]).ok()?.trim();
                return (!version.is_empty()).then(|| version.to_string());
            }
            if rest.len() > MAX_VERSION {
                return None;
            }
            // the version continues in the next chunk
            buf.drain(..pos);
        } else {
            let keep = prefix.len() - 1;
            if buf.len() > keep {
                buf.drain(..buf.len() - keep);
            }
        }
    }
}

fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            return Ok(hex(&hasher.finalize()));
        }
        hasher.update(&buf[..n]);
    }
}

// --- plan ---

/// The versions of the latest release (`versions.json`), if it has them.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
struct Versions {
    deew: Option<String>,
    gui: Option<String>,
    web: Option<String>,
}

fn parse_versions(text: &str) -> Option<Versions> {
    let json: serde_json::Value = serde_json::from_str(text).ok()?;
    let get = |key: &str| json.get(key)?.as_str().map(str::to_string);
    Some(Versions {
        deew: get("deew"),
        gui: get("deew-gui"),
        web: get("deew-web"),
    })
}

impl Versions {
    fn of(&self, comp: Comp) -> Option<&str> {
        match comp {
            Comp::Deew => self.deew.as_deref(),
            Comp::Gui => self.gui.as_deref(),
            Comp::Web => self.web.as_deref(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Action {
    Update,
    Install,
    UpToDate,
    /// Not installed, and `all` was not given.
    Skip,
    /// The release has no (verifiable) asset for it.
    Unavailable,
    /// The local version is newer than the release: never downgraded.
    Newer,
}

impl Action {
    fn text(self) -> &'static str {
        match self {
            Action::Update => "update",
            Action::Install => "install",
            Action::UpToDate => "up to date",
            Action::Skip => "skipped (-u all installs it)",
            Action::Unavailable => "not in the release",
            Action::Newer => "newer than the release",
        }
    }

    fn color(self) -> &'static str {
        match self {
            Action::Update | Action::Install => "bold yellow",
            Action::UpToDate => "green",
            Action::Skip | Action::Newer => "not bold white",
            Action::Unavailable => "red",
        }
    }

    fn todo(self) -> bool {
        matches!(self, Action::Update | Action::Install)
    }
}

#[derive(Debug, Clone)]
struct Item {
    comp: Comp,
    /// The folder, when it isn't the deew folder.
    place: Option<PathBuf>,
    current: String,
    latest: String,
    action: Action,
    /// The asset to download for an update or install.
    asset: Option<Asset>,
    /// The old bare macOS GUI, deleted after the app is in place.
    remove_after: Option<PathBuf>,
}

/// What `make_plan` needs to know about the machine and the release.
struct Context<'a> {
    names: Names,
    dir: &'a Path,
    /// The folder of `deew-gui.app` (`dir` unless it is in Applications).
    gui_dir: &'a Path,
    /// The running executable.
    exe: &'a Path,
    /// The version of the running deew.
    deew_version: &'a str,
    all: bool,
    tag: &'a str,
    assets: &'a [Asset],
    versions: &'a Versions,
}

/// The SHA256 GitHub computed for an asset.
fn asset_digest(asset: &Asset) -> Option<String> {
    asset
        .digest
        .as_deref()
        .and_then(|d| d.strip_prefix("sha256:"))
        .filter(|h| is_sha256(h))
        .map(str::to_ascii_lowercase)
}

/// A dotted numeric version (`5.5.4`, `v1.0`) as numbers.
fn parse_version(text: &str) -> Option<Vec<u64>> {
    let text = text.trim().trim_start_matches('v');
    text.split('.')
        .map(|part| part.parse::<u64>().ok())
        .collect()
}

/// How `have` compares to `want`; `None` unless both are known and parse.
fn compare_versions(have: Option<&str>, want: Option<&str>) -> Option<Ordering> {
    let (mut a, mut b) = (parse_version(have?)?, parse_version(want?)?);
    let len = a.len().max(b.len());
    a.resize(len, 0);
    b.resize(len, 0);
    Some(a.cmp(&b))
}

fn make_plan(cx: &Context) -> Vec<Item> {
    let tag = cx.tag.trim_start_matches('v');
    [Comp::Deew, Comp::Gui, Comp::Web]
        .into_iter()
        .map(|comp| {
            let latest = match comp {
                Comp::Deew => tag.to_string(),
                _ => cx.versions.of(comp).unwrap_or("?").to_string(),
            };
            let asset = cx
                .assets
                .iter()
                .find(|a| a.name == cx.names.asset(comp))
                .cloned();
            let dir = if comp == Comp::Gui {
                cx.gui_dir
            } else {
                cx.dir
            };
            let local = if comp == Comp::Deew {
                Local::Binary(None)
            } else {
                local_state(&cx.names, comp, dir)
            };
            let local_path = if comp == Comp::Deew {
                cx.exe.to_path_buf()
            } else {
                dir.join(cx.names.local(comp))
            };
            // a bare binary without a marker (older builds) that is byte for
            // byte the release asset is the release's version
            let by_asset = match (&local, comp, &asset) {
                (Local::Binary(None), Comp::Gui | Comp::Web, Some(a)) => asset_digest(a)
                    .filter(|d| sha256_file(&local_path).ok().as_ref() == Some(d))
                    .and_then(|_| cx.versions.of(comp).map(str::to_string)),
                _ => None,
            };
            let known = match &local {
                Local::App(v) => v.clone(),
                Local::Binary(v) if comp != Comp::Deew => v.clone().or(by_asset),
                _ => None,
            };
            let mut item = Item {
                comp,
                place: (dir != cx.dir).then(|| dir.to_path_buf()),
                current: match &local {
                    Local::Missing => "missing".to_string(),
                    _ if comp == Comp::Deew => cx.deew_version.to_string(),
                    _ => known.clone().unwrap_or_else(|| "unknown".to_string()),
                },
                latest,
                action: Action::UpToDate,
                asset: None,
                remove_after: None,
            };
            if local == Local::Missing && !cx.all {
                item.action = Action::Skip;
                return item;
            }
            let Some(asset) = asset.filter(|a| asset_digest(a).is_some()) else {
                item.action = Action::Unavailable;
                return item;
            };
            // the local version, when known
            let have = if comp == Comp::Deew {
                Some(cx.deew_version)
            } else {
                known.as_deref()
            };
            let want = if comp == Comp::Deew {
                Some(tag)
            } else {
                cx.versions.of(comp)
            };
            let by_digest = || {
                if sha256_file(&local_path).ok() == asset_digest(&asset) {
                    Action::UpToDate
                } else {
                    Action::Update
                }
            };
            item.action = match &local {
                Local::Missing => Action::Install,
                Local::OldBare => Action::Update,
                _ => match compare_versions(have, want) {
                    Some(Ordering::Less) => Action::Update,
                    Some(Ordering::Equal) => Action::UpToDate,
                    Some(Ordering::Greater) => Action::Newer,
                    // an app of unknown version is outdated; a bare file is
                    // judged by its digest
                    None if matches!(local, Local::App(_)) => Action::Update,
                    None => by_digest(),
                },
            };
            if local == Local::OldBare {
                item.remove_after = Some(dir.join("deew-gui-macos"));
            }
            if item.action.todo() {
                item.asset = Some(asset);
            }
            item
        })
        .collect()
}

// --- staging: download and verify ---

/// A folder that is removed when dropped, unless it was renamed into place.
struct DirGuard {
    path: PathBuf,
}

impl Drop for DirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

impl Item {
    /// The name in the table, with the folder when it isn't the deew folder.
    fn label(&self) -> String {
        match &self.place {
            Some(place) => format!("{} ({})", self.comp.name(), place.display()),
            None => self.comp.name().to_string(),
        }
    }
}

/// A downloaded, verified file or app next to its destination.
enum Staged {
    Bare { new: TempFile, dest: PathBuf },
    App { new: DirGuard, dest: PathBuf },
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut name: OsString = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: Option<u32>) -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    match mode {
        Some(mode) => std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode & 0o777)),
        None => Ok(()),
    }
}

#[cfg(not(unix))]
fn set_mode(_path: &Path, _mode: Option<u32>) -> io::Result<()> {
    Ok(())
}

#[cfg(unix)]
fn make_symlink(link: &str, path: &Path) -> io::Result<()> {
    std::os::unix::fs::symlink(link, path)
}

#[cfg(not(unix))]
fn make_symlink(_link: &str, _path: &Path) -> io::Result<()> {
    Err(io::Error::other("symlinks are not supported here"))
}

/// Unpack a zipped app: the top folder (`deew-gui.app/`) of the archive
/// becomes `dest`. File modes, folders and symlinks are kept.
fn extract_app(zip_path: &Path, dest: &Path, progress: &dyn Fn(u64, u64)) -> Result<(), String> {
    let file = File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(io::BufReader::new(file))
        .map_err(|e| format!("cannot read the archive: {e}"))?;
    let mut total = 0u64;
    for i in 0..archive.len() {
        if let Ok(entry) = archive.by_index_raw(i) {
            total += entry.size();
        }
    }
    std::fs::create_dir_all(dest).map_err(|e| format!("cannot create {}: {e}", dest.display()))?;
    let mut done = 0u64;
    for i in 0..archive.len() {
        check_interrupt()?;
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let Some(path) = entry.enclosed_name() else {
            return Err(format!(
                "unsafe path in the archive: {}",
                entry.name().unwrap_or_default()
            ));
        };
        let mut parts = path.components();
        // the top folder; anything outside of an .app is not part of it
        let top_is_app = matches!(parts.next(), Some(Component::Normal(top))
            if Path::new(top).extension().is_some_and(|e| e.eq_ignore_ascii_case("app")));
        if !top_is_app {
            continue;
        }
        let rel: PathBuf = parts.collect();
        if rel.as_os_str().is_empty() {
            continue;
        }
        let target = dest.join(&rel);
        let fail = |e: io::Error| format!("cannot write {}: {e}", target.display());
        let mode = entry.unix_mode();
        if entry.is_dir() {
            std::fs::create_dir_all(&target).map_err(fail)?;
            set_mode(&target, mode.map(|m| m | 0o700)).map_err(fail)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(fail)?;
        }
        if entry.is_symlink() {
            let mut link = String::new();
            entry
                .read_to_string(&mut link)
                .map_err(|e| format!("cannot read the archive: {e}"))?;
            if Path::new(&link).is_absolute() {
                return Err(format!("unsafe symlink in the archive: {link}"));
            }
            make_symlink(&link, &target).map_err(fail)?;
        } else {
            let mut out = File::create(&target).map_err(fail)?;
            io::copy(&mut entry, &mut out).map_err(fail)?;
            drop(out);
            set_mode(&target, Some(mode.unwrap_or(0o644))).map_err(fail)?;
        }
        done += entry.size();
        progress(done, total);
    }
    if !dest.join(APP_BINARY).is_file() {
        return Err(format!("{APP_BINARY} is missing from the archive"));
    }
    Ok(())
}

/// Download the asset of `item` next to its destination and verify it.
fn stage(
    agent: &ureq::Agent,
    dir: &Path,
    gui_dir: &Path,
    names: &Names,
    item: &Item,
    row: &Row,
) -> Result<Staged, String> {
    let dir = if item.comp == Comp::Gui { gui_dir } else { dir };
    let asset = item.asset.as_ref().ok_or("nothing to download")?;
    let digest = asset_digest(asset).ok_or("the release has no checksum for it")?;
    let plan = Plan {
        url: asset.url.clone(),
        archive: Archive::Zip,
        checksum: Checksum::Known(digest),
    };
    let progress = |done: u64, total: Option<u64>| row.progress(Status::Downloading, done, total);
    let dest = dir.join(names.local(item.comp));
    if item.comp == Comp::Gui && names.app {
        let zip = TempFile::new(dir, "deew-gui", "zip");
        fetch_archive(agent, &plan, &zip.path, &progress)?;
        row.set(Status::Unpacking, Some(DOWNLOAD_SHARE));
        let new = DirGuard {
            path: with_suffix(&dest, ".new"),
        };
        let _ = std::fs::remove_dir_all(&new.path);
        extract_app(&zip.path, &new.path, &|done, total| {
            row.progress(Status::Unpacking, done, Some(total));
        })?;
        return Ok(Staged::App { new, dest });
    }
    let new = TempFile::at(with_suffix(&dest, ".new"));
    fetch_archive(agent, &plan, &new.path, &progress)?;
    make_executable(&new.path).map_err(|e| format!("cannot write {}: {e}", new.path.display()))?;
    Ok(Staged::Bare { new, dest })
}

// --- swap ---

/// Put `new` in place of `dest`. With `keep_old` (Windows, where a running
/// executable can be renamed but not replaced) the old file is renamed to
/// `<dest>.old` first and renamed back when the second rename fails.
fn swap_file(dest: &Path, new: &Path, keep_old: bool) -> Result<(), String> {
    let fail = |e: io::Error| format!("cannot replace {}: {e}", dest.display());
    if !keep_old || !dest.exists() {
        return std::fs::rename(new, dest).map_err(fail);
    }
    let old = with_suffix(dest, ".old");
    let _ = std::fs::remove_file(&old);
    std::fs::rename(dest, &old).map_err(fail)?;
    if let Err(e) = std::fs::rename(new, dest) {
        // never leave the tool missing
        let _ = std::fs::rename(&old, dest);
        return Err(fail(e));
    }
    Ok(())
}

/// Put the app `new` in place of `dest`: the old one is renamed to
/// `<dest>.old`, renamed back on failure and removed when all went well.
fn swap_app(dest: &Path, new: &Path) -> Result<(), String> {
    let fail = |e: io::Error| format!("cannot replace {}: {e}", dest.display());
    if !dest.exists() {
        return std::fs::rename(new, dest).map_err(fail);
    }
    let old = with_suffix(dest, ".old");
    let _ = std::fs::remove_dir_all(&old);
    std::fs::rename(dest, &old).map_err(fail)?;
    if let Err(e) = std::fs::rename(new, dest) {
        let _ = std::fs::rename(&old, dest);
        return Err(fail(e));
    }
    // a failure here is cleaned up at the next start
    let _ = std::fs::remove_dir_all(&old);
    Ok(())
}

fn install(staged: &Staged) -> Result<(), String> {
    match staged {
        Staged::Bare { new, dest } => swap_file(dest, &new.path, cfg!(windows)),
        Staged::App { new, dest } => swap_app(dest, &new.path),
    }
}

// --- the command ---

/// Fail unless files can be created in `dir`.
fn check_writable(dir: &Path) -> Result<(), String> {
    check_writable_with(
        dir,
        &format!(
            "Run deew as administrator (sudo), or download the new version from {RELEASES_URL}"
        ),
    )
}

/// Fail unless files can be created in `dir`; `advice` follows the error.
fn check_writable_with(dir: &Path, advice: &str) -> Result<(), String> {
    let probe = dir.join(format!(".deew-update-probe-{}", std::process::id()));
    match File::create(&probe) {
        Ok(file) => {
            drop(file);
            let _ = std::fs::remove_file(&probe);
            Ok(())
        }
        Err(e) => Err(format!("cannot write to {}: {e}. {advice}", dir.display())),
    }
}

fn fetch_versions(agent: &ureq::Agent, assets: &[Asset]) -> Versions {
    assets
        .iter()
        .find(|a| a.name == "versions.json")
        .and_then(|a| get_text(agent, &a.url).ok())
        .and_then(|text| parse_versions(&text))
        .unwrap_or_default()
}

/// The table of the plan as markup.
fn table_lines(items: &[Item], color: bool) -> Vec<String> {
    let width = |f: &dyn Fn(&Item) -> usize, header: &str| {
        items.iter().map(f).chain([header.len()]).max().unwrap_or(0)
    };
    let (w0, w1, w2) = (
        width(&|i| i.label().len(), "component"),
        width(&|i| i.current.len(), "current"),
        width(&|i| i.latest.len(), "latest"),
    );
    let sep = if color {
        " [color(240)]│[/color(240)] "
    } else {
        " │ "
    };
    let mut lines = vec![format!(
        "[bold white]{:<w0$}[/bold white]{sep}[bold white]{:<w1$}[/bold white]{sep}[bold white]{:<w2$}[/bold white]{sep}[bold white]action[/bold white]",
        "component", "current", "latest"
    )];
    for item in items {
        lines.push(format!(
            "[bold cyan]{}[/bold cyan]{sep}{}{sep}{}{sep}[{c}]{}[/{c}]",
            escape(&format!("{:<w0$}", item.label())),
            escape(&format!("{:<w1$}", item.current)),
            escape(&format!("{:<w2$}", item.latest)),
            item.action.text(),
            c = item.action.color(),
        ));
    }
    lines
}

fn fail(message: &str) -> ! {
    error_line(&escape(message))
}

/// `-u [all]`: update and exit.
pub fn run(exe_dir: &Path, all: bool, no_prompt: bool) -> ! {
    let platform = Platform::current();
    let Some(names) = platform_names(platform) else {
        fail(&format!(
            "no deew build for this platform ({})",
            platform.describe()
        ));
    };
    if let Err(e) = check_writable(exe_dir) {
        fail(&e);
    }
    let exe = std::env::current_exe().unwrap_or_else(|e| fail(&format!("cannot find deew: {e}")));
    let app_dirs = AppDirs::current();
    let gui_dir = gui_dir(&names, exe_dir, &app_dirs, &|d| check_writable(d).is_ok());
    let agent = agent();
    let Release { tag, url, assets } = latest_release(&agent, REPO).unwrap_or_else(|e| fail(&e));
    let versions = fetch_versions(&agent, &assets);
    let items = make_plan(&Context {
        names,
        dir: exe_dir,
        gui_dir: &gui_dir,
        exe: &exe,
        deew_version: env!("CARGO_PKG_VERSION"),
        all,
        tag: &tag,
        assets: &assets,
        versions: &versions,
    });

    for line in table_lines(&items, color_enabled()) {
        rprintln!("{line}");
    }
    let release_url = url.unwrap_or_else(|| RELEASES_URL.to_string());
    rprintln!("[bold cyan]Release:[/bold cyan] {}", escape(&release_url));
    if !items.iter().any(|i| i.action.todo()) {
        rprintln!("[bold green]Everything is up to date.[/bold green]");
        std::process::exit(0);
    }
    let question = "Update now?";
    if no_prompt {
        crate::events::warning(question);
        rprintln!(
            "[bold yellow]{question}[/bold yellow] [not bold white]Continuing (-np).[/not bold white]"
        );
    } else if !crate::prompt::confirm(question) {
        std::process::exit(1);
    }

    // every folder that is written must be writable
    if items.iter().any(|i| i.comp == Comp::Gui && i.action.todo()) && gui_dir != exe_dir {
        if !gui_dir.exists() {
            let _ = std::fs::create_dir_all(&gui_dir);
        }
        let advice = "Run as an admin user or move deew-gui.app to ~/Applications";
        let result = if gui_dir == app_dirs.system {
            check_writable_with(&gui_dir, advice)
                .map_err(|e| e.replacen("cannot write to", "can't write to", 1))
        } else {
            check_writable(&gui_dir)
        };
        if let Err(e) = result {
            fail(&e);
        }
    }

    // download and verify everything first, nothing is replaced before that
    let todo: Vec<&Item> = items.iter().filter(|i| i.action.todo()).collect();
    let rows: Vec<Arc<Row>> = todo
        .iter()
        .map(|i| Arc::new(Row::new(i.comp.name(), names.local(i.comp).to_string())))
        .collect();
    let staged: Vec<Mutex<Option<Result<Staged, String>>>> =
        todo.iter().map(|_| Mutex::new(None)).collect();
    with_live_rows(&rows, || {
        std::thread::scope(|scope| {
            for ((item, row), slot) in todo.iter().zip(&rows).zip(&staged) {
                let (agent, names, gui_dir) = (&agent, &names, gui_dir.as_path());
                scope.spawn(move || {
                    let result = stage(agent, exe_dir, gui_dir, names, item, row);
                    row.finish(result.is_ok());
                    *slot.lock().expect("staged") = Some(result);
                });
            }
        });
    });
    let mut ready: Vec<(&Item, Staged)> = Vec::new();
    let mut failed = false;
    for (item, slot) in todo.iter().zip(staged) {
        match slot.into_inner().expect("staged") {
            Some(Ok(staged)) => ready.push((item, staged)),
            Some(Err(reason)) => {
                failed = true;
                rprintln!(
                    "[color(231) on red]ERROR:[/color(231) on red] failed to download [bold yellow]{}[/bold yellow]: {}",
                    item.comp.name(),
                    escape(&reason)
                );
            }
            None => failed = true,
        }
    }
    if failed {
        // removes the downloads, nothing was changed
        drop(ready);
        rprintln!("Nothing was updated.");
        std::process::exit(1);
    }

    // deew itself last
    ready.sort_by_key(|(item, _)| item.comp == Comp::Deew);
    let mut done: Vec<&Item> = Vec::new();
    for (item, staged) in &ready {
        match install(staged) {
            Ok(()) => {
                if let Some(old) = &item.remove_after {
                    let _ = std::fs::remove_file(old);
                }
                done.push(item);
            }
            Err(reason) => {
                rprintln!(
                    "[color(231) on red]ERROR:[/color(231) on red] failed to update [bold yellow]{}[/bold yellow]: {}",
                    item.comp.name(),
                    escape(&reason)
                );
                if !done.is_empty() {
                    rprintln!(
                        "Already updated: {}.",
                        done.iter()
                            .map(|i| i.comp.name())
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
                drop(ready);
                std::process::exit(1);
            }
        }
    }
    drop(ready);

    for item in &done {
        let verb = if item.action == Action::Install {
            "Installed"
        } else {
            "Updated"
        };
        let version = if item.latest == "?" {
            String::new()
        } else {
            format!(" ({})", escape(&item.latest))
        };
        rprintln!(
            "[bold green]{verb}[/bold green] [bold cyan]{}[/bold cyan]{version}",
            item.comp.name()
        );
    }
    rprintln!(
        "[bold cyan]Saved to:[/bold cyan] {}",
        escape(&exe_dir.display().to_string())
    );
    for item in done.iter().filter(|i| i.place.is_some()) {
        if let Some(place) = &item.place {
            rprintln!(
                "[bold cyan]{} saved to:[/bold cyan] {}",
                item.comp.name(),
                escape(&place.display().to_string())
            );
        }
    }
    if done
        .iter()
        .any(|i| i.comp != Comp::Deew && i.action == Action::Update)
    {
        rprintln!("Restart deew-gui/deew-web if they are running.");
    }
    std::process::exit(0);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const LINUX: Platform = Platform {
        os: Os::Linux,
        arch: Arch::X86_64,
    };
    const WINDOWS: Platform = Platform {
        os: Os::Windows,
        arch: Arch::X86_64,
    };
    const DEEW_VERSION: &str = "9.9.9";
    const MAC: Platform = Platform {
        os: Os::Darwin,
        arch: Arch::Aarch64,
    };

    fn sha(bytes: &[u8]) -> String {
        hex(&Sha256::digest(bytes))
    }

    fn asset(name: &str, content: &[u8]) -> Asset {
        Asset {
            name: name.to_string(),
            url: format!("https://example.invalid/{name}"),
            digest: Some(format!("sha256:{}", sha(content))),
        }
    }

    fn versions() -> Versions {
        Versions {
            deew: Some("9.9.9".into()),
            gui: Some("2.0.0".into()),
            web: Some("2.0.0".into()),
        }
    }

    fn plan_for(
        names: Names,
        dir: &Path,
        exe: &Path,
        all: bool,
        assets: &[Asset],
        versions: &Versions,
    ) -> Vec<Item> {
        make_plan(&Context {
            names,
            dir,
            gui_dir: dir,
            exe,
            deew_version: DEEW_VERSION,
            all,
            tag: "v9.9.9",
            assets,
            versions,
        })
    }

    fn actions(items: &[Item]) -> Vec<Action> {
        items.iter().map(|i| i.action).collect()
    }

    // --- parsing ---

    fn parse(argv: &[&str]) -> Result<crate::cli::Args, String> {
        let argv: Vec<String> = argv.iter().map(|s| s.to_string()).collect();
        crate::cli::parse_quiet(&argv)
    }

    #[test]
    fn the_option_takes_an_optional_all() {
        let a = parse(&["-u"]).unwrap();
        assert!(a.update && !a.update_all);
        let a = parse(&["-u", "all"]).unwrap();
        assert!(a.update && a.update_all);
        let a = parse(&["--update", "all"]).unwrap();
        assert!(a.update && a.update_all);
        let a = parse(&["--update=all"]).unwrap();
        assert!(a.update && a.update_all);
        let a = parse(&["-u", "-np"]).unwrap();
        assert!(a.update && !a.update_all && a.no_prompt);
        let a = parse(&["-np", "-u"]).unwrap();
        assert!(a.update && !a.update_all && a.no_prompt);
    }

    #[test]
    fn other_values_are_invalid_choices() {
        assert_eq!(
            parse(&["-u", "bogus"]).unwrap_err(),
            "argument -u/--update: invalid choice: 'bogus' (choose from 'all')"
        );
        assert!(
            parse(&["--update=x"])
                .unwrap_err()
                .contains("invalid choice")
        );
    }

    // --- platform ---

    #[test]
    fn assets_of_the_platforms() {
        let n = platform_names(LINUX).unwrap();
        assert_eq!(
            (n.deew, n.gui, n.web, n.app),
            ("deew", "deew-gui", "deew-web", false)
        );
        let n = platform_names(WINDOWS).unwrap();
        assert_eq!(
            (n.deew, n.gui, n.web, n.app),
            ("deew.exe", "deew-gui.exe", "deew-web.exe", false)
        );
        let n = platform_names(MAC).unwrap();
        assert_eq!(
            (n.deew, n.gui, n.web, n.app),
            ("deew-macos", "deew-gui-macos.zip", "deew-web-macos", true)
        );
        assert_eq!(n.local(Comp::Gui), "deew-gui.app");
        assert_eq!(n.local(Comp::Web), "deew-web-macos");
        let other = |os, arch| platform_names(Platform { os, arch });
        assert!(other(Os::Linux, Arch::Aarch64).is_none());
        assert!(other(Os::Windows, Arch::Aarch64).is_none());
        assert!(other(Os::Other, Arch::X86_64).is_none());
    }

    #[test]
    fn install_dir_is_the_exe_dir() {
        assert_eq!(
            install_dir(Path::new("/tools/deew")),
            Some(PathBuf::from("/tools"))
        );
    }

    // --- local state ---

    #[test]
    fn plist_versions() {
        let plist = "<dict>\n<key>CFBundleVersion</key>\n<string>7</string>\n<key>CFBundleShortVersionString</key>\n\t<string> 1.0.1 </string>\n</dict>";
        assert_eq!(plist_version(plist), Some("1.0.1".to_string()));
        assert_eq!(plist_version("<dict></dict>"), None);
        assert_eq!(
            plist_version("<key>CFBundleShortVersionString</key><string></string>"),
            None
        );
    }

    #[test]
    fn installed_detection() {
        let dir = tempfile::tempdir().unwrap();
        let lin = platform_names(LINUX).unwrap();
        assert_eq!(local_state(&lin, Comp::Gui, dir.path()), Local::Missing);
        std::fs::write(dir.path().join("deew-gui"), b"x").unwrap();
        assert_eq!(
            local_state(&lin, Comp::Gui, dir.path()),
            Local::Binary(None)
        );
        std::fs::write(
            dir.path().join("deew-gui"),
            b"junk\0deew-gui-version=1.2.3\0junk",
        )
        .unwrap();
        assert_eq!(
            local_state(&lin, Comp::Gui, dir.path()),
            Local::Binary(Some("1.2.3".into()))
        );
        // the web marker is not the gui marker
        std::fs::write(
            dir.path().join("deew-web"),
            b"junk\0deew-gui-version=1.2.3\0junk",
        )
        .unwrap();
        assert_eq!(
            local_state(&lin, Comp::Web, dir.path()),
            Local::Binary(None)
        );

        let mac = platform_names(MAC).unwrap();
        assert_eq!(local_state(&mac, Comp::Gui, dir.path()), Local::Missing);
        std::fs::write(dir.path().join("deew-gui-macos"), b"x").unwrap();
        assert_eq!(local_state(&mac, Comp::Gui, dir.path()), Local::OldBare);
        let contents = dir.path().join("deew-gui.app/Contents");
        std::fs::create_dir_all(&contents).unwrap();
        assert_eq!(local_state(&mac, Comp::Gui, dir.path()), Local::App(None));
        std::fs::write(
            contents.join("Info.plist"),
            "<key>CFBundleShortVersionString</key><string>1.0.1</string>",
        )
        .unwrap();
        assert_eq!(
            local_state(&mac, Comp::Gui, dir.path()),
            Local::App(Some("1.0.1".into()))
        );
    }

    // --- plan ---

    #[test]
    fn plan_compares_digests() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("deew");
        std::fs::write(&exe, b"deew new").unwrap();
        std::fs::write(dir.path().join("deew-gui"), b"gui old").unwrap();
        let assets = [
            asset("deew", b"deew new"),
            asset("deew-gui", b"gui new"),
            asset("deew-web", b"web new"),
        ];
        let names = platform_names(LINUX).unwrap();
        let items = plan_for(names, dir.path(), &exe, false, &assets, &versions());
        assert_eq!(
            actions(&items),
            [Action::UpToDate, Action::Update, Action::Skip]
        );
        assert_eq!(items[0].current, DEEW_VERSION);
        assert_eq!(items[0].latest, "9.9.9");
        assert_eq!(items[1].current, "unknown");
        assert_eq!(items[1].latest, "2.0.0");
        assert_eq!(items[2].current, "missing");
        assert!(items[1].asset.is_some() && items[0].asset.is_none());

        let items = plan_for(names, dir.path(), &exe, true, &assets, &versions());
        assert_eq!(
            actions(&items),
            [Action::UpToDate, Action::Update, Action::Install]
        );
    }

    #[test]
    fn plan_without_versions_or_assets() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("deew");
        std::fs::write(&exe, b"old").unwrap();
        let names = platform_names(LINUX).unwrap();
        let items = plan_for(
            names,
            dir.path(),
            &exe,
            true,
            &[asset("deew", b"new")],
            &Versions::default(),
        );
        assert_eq!(
            actions(&items),
            [Action::UpToDate, Action::Unavailable, Action::Unavailable]
        );
        assert_eq!(items[1].latest, "?");
        // an asset without a digest can't be verified
        let mut no_digest = asset("deew", b"new");
        no_digest.digest = None;
        let items = plan_for(
            names,
            dir.path(),
            &exe,
            false,
            &[no_digest],
            &Versions::default(),
        );
        assert_eq!(items[0].action, Action::Unavailable);
    }

    #[test]
    fn marker_scan_across_chunks() {
        let mut data = vec![b'a'; 100];
        data.extend_from_slice(b"\0deew-web-version=1.2.3\0");
        data.extend_from_slice(&[b'b'; 50]);
        let prefix = b"\0deew-web-version=";
        for chunk in [1, 2, 7, 13, 100, 105, 110, 4096] {
            assert_eq!(
                scan_marker(&data[..], prefix, chunk),
                Some("1.2.3".to_string()),
                "chunk {chunk}"
            );
        }
        assert_eq!(scan_marker(&b"nothing here"[..], prefix, 4), None);
        assert_eq!(scan_marker(&b"\0deew-web-version=1.2"[..], prefix, 4), None);
    }

    #[test]
    fn plan_of_bare_binaries_by_marker_and_digest() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("deew");
        std::fs::write(&exe, b"deew new").unwrap();
        let names = platform_names(LINUX).unwrap();
        let gui = dir.path().join("deew-gui");
        let web = dir.path().join("deew-web");
        let mark = |name: &str, v: &str| format!("xx\0{name}-version={v}\0yy").into_bytes();
        let assets = [
            asset("deew", b"deew new"),
            asset("deew-gui", b"gui release"),
            asset("deew-web", b"web release"),
        ];
        // marker older / equal / newer than 2.0.0 (gui) and the web version
        let v = versions();
        let web_latest = v.web.clone().unwrap();
        for (gui_v, want) in [
            ("1.9.9", Action::Update),
            ("2.0.0", Action::UpToDate),
            ("2.0.1", Action::Newer),
        ] {
            std::fs::write(&gui, mark("deew-gui", gui_v)).unwrap();
            std::fs::write(&web, mark("deew-web", &web_latest)).unwrap();
            let items = plan_for(names, dir.path(), &exe, false, &assets, &v);
            assert_eq!(items[1].current, gui_v);
            assert_eq!(items[1].action, want);
            assert_eq!(items[2].current, web_latest);
            assert_eq!(items[2].action, Action::UpToDate);
        }
        // no marker, same bytes as the release: the release version
        std::fs::write(&gui, b"gui release").unwrap();
        let items = plan_for(names, dir.path(), &exe, false, &assets, &v);
        assert_eq!(items[1].current, "2.0.0");
        assert_eq!(items[1].action, Action::UpToDate);
        // no marker, other bytes: unknown and outdated
        std::fs::write(&gui, b"gui other").unwrap();
        let items = plan_for(names, dir.path(), &exe, false, &assets, &v);
        assert_eq!(items[1].current, "unknown");
        assert_eq!(items[1].action, Action::Update);
    }

    #[test]
    fn plan_of_the_mac_app() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("deew-macos");
        std::fs::write(&exe, b"deew").unwrap();
        let contents = dir.path().join("deew-gui.app/Contents");
        std::fs::create_dir_all(&contents).unwrap();
        let names = platform_names(MAC).unwrap();
        let assets = [
            asset("deew-macos", b"deew"),
            asset("deew-gui-macos.zip", b"zip"),
        ];
        // version unknown: outdated
        let items = plan_for(names, dir.path(), &exe, false, &assets, &versions());
        assert_eq!(items[1].action, Action::Update);
        assert_eq!(items[1].current, "unknown");
        std::fs::write(
            contents.join("Info.plist"),
            "<key>CFBundleShortVersionString</key><string>1.0.0</string>",
        )
        .unwrap();
        let items = plan_for(names, dir.path(), &exe, false, &assets, &versions());
        assert_eq!(items[1].action, Action::Update);
        assert_eq!(items[1].current, "1.0.0");
        // no versions.json: outdated
        let items = plan_for(
            names,
            dir.path(),
            &exe,
            false,
            &assets,
            &Versions::default(),
        );
        assert_eq!(items[1].action, Action::Update);
        // same version
        let mut v = versions();
        v.gui = Some("1.0.0".into());
        let items = plan_for(names, dir.path(), &exe, false, &assets, &v);
        assert_eq!(items[1].action, Action::UpToDate);
        assert!(items[1].asset.is_none());
    }

    #[test]
    fn plan_replaces_the_old_bare_mac_gui() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("deew-macos");
        std::fs::write(&exe, b"deew").unwrap();
        std::fs::write(dir.path().join("deew-gui-macos"), b"old gui").unwrap();
        let names = platform_names(MAC).unwrap();
        let assets = [asset("deew-gui-macos.zip", b"zip")];
        let items = plan_for(names, dir.path(), &exe, false, &assets, &versions());
        assert_eq!(items[1].action, Action::Update);
        assert_eq!(
            items[1].remove_after,
            Some(dir.path().join("deew-gui-macos"))
        );
    }

    #[test]
    fn version_comparison() {
        let c = |a: &str, b: &str| compare_versions(Some(a), Some(b));
        assert_eq!(c("5.5.4", "5.5.3"), Some(Ordering::Greater));
        assert_eq!(c("5.5.4", "v5.5.4"), Some(Ordering::Equal));
        assert_eq!(c("5.5", "5.5.0"), Some(Ordering::Equal));
        assert_eq!(c("5.5.4", "5.10.0"), Some(Ordering::Less));
        assert_eq!(c("5.5.4-dev", "5.5.4"), None);
        assert_eq!(c("", "5.5.4"), None);
        assert_eq!(compare_versions(None, Some("1.0.0")), None);
    }

    #[test]
    fn plan_never_downgrades() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("deew");
        std::fs::write(&exe, b"self built").unwrap();
        let assets = [asset("deew", b"release")];
        let names = platform_names(LINUX).unwrap();
        let plan = |tag: &str| {
            make_plan(&Context {
                names,
                dir: dir.path(),
                gui_dir: dir.path(),
                exe: &exe,
                deew_version: "5.5.4",
                all: false,
                tag,
                assets: &assets,
                versions: &Versions::default(),
            })
        };
        // newer than the release: left alone, nothing to download
        let items = plan("v5.5.3");
        assert_eq!(items[0].action, Action::Newer);
        assert!(items[0].asset.is_none() && !items[0].action.todo());
        // same version, different digest: up to date
        assert_eq!(plan("v5.5.4")[0].action, Action::UpToDate);
        // older: update
        let items = plan("v5.6.0");
        assert_eq!(items[0].action, Action::Update);
        assert!(items[0].asset.is_some());
        // unparsable tag: the digest decides
        assert_eq!(plan("nightly")[0].action, Action::Update);
        std::fs::write(&exe, b"release").unwrap();
        assert_eq!(plan("nightly")[0].action, Action::UpToDate);
    }

    #[test]
    fn plan_of_the_mac_app_newer() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("deew-macos");
        std::fs::write(&exe, b"deew").unwrap();
        let contents = dir.path().join("deew-gui.app/Contents");
        std::fs::create_dir_all(&contents).unwrap();
        std::fs::write(
            contents.join("Info.plist"),
            "<key>CFBundleShortVersionString</key><string>3.0.0</string>",
        )
        .unwrap();
        let names = platform_names(MAC).unwrap();
        let assets = [asset("deew-gui-macos.zip", b"zip")];
        let items = plan_for(names, dir.path(), &exe, false, &assets, &versions());
        assert_eq!(items[1].action, Action::Newer);
        assert!(items[1].asset.is_none());
    }

    #[test]
    fn versions_json() {
        let v = parse_versions(r#"{"deew": "5.5.4", "deew-gui": "1.0.1", "deew-web": "1.0.1"}"#)
            .unwrap();
        assert_eq!(v.of(Comp::Deew), Some("5.5.4"));
        assert_eq!(v.of(Comp::Gui), Some("1.0.1"));
        assert_eq!(v.of(Comp::Web), Some("1.0.1"));
        assert!(parse_versions("nope").is_none());
    }

    #[test]
    fn table_has_a_row_per_component() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("deew");
        std::fs::write(&exe, b"x").unwrap();
        let names = platform_names(LINUX).unwrap();
        let items = plan_for(names, dir.path(), &exe, false, &[], &versions());
        let lines = table_lines(&items, false);
        assert_eq!(lines.len(), 4);
        assert!(crate::markup::strip(&lines[0]).starts_with("component"));
        assert!(crate::markup::strip(&lines[2]).starts_with("deew-gui"));
    }

    // --- swap ---

    fn read(path: &Path) -> String {
        std::fs::read_to_string(path).unwrap()
    }

    #[test]
    fn swap_replaces_in_place() {
        let dir = tempfile::tempdir().unwrap();
        let (dest, new) = (dir.path().join("deew"), dir.path().join("deew.new"));
        std::fs::write(&dest, "old").unwrap();
        std::fs::write(&new, "new").unwrap();
        swap_file(&dest, &new, false).unwrap();
        assert_eq!(read(&dest), "new");
        assert!(!new.exists());
        assert!(!dir.path().join("deew.old").exists());
    }

    #[test]
    fn windows_style_swap_keeps_the_old_file() {
        let dir = tempfile::tempdir().unwrap();
        let (dest, new) = (dir.path().join("deew.exe"), dir.path().join("deew.exe.new"));
        std::fs::write(&dest, "old").unwrap();
        std::fs::write(&new, "new").unwrap();
        swap_file(&dest, &new, true).unwrap();
        assert_eq!(read(&dest), "new");
        assert_eq!(read(&dir.path().join("deew.exe.old")), "old");
        // the next start removes it
        remove_leftovers(dir.path());
        assert!(!dir.path().join("deew.exe.old").exists());
        assert_eq!(read(&dest), "new");
    }

    #[test]
    fn windows_style_swap_rolls_back() {
        let dir = tempfile::tempdir().unwrap();
        let (dest, new) = (dir.path().join("deew.exe"), dir.path().join("missing.new"));
        std::fs::write(&dest, "old").unwrap();
        // the second rename fails: the old file is back
        assert!(swap_file(&dest, &new, true).is_err());
        assert_eq!(read(&dest), "old");
        assert!(!dir.path().join("deew.exe.old").exists());
    }

    #[test]
    fn swap_installs_a_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let (dest, new) = (
            dir.path().join("deew-web.exe"),
            dir.path().join("deew-web.exe.new"),
        );
        std::fs::write(&new, "new").unwrap();
        swap_file(&dest, &new, true).unwrap();
        assert_eq!(read(&dest), "new");
        assert!(!dir.path().join("deew-web.exe.old").exists());
    }

    #[test]
    fn app_swap_and_rollback() {
        let dir = tempfile::tempdir().unwrap();
        let (dest, new) = (
            dir.path().join("deew-gui.app"),
            dir.path().join("deew-gui.app.new"),
        );
        std::fs::create_dir_all(dest.join("Contents")).unwrap();
        std::fs::write(dest.join("Contents/v"), "old").unwrap();
        std::fs::create_dir_all(new.join("Contents")).unwrap();
        std::fs::write(new.join("Contents/v"), "new").unwrap();
        swap_app(&dest, &new).unwrap();
        assert_eq!(read(&dest.join("Contents/v")), "new");
        assert!(!new.exists());
        assert!(!dir.path().join("deew-gui.app.old").exists());

        // a failing second rename restores the app
        assert!(swap_app(&dest, &dir.path().join("nothing.new")).is_err());
        assert_eq!(read(&dest.join("Contents/v")), "new");
        assert!(!dir.path().join("deew-gui.app.old").exists());
    }

    // --- extraction ---

    fn app_zip(path: &Path, with_binary: bool) {
        use zip::write::SimpleFileOptions;
        let file = File::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let dir = opts.unix_permissions(0o755);
        zip.add_directory("deew-gui.app/", dir).unwrap();
        zip.add_directory("deew-gui.app/Contents/", dir).unwrap();
        zip.add_directory("deew-gui.app/Contents/MacOS/", dir)
            .unwrap();
        if with_binary {
            zip.start_file("deew-gui.app/Contents/MacOS/deew-gui", dir)
                .unwrap();
            zip.write_all(b"binary").unwrap();
        }
        zip.start_file(
            "deew-gui.app/Contents/Info.plist",
            opts.unix_permissions(0o644),
        )
        .unwrap();
        zip.write_all(b"plist").unwrap();
        zip.add_symlink("deew-gui.app/Contents/link", "Info.plist", opts)
            .unwrap();
        zip.start_file("__MACOSX/junk", opts).unwrap();
        zip.write_all(b"junk").unwrap();
        zip.finish().unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn app_extraction_keeps_modes_and_links() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let zip = dir.path().join("app.zip");
        app_zip(&zip, true);
        let dest = dir.path().join("deew-gui.app.new");
        extract_app(&zip, &dest, &|_, _| {}).unwrap();
        let mode = |p: &str| {
            std::fs::metadata(dest.join(p))
                .unwrap()
                .permissions()
                .mode()
                & 0o777
        };
        assert_eq!(mode("Contents/MacOS/deew-gui"), 0o755);
        assert_eq!(mode("Contents/Info.plist"), 0o644);
        assert_eq!(mode("Contents/MacOS"), 0o755);
        assert_eq!(read(&dest.join("Contents/MacOS/deew-gui")), "binary");
        let link = dest.join("Contents/link");
        assert_eq!(std::fs::read_link(&link).unwrap(), Path::new("Info.plist"));
        assert_eq!(read(&link), "plist");
        assert!(!dest.join("junk").exists() && !dir.path().join("__MACOSX").exists());
    }

    #[test]
    fn app_extraction_needs_the_binary() {
        let dir = tempfile::tempdir().unwrap();
        let zip = dir.path().join("app.zip");
        app_zip(&zip, false);
        let dest = dir.path().join("deew-gui.app.new");
        let err = extract_app(&zip, &dest, &|_, _| {}).unwrap_err();
        assert!(err.contains("Contents/MacOS/deew-gui"), "{err}");
    }

    // --- cleanup ---

    #[test]
    fn startup_cleanup_removes_only_the_leftovers() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "deew.exe.old",
            "deew-gui.exe.old",
            "deew-web.old",
            "deew",
            "config.toml",
        ] {
            std::fs::write(dir.path().join(name), "x").unwrap();
        }
        std::fs::create_dir_all(dir.path().join("deew-gui.app.old/Contents")).unwrap();
        std::fs::create_dir_all(dir.path().join("deew-gui.app/Contents")).unwrap();
        remove_leftovers(dir.path());
        let mut left: Vec<String> = std::fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(left, ["config.toml", "deew", "deew-gui.app"]);
        // a missing folder is fine
        remove_leftovers(&dir.path().join("nope"));
    }

    #[test]
    fn app_leftover_cleanup_is_exact() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("deew-gui.app.old/Contents")).unwrap();
        std::fs::create_dir_all(dir.path().join("deew-gui.app/Contents")).unwrap();
        std::fs::create_dir_all(dir.path().join("other.app.old")).unwrap();
        remove_app_leftover(dir.path());
        assert!(!dir.path().join("deew-gui.app.old").exists());
        assert!(dir.path().join("deew-gui.app").exists());
        assert!(dir.path().join("other.app.old").exists());
    }

    // --- where the app lives ---

    struct Mac {
        _root: tempfile::TempDir,
        exe_dir: PathBuf,
        dirs: AppDirs,
    }

    fn mac(in_support: bool) -> Mac {
        let root = tempfile::tempdir().unwrap();
        let p = root.path();
        let (system, user) = (p.join("Applications"), p.join("home/Applications"));
        let support = p.join("home/Library/Application Support/deew");
        let portable = p.join("portable");
        for d in [&system, &user, &support, &portable] {
            std::fs::create_dir_all(d).unwrap();
        }
        Mac {
            exe_dir: if in_support {
                support.clone()
            } else {
                portable
            },
            dirs: AppDirs {
                system,
                user: Some(user),
                support: Some(support),
            },
            _root: root,
        }
    }

    fn make_app(dir: &Path) {
        std::fs::create_dir_all(dir.join("deew-gui.app/Contents")).unwrap();
    }

    #[test]
    fn app_lookup_order() {
        let names = platform_names(MAC).unwrap();
        let m = mac(true);
        let find = |m: &Mac| gui_dir(&names, &m.exe_dir, &m.dirs, &|_| true);
        // missing: installs in /Applications
        assert_eq!(find(&m), m.dirs.system);
        make_app(m.dirs.user.as_ref().unwrap());
        assert_eq!(find(&m), *m.dirs.user.as_ref().unwrap());
        make_app(&m.dirs.system);
        assert_eq!(find(&m), m.dirs.system);
        // the portable folder wins
        make_app(&m.exe_dir);
        assert_eq!(find(&m), m.exe_dir);
    }

    #[test]
    fn old_bare_gui_counts_as_portable() {
        let names = platform_names(MAC).unwrap();
        let m = mac(false);
        make_app(&m.dirs.system);
        std::fs::write(m.exe_dir.join("deew-gui-macos"), b"x").unwrap();
        assert_eq!(gui_dir(&names, &m.exe_dir, &m.dirs, &|_| true), m.exe_dir);
    }

    #[test]
    fn app_install_location() {
        let names = platform_names(MAC).unwrap();
        let m = mac(true);
        let user = m.dirs.user.clone().unwrap();
        assert_eq!(
            gui_dir(&names, &m.exe_dir, &m.dirs, &|_| true),
            m.dirs.system
        );
        assert_eq!(gui_dir(&names, &m.exe_dir, &m.dirs, &|_| false), user);
        // a portable folder installs next to deew
        let m = mac(false);
        assert_eq!(gui_dir(&names, &m.exe_dir, &m.dirs, &|_| true), m.exe_dir);
    }

    #[test]
    fn other_platforms_keep_the_exe_dir() {
        let m = mac(true);
        make_app(&m.dirs.system);
        for platform in [LINUX, WINDOWS] {
            let names = platform_names(platform).unwrap();
            assert_eq!(gui_dir(&names, &m.exe_dir, &m.dirs, &|_| true), m.exe_dir);
        }
    }

    #[test]
    fn plan_and_table_show_the_app_folder() {
        let m = mac(true);
        make_app(&m.dirs.system);
        let exe = m.exe_dir.join("deew-macos");
        std::fs::write(&exe, b"deew").unwrap();
        let names = platform_names(MAC).unwrap();
        let assets = [asset("deew-gui-macos.zip", b"zip")];
        let items = make_plan(&Context {
            names,
            dir: &m.exe_dir,
            gui_dir: &m.dirs.system,
            exe: &exe,
            deew_version: DEEW_VERSION,
            all: false,
            tag: "v9.9.9",
            assets: &assets,
            versions: &versions(),
        });
        assert_eq!(items[1].action, Action::Update);
        assert_eq!(items[0].place, None);
        assert_eq!(items[1].place.as_deref(), Some(m.dirs.system.as_path()));
        let lines = table_lines(&items, false);
        let gui = crate::markup::strip(&lines[2]);
        assert!(gui.starts_with(&format!("deew-gui ({})", m.dirs.system.display())));
        // the columns line up
        let col = |l: &str| l.find('│');
        assert_eq!(col(&crate::markup::strip(&lines[1])), col(&gui));
    }

    #[test]
    fn writable_probe() {
        let dir = tempfile::tempdir().unwrap();
        assert!(check_writable(dir.path()).is_ok());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
        let err = check_writable(&dir.path().join("nope")).unwrap_err();
        assert!(err.contains("nope") && err.contains("administrator"));
    }
}
