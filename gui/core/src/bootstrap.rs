//! Downloads the deew CLI from the latest GitHub release into the deew
//! folder, for the Utilities "Update" entries when no CLI is present yet.
//!
//! The release's `digest` (`sha256:<hex>`) of the asset is the only
//! trust anchor: a release without one is never installed. The network part
//! is behind [`Fetch`] so the file handling can be tested without it.

use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

const RELEASE_URL: &str = "https://api.github.com/repos/pcrlnd/deew/releases/latest";
const USER_AGENT: &str = "deew-gui";
/// Marker error of a download stopped by the user.
pub const CANCELLED: &str = "stopped";

/// The release asset (which is also the installed file name) for a platform
/// (`std::env::consts` values).
pub fn asset_for(os: &str, arch: &str) -> Result<&'static str, String> {
    match (os, arch) {
        ("linux", "x86_64") => Ok("deew"),
        ("windows", "x86_64") => Ok("deew.exe"),
        ("macos", "x86_64" | "aarch64") => Ok("deew-macos"),
        _ => Err("no deew build for this platform".into()),
    }
}

/// The asset for the platform this program runs on.
pub fn current_asset() -> Result<&'static str, String> {
    asset_for(std::env::consts::OS, std::env::consts::ARCH)
}

/// A release asset to download.
#[derive(Debug, PartialEq, Eq)]
pub struct Release {
    pub version: String,
    pub url: String,
    /// Lowercase hex SHA-256 of the asset.
    pub sha256: String,
}

/// `sha256:<64 hex digits>` into the lowercase hex.
pub fn parse_digest(digest: &str) -> Option<String> {
    let hex = digest.strip_prefix("sha256:")?;
    (hex.len() == 64 && hex.bytes().all(|b| b.is_ascii_hexdigit()))
        .then(|| hex.to_ascii_lowercase())
}

/// Picks `asset` out of the JSON of a GitHub release.
pub fn parse_release(json: &str, asset: &str) -> Result<Release, String> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("cannot read the latest release: {e}"))?;
    let version = value
        .get("tag_name")
        .and_then(|v| v.as_str())
        .ok_or("the latest release has no version")?;
    let version = version.strip_prefix('v').unwrap_or(version).to_string();
    let found = value
        .get("assets")
        .and_then(|a| a.as_array())
        .and_then(|assets| {
            assets
                .iter()
                .find(|a| a.get("name").and_then(|n| n.as_str()) == Some(asset))
        })
        .ok_or_else(|| format!("the latest release has no {asset}"))?;
    let url = found
        .get("browser_download_url")
        .and_then(|u| u.as_str())
        .ok_or_else(|| format!("the latest release has no download link for {asset}"))?
        .to_string();
    let sha256 = found
        .get("digest")
        .and_then(|d| d.as_str())
        .and_then(parse_digest)
        .ok_or_else(|| format!("GitHub gives no SHA-256 for {asset}, refusing to install it"))?;
    Ok(Release {
        version,
        url,
        sha256,
    })
}

/// The network side of the download.
pub trait Fetch {
    /// The JSON of the latest release.
    fn latest_release(&self) -> Result<String, String>;
    /// Writes the body of `url` into `out`; `progress(done, total)` returns
    /// false to give up (then the error is [`CANCELLED`]).
    fn download(
        &self,
        url: &str,
        out: &mut dyn Write,
        progress: &mut dyn FnMut(u64, Option<u64>) -> bool,
    ) -> Result<(), String>;
}

/// [`Fetch`] over HTTPS with ureq.
pub struct Github;

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(15)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        .accept_encoding("identity")
        .build()
        .into()
}

fn get(url: &str) -> Result<ureq::http::Response<ureq::Body>, String> {
    agent()
        .get(url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/vnd.github+json, */*")
        .call()
        .map_err(|e| e.to_string())
}

impl Fetch for Github {
    fn latest_release(&self) -> Result<String, String> {
        get(RELEASE_URL)
            .and_then(|mut r| r.body_mut().read_to_string().map_err(|e| e.to_string()))
            .map_err(|e| format!("cannot read the latest release: {e}"))
    }

    fn download(
        &self,
        url: &str,
        out: &mut dyn Write,
        progress: &mut dyn FnMut(u64, Option<u64>) -> bool,
    ) -> Result<(), String> {
        let mut response = get(url).map_err(|e| format!("download failed: {e}"))?;
        let total = response.body().content_length();
        let mut reader = response.body_mut().as_reader();
        let mut buf = vec![0u8; 64 * 1024];
        let mut done = 0u64;
        if !progress(0, total) {
            return Err(CANCELLED.into());
        }
        loop {
            let n = reader
                .read(&mut buf)
                .map_err(|e| format!("download failed: {e}"))?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n])
                .map_err(|e| format!("cannot write the download: {e}"))?;
            done += n as u64;
            if !progress(done, total) {
                return Err(CANCELLED.into());
            }
        }
        if let Some(total) = total
            && done != total
        {
            return Err(format!("download incomplete ({done} of {total} bytes)"));
        }
        Ok(())
    }
}

/// Forwards to the file and the hash.
struct HashingWriter<W: Write> {
    inner: W,
    hasher: Sha256,
}

impl<W: Write> Write for HashingWriter<W> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let n = self.inner.write(buf)?;
        self.hasher.update(&buf[..n]);
        Ok(n)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.flush()
    }
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Downloads `asset` of the latest release into `dir` (created when
/// missing), verifies it and installs it as `dir/asset`. Progress lines go to
/// `say` (terminal text, `\r\n` line ends). `cancel` stops the download
/// (error [`CANCELLED`]); no `.new` file is left behind on any failure.
pub fn install_cli(
    fetch: &dyn Fetch,
    dir: &Path,
    asset: &str,
    cancel: &AtomicBool,
    say: &mut dyn FnMut(&str),
) -> Result<PathBuf, String> {
    if cancel.load(Ordering::SeqCst) {
        return Err(CANCELLED.into());
    }
    let release = parse_release(&fetch.latest_release()?, asset)?;
    say(&format!(
        "deew CLI not found, downloading {asset} {} from GitHub\u{2026}\r\n",
        release.version
    ));
    std::fs::create_dir_all(dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;
    let target = dir.join(asset);
    let part = dir.join(format!("{asset}.new"));
    let result = download_and_verify(fetch, &release, &part, cancel, say)
        .and_then(|()| install_file(&part, &target));
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result?;
    say(&format!(
        "checked SHA-256, saved to {}\r\n",
        target.display()
    ));
    Ok(target)
}

fn download_and_verify(
    fetch: &dyn Fetch,
    release: &Release,
    part: &Path,
    cancel: &AtomicBool,
    say: &mut dyn FnMut(&str),
) -> Result<(), String> {
    let file = std::fs::File::create(part)
        .map_err(|e| format!("cannot create {}: {e}", part.display()))?;
    let mut out = HashingWriter {
        inner: std::io::BufWriter::new(file),
        hasher: Sha256::new(),
    };
    let mut last_shown = None;
    let mut last_time = Instant::now() - Duration::from_secs(1);
    let mut progress = |done: u64, total: Option<u64>| {
        if cancel.load(Ordering::SeqCst) {
            return false;
        }
        let shown = match total {
            Some(total) if total > 0 => Some(format!("{}%", done * 100 / total)),
            _ => None,
        };
        let finished = total == Some(done);
        if shown.is_some()
            && shown != last_shown
            && (finished || last_time.elapsed() >= Duration::from_millis(100))
        {
            say(&format!("\r{}", shown.as_deref().unwrap_or_default()));
            last_shown = shown;
            last_time = Instant::now();
        }
        true
    };
    fetch.download(&release.url, &mut out, &mut progress)?;
    if last_shown.is_some() {
        say("\r\n");
    }
    out.flush()
        .map_err(|e| format!("cannot write {}: {e}", part.display()))?;
    let actual = hex(&out.hasher.finalize());
    if actual != release.sha256 {
        return Err("SHA-256 mismatch, the download is corrupt".into());
    }
    Ok(())
}

fn install_file(part: &Path, target: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(part, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("cannot make {} executable: {e}", part.display()))?;
    }
    std::fs::rename(part, target).map_err(|e| format!("cannot install {}: {e}", target.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn assets_per_platform() {
        assert_eq!(asset_for("linux", "x86_64"), Ok("deew"));
        assert_eq!(asset_for("windows", "x86_64"), Ok("deew.exe"));
        assert_eq!(asset_for("macos", "x86_64"), Ok("deew-macos"));
        assert_eq!(asset_for("macos", "aarch64"), Ok("deew-macos"));
        for (os, arch) in [
            ("linux", "aarch64"),
            ("windows", "aarch64"),
            ("freebsd", "x86_64"),
        ] {
            assert_eq!(
                asset_for(os, arch),
                Err("no deew build for this platform".to_string())
            );
        }
    }

    const HEX: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";

    #[test]
    fn digests() {
        assert_eq!(parse_digest(&format!("sha256:{HEX}")), Some(HEX.into()));
        assert_eq!(
            parse_digest(&format!("sha256:{}", HEX.to_uppercase())),
            Some(HEX.into())
        );
        assert_eq!(parse_digest("sha256:abc"), None);
        assert_eq!(parse_digest(&format!("sha1:{HEX}")), None);
        assert_eq!(parse_digest(HEX), None);
        assert_eq!(parse_digest(&format!("sha256:{}g", &HEX[1..])), None);
    }

    fn release_json(digest: Option<&str>) -> String {
        let digest = digest
            .map(|d| format!(r#","digest":"{d}""#))
            .unwrap_or_default();
        format!(
            r#"{{"tag_name":"v5.6.0","assets":[
            {{"name":"deew.exe","browser_download_url":"https://x/deew.exe"}},
            {{"name":"deew","browser_download_url":"https://x/deew"{digest}}}]}}"#
        )
    }

    #[test]
    fn release_parsing() {
        let json = release_json(Some(&format!("sha256:{HEX}")));
        assert_eq!(
            parse_release(&json, "deew"),
            Ok(Release {
                version: "5.6.0".into(),
                url: "https://x/deew".into(),
                sha256: HEX.into()
            })
        );
        // no digest: never installed unverified
        assert!(
            parse_release(&release_json(None), "deew")
                .unwrap_err()
                .contains("SHA-256")
        );
        assert!(
            parse_release(&json, "deew-macos")
                .unwrap_err()
                .contains("no deew-macos")
        );
        assert!(parse_release("nope", "deew").is_err());
    }

    struct Fake {
        body: Vec<u8>,
        digest: String,
        fail_after: Option<usize>,
    }

    impl Fetch for Fake {
        fn latest_release(&self) -> Result<String, String> {
            Ok(release_json(Some(&format!("sha256:{}", self.digest))))
        }
        fn download(
            &self,
            _url: &str,
            out: &mut dyn Write,
            progress: &mut dyn FnMut(u64, Option<u64>) -> bool,
        ) -> Result<(), String> {
            let total = Some(self.body.len() as u64);
            let mut done = 0;
            for chunk in self.body.chunks(4) {
                if self.fail_after.is_some_and(|n| done >= n) {
                    return Err("download failed: boom".into());
                }
                out.write_all(chunk).map_err(|e| e.to_string())?;
                done += chunk.len();
                if !progress(done as u64, total) {
                    return Err(CANCELLED.into());
                }
            }
            Ok(())
        }
    }

    fn sha(bytes: &[u8]) -> String {
        hex(&Sha256::digest(bytes))
    }

    fn leftovers(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn installs_a_verified_download() {
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("a/b");
        let body = b"hello world, this is deew".to_vec();
        let fake = Fake {
            digest: sha(&body),
            body: body.clone(),
            fail_after: None,
        };
        let mut text = String::new();
        let path = install_cli(&fake, &dir, "deew", &AtomicBool::new(false), &mut |s| {
            text.push_str(s)
        })
        .unwrap();
        assert_eq!(path, dir.join("deew"));
        assert_eq!(std::fs::read(&path).unwrap(), body);
        assert_eq!(leftovers(&dir), ["deew"]);
        assert!(
            text.starts_with("deew CLI not found, downloading deew 5.6.0 from GitHub\u{2026}\r\n")
        );
        assert!(text.contains("100%"));
        assert!(text.ends_with(&format!("checked SHA-256, saved to {}\r\n", path.display())));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                std::fs::metadata(&path).unwrap().permissions().mode() & 0o111,
                0o111
            );
        }
    }

    #[test]
    fn digest_mismatch_leaves_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let fake = Fake {
            digest: HEX.into(),
            body: b"other bytes".to_vec(),
            fail_after: None,
        };
        let err = install_cli(
            &fake,
            tmp.path(),
            "deew",
            &AtomicBool::new(false),
            &mut |_| {},
        )
        .unwrap_err();
        assert!(err.contains("mismatch"));
        assert!(leftovers(tmp.path()).is_empty());
    }

    #[test]
    fn failure_and_cancel_clean_up() {
        let tmp = tempfile::tempdir().unwrap();
        let body = b"0123456789".to_vec();
        let fake = Fake {
            digest: sha(&body),
            body,
            fail_after: Some(4),
        };
        let err = install_cli(
            &fake,
            tmp.path(),
            "deew",
            &AtomicBool::new(false),
            &mut |_| {},
        )
        .unwrap_err();
        assert!(err.contains("boom"));
        assert!(leftovers(tmp.path()).is_empty());

        let fake = Fake {
            digest: HEX.into(),
            body: b"0123456789".to_vec(),
            fail_after: None,
        };
        let cancel = AtomicBool::new(false);
        let mut calls = 0;
        let err = install_cli(&fake, tmp.path(), "deew", &cancel, &mut |_| {
            calls += 1;
            cancel.store(true, Ordering::SeqCst);
        })
        .unwrap_err();
        assert_eq!(err, CANCELLED);
        assert!(calls >= 1);
        assert!(leftovers(tmp.path()).is_empty());
    }

    /// Real GitHub, real download: `cargo test -p deew-gui-core -- --ignored`.
    #[test]
    #[ignore]
    fn downloads_the_real_cli() {
        let dir = std::env::var("DEEW_BOOTSTRAP_DIR").expect("set DEEW_BOOTSTRAP_DIR");
        let asset = current_asset().unwrap();
        let path = install_cli(
            &Github,
            Path::new(&dir),
            asset,
            &AtomicBool::new(false),
            &mut |s| print!("{s}"),
        )
        .unwrap();
        println!("{}", path.display());
        assert!(path.is_file());
    }
}
