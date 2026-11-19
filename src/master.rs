//! Dolby Atmos master inputs (ADM BWF and DAMF): DEE reads them directly,
//! so they are found by their headers and never go through ffprobe, ffmpeg
//! or truehdd.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Which kind of Atmos master a file is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MasterKind {
    /// ADM BWF (a RIFF/RF64/BW64 wave file with `chna` and `axml` chunks).
    Adm,
    /// DAMF (`.atmos` with its `.atmos.audio` and `.atmos.metadata`).
    Damf,
}

impl MasterKind {
    /// How the source is named in the summary.
    pub fn label(self) -> &'static str {
        match self {
            MasterKind::Adm => "ADM BWF Atmos master",
            MasterKind::Damf => "DAMF Atmos master",
        }
    }
}

/// What deew knows about a master without running a tool.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Master {
    pub kind: MasterKind,
    /// Seconds, if the header tells it.
    pub duration: Option<f64>,
}

/// Longest chain of chunks that is walked: a real file has a handful.
const MAX_CHUNKS: usize = 4096;

/// What an ADM BWF header tells.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AdmInfo {
    pub duration: Option<f64>,
}

fn read_array<const N: usize>(r: &mut impl Read) -> Option<[u8; N]> {
    let mut buf = [0u8; N];
    r.read_exact(&mut buf).ok()?;
    Some(buf)
}

fn u32_le(b: &[u8]) -> u32 {
    u32::from_le_bytes([b[0], b[1], b[2], b[3]])
}

fn u64_le(b: &[u8]) -> u64 {
    let mut a = [0u8; 8];
    a.copy_from_slice(&b[..8]);
    u64::from_le_bytes(a)
}

/// Walk the chunks of a RIFF / RF64 / BW64 wave file by their headers (the
/// audio and the XML are never read, `axml` often follows the `data`
/// chunk). `Some` if the file has both a `chna` and an `axml` chunk.
/// A file that ends early is not ADM.
pub fn parse_adm<R: Read + Seek>(r: &mut R) -> Option<AdmInfo> {
    let head: [u8; 12] = read_array(r)?;
    let id = &head[0..4];
    if !(id == b"RIFF" || id == b"RF64" || id == b"BW64") || &head[8..12] != b"WAVE" {
        return None;
    }
    let (mut chna, mut axml) = (false, false);
    // `ds64`: the 64-bit sizes of the chunks whose size field is 0xFFFFFFFF
    let mut data_size64: Option<u64> = None;
    let mut table: Vec<([u8; 4], u64)> = Vec::new();
    let mut fmt: Option<(u32, u32)> = None; // (block align, sample rate)
    let mut data_size: Option<u64> = None;
    for _ in 0..MAX_CHUNKS {
        let Some(header) = read_array::<8>(r) else {
            break;
        };
        let mut name = [0u8; 4];
        name.copy_from_slice(&header[0..4]);
        let mut size = u64::from(u32_le(&header[4..8]));
        if size == 0xFFFF_FFFF {
            if &name == b"data" {
                if let Some(s) = data_size64 {
                    size = s;
                }
            } else if let Some((_, s)) = table.iter().find(|(n, _)| *n == name) {
                size = *s;
            }
        }
        match &name {
            b"ds64" => {
                let body = size.min(64 * 1024) as usize;
                let mut buf = vec![0u8; body];
                r.read_exact(&mut buf).ok()?;
                if buf.len() >= 28 {
                    data_size64 = Some(u64_le(&buf[8..16]));
                    let count = (u32_le(&buf[24..28]) as usize).min(4096);
                    for i in 0..count {
                        let at = 28 + i * 12;
                        if buf.len() < at + 12 {
                            break;
                        }
                        let mut n = [0u8; 4];
                        n.copy_from_slice(&buf[at..at + 4]);
                        table.push((n, u64_le(&buf[at + 4..at + 12])));
                    }
                }
                skip(r, size - body as u64 + (size & 1))?;
                continue;
            }
            b"fmt " if size >= 16 => {
                let mut buf = vec![0u8; size.min(64 * 1024) as usize];
                r.read_exact(&mut buf).ok()?;
                fmt = Some((
                    u32::from(u16::from_le_bytes([buf[12], buf[13]])),
                    u32_le(&buf[4..8]),
                ));
                skip(r, size - buf.len() as u64 + (size & 1))?;
                continue;
            }
            b"chna" => chna = true,
            b"axml" => axml = true,
            b"data" => data_size = Some(size),
            _ => {}
        }
        skip(r, size)?;
        // chunks are word aligned, the last one may lack its pad byte
        if size & 1 == 1 {
            let _ = skip(r, 1);
        }
        if chna && axml {
            break;
        }
    }
    if !(chna && axml) {
        return None;
    }
    let duration = match (fmt, data_size) {
        (Some((align, rate)), Some(bytes)) if align > 0 && rate > 0 => {
            Some(bytes as f64 / f64::from(align) / f64::from(rate))
        }
        _ => None,
    };
    Some(AdmInfo { duration })
}

/// Seek `n` bytes forward; `None` when the file is shorter. Seeking past
/// the end succeeds, so the position is checked against the length.
fn skip<R: Read + Seek>(r: &mut R, n: u64) -> Option<()> {
    let at = r.stream_position().ok()?;
    let target = at.checked_add(n)?;
    let end = r.seek(SeekFrom::End(0)).ok()?;
    if target > end {
        return None;
    }
    r.seek(SeekFrom::Start(target)).ok()?;
    Some(())
}

/// `.atmos` (case-insensitive).
pub fn is_damf_name(path: &str) -> bool {
    Path::new(path)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("atmos"))
}

/// A `.atmos.audio` / `.atmos.metadata` file that belongs to a DAMF in the
/// same folder (`dir` lists the file names): it is part of the master and
/// not an input of its own.
pub fn is_damf_part(name: &str, dir: &[String]) -> bool {
    let lower = name.to_lowercase();
    ["audio", "metadata"].iter().any(|part| {
        let suffix = format!(".atmos.{part}");
        lower.ends_with(&suffix)
            && dir.iter().any(|other| {
                other.len() == name.len() - part.len() - 1
                    && name.starts_with(other.as_str())
                    && is_damf_name(other)
            })
    })
}

/// The file names a DAMF header refers to (`metadata:` and `audio:`
/// of its presentation), the default names when it has none.
pub fn damf_parts(header: &str, atmos: &str) -> Vec<String> {
    let dir = Path::new(atmos).parent().unwrap_or_else(|| Path::new(""));
    let mut found = Vec::new();
    for key in ["metadata", "audio"] {
        let value = header.lines().find_map(|line| {
            let line = line.trim();
            let rest = line.strip_prefix(key)?.strip_prefix(':')?;
            let rest = rest.trim().trim_matches(['"', '\'']);
            (!rest.is_empty()).then(|| rest.to_string())
        });
        let path = match value {
            Some(v) => dir.join(v).to_string_lossy().into_owned(),
            None => format!("{atmos}.{key}"),
        };
        found.push(path);
    }
    found
}

/// The files of a DAMF that don't exist.
pub fn damf_missing(atmos: &str) -> Vec<String> {
    let header = std::fs::read_to_string(atmos).unwrap_or_default();
    damf_parts(&header, atmos)
        .into_iter()
        .filter(|p| !Path::new(p).is_file())
        .collect()
}

/// `Content duration (in samples): N` of `atmos_info`, at 48 kHz.
pub fn parse_atmos_info_duration(output: &str) -> Option<f64> {
    output.lines().find_map(|line| {
        let (_, value) = line.split_once("Content duration (in samples):")?;
        let samples: f64 = value.trim().parse().ok()?;
        (samples > 0.0).then(|| samples / 48000.0)
    })
}

/// DAMF duration from DEE's `atmos_info` (best effort, `None` if it is not there or fails).
pub fn damf_duration(atmos_info: &str, atmos: &str) -> Option<f64> {
    let out = std::process::Command::new(atmos_info)
        .args(["-i", atmos])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    parse_atmos_info_duration(&String::from_utf8_lossy(&out.stdout))
}

/// Is `path` an Atmos master? A DAMF is told by its name, an ADM BWF by its
/// chunks (only the headers are read). The DAMF files that are missing are
/// the `Err`.
pub fn detect(path: &str) -> Result<Option<Master>, Vec<String>> {
    if is_damf_name(path) && Path::new(path).is_file() {
        let missing = damf_missing(path);
        if !missing.is_empty() {
            return Err(missing);
        }
        return Ok(Some(Master {
            kind: MasterKind::Damf,
            duration: None,
        }));
    }
    let Ok(mut file) = std::fs::File::open(path) else {
        return Ok(None);
    };
    Ok(parse_adm(&mut file).map(|info| Master {
        kind: MasterKind::Adm,
        duration: info.duration,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut v = id.to_vec();
        v.extend((body.len() as u32).to_le_bytes());
        v.extend(body);
        if body.len() % 2 == 1 {
            v.push(0);
        }
        v
    }

    fn fmt_chunk() -> Vec<u8> {
        // PCM, 2 channels, 48000 Hz, 24 bit: block align 6
        let mut b = Vec::new();
        b.extend(1u16.to_le_bytes());
        b.extend(2u16.to_le_bytes());
        b.extend(48000u32.to_le_bytes());
        b.extend((48000u32 * 6).to_le_bytes());
        b.extend(6u16.to_le_bytes());
        b.extend(24u16.to_le_bytes());
        chunk(b"fmt ", &b)
    }

    fn file(magic: &[u8; 4], chunks: &[Vec<u8>]) -> Vec<u8> {
        let mut body = b"WAVE".to_vec();
        for c in chunks {
            body.extend(c);
        }
        let mut v = magic.to_vec();
        v.extend((body.len() as u32).to_le_bytes());
        v.extend(body);
        v
    }

    fn adm(bytes: Vec<u8>) -> Option<AdmInfo> {
        parse_adm(&mut Cursor::new(bytes))
    }

    #[test]
    fn chunks_before_the_data() {
        let bytes = file(
            b"RIFF",
            &[
                fmt_chunk(),
                chunk(b"chna", &[0; 4]),
                chunk(b"axml", b"<x/>"),
                chunk(b"data", &[0; 48000 * 6]),
            ],
        );
        assert_eq!(adm(bytes), Some(AdmInfo { duration: None }));
    }

    #[test]
    fn axml_after_the_data_and_odd_sizes() {
        let bytes = file(
            b"RIFF",
            &[
                fmt_chunk(),
                chunk(b"data", &[0; 48000 * 6 + 3]),
                chunk(b"chna", &[0; 5]),
                chunk(b"axml", b"<x/>x"),
            ],
        );
        let info = adm(bytes).unwrap();
        assert!((info.duration.unwrap() - 1.0).abs() < 0.001);
    }

    #[test]
    fn plain_wav_is_not_adm() {
        let bytes = file(b"RIFF", &[fmt_chunk(), chunk(b"data", &[0; 600])]);
        assert_eq!(adm(bytes), None);
        // only one of the two chunks
        let bytes = file(
            b"RIFF",
            &[
                fmt_chunk(),
                chunk(b"chna", &[0; 4]),
                chunk(b"data", &[0; 6]),
            ],
        );
        assert_eq!(adm(bytes), None);
        assert_eq!(adm(b"fLaC\0\0\0\0WAVE".to_vec()), None);
        assert_eq!(adm(b"RIFF\0\0\0\0AVI ".to_vec()), None);
        assert_eq!(adm(Vec::new()), None);
    }

    #[test]
    fn truncated_files_are_not_adm() {
        let full = file(
            b"RIFF",
            &[
                fmt_chunk(),
                chunk(b"data", &[0; 1000]),
                chunk(b"chna", &[0; 4]),
                chunk(b"axml", b"<x/>"),
            ],
        );
        assert!(adm(full.clone()).is_some());
        // cut inside the data, inside the axml header, inside the axml body
        for cut in [60, full.len() - 20, full.len() - 6, full.len() - 1] {
            assert_eq!(adm(full[..cut].to_vec()), None, "cut at {cut}");
        }
    }

    #[test]
    fn bw64_with_ds64() {
        let mut ds64 = Vec::new();
        ds64.extend(0u64.to_le_bytes()); // riff size
        ds64.extend(288_000u64.to_le_bytes()); // data size
        ds64.extend(0u64.to_le_bytes()); // sample count
        ds64.extend(0u32.to_le_bytes()); // table length
        let mut data = b"data".to_vec();
        data.extend(0xFFFF_FFFFu32.to_le_bytes());
        data.extend(vec![0u8; 288_000]);
        for magic in [b"BW64", b"RF64"] {
            let bytes = file(
                magic,
                &[
                    chunk(b"ds64", &ds64),
                    fmt_chunk(),
                    chunk(b"chna", &[0; 4]),
                    data.clone(),
                    chunk(b"axml", b"<x/>"),
                ],
            );
            let info = adm(bytes).unwrap();
            assert!((info.duration.unwrap() - 1.0).abs() < 0.001);
        }
    }

    #[test]
    fn ds64_size_table_gives_the_axml_size() {
        let mut ds64 = Vec::new();
        ds64.extend(0u64.to_le_bytes());
        ds64.extend(0u64.to_le_bytes());
        ds64.extend(0u64.to_le_bytes());
        ds64.extend(1u32.to_le_bytes());
        ds64.extend(b"axml");
        ds64.extend(6u64.to_le_bytes());
        let mut axml = b"axml".to_vec();
        axml.extend(0xFFFF_FFFFu32.to_le_bytes());
        axml.extend(b"<xml/>");
        let bytes = file(
            b"BW64",
            &[chunk(b"ds64", &ds64), chunk(b"chna", &[0; 4]), axml],
        );
        assert!(adm(bytes).is_some());
    }

    #[test]
    fn damf_names() {
        assert!(is_damf_name("/a/b.atmos"));
        assert!(is_damf_name("B.ATMOS"));
        assert!(!is_damf_name("b.atmos.audio"));
        assert!(!is_damf_name("b.atmosx"));
        let dir: Vec<String> = [
            "m.atmos",
            "m.atmos.audio",
            "m.atmos.metadata",
            "x.atmos.audio",
        ]
        .map(String::from)
        .to_vec();
        assert!(is_damf_part("m.atmos.audio", &dir));
        assert!(is_damf_part("m.atmos.metadata", &dir));
        assert!(!is_damf_part("m.atmos", &dir));
        // no header of its own: a plain input
        assert!(!is_damf_part("x.atmos.audio", &dir));
    }

    #[test]
    fn damf_header_parts() {
        let header = "version: 0.5.1\npresentations:\n  - type: home\n    metadata: a.atmos.metadata\n    audio: \"a.atmos.audio\"\n";
        assert_eq!(
            damf_parts(header, "/d/a.atmos"),
            [
                Path::new("/d").join("a.atmos.metadata").to_string_lossy(),
                Path::new("/d").join("a.atmos.audio").to_string_lossy()
            ]
        );
        assert_eq!(
            damf_parts("", "/d/a.atmos"),
            ["/d/a.atmos.metadata", "/d/a.atmos.audio"]
        );
    }

    #[test]
    fn atmos_info_duration() {
        let out = "[x] Atmos master type: DAMF\n[x] Content duration (in samples): 960000\n";
        assert_eq!(parse_atmos_info_duration(out), Some(20.0));
        assert_eq!(parse_atmos_info_duration("nothing"), None);
    }
}
