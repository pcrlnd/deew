//! ffmpeg, truehdd and DEE command lines (argument lists and the markup printed for
//! them). Everything here is pure.

use crate::config::DdpAtmosMode;
use crate::error::escape;
use crate::plan::Format;
use crate::tools::Shim;
use crate::util::shell_quote;

/// Which pan filter an input needs: DEE wants the slots L R C LFE Ls Rs (Lrs
/// Rrs), where Ls/Rs are the side and Lrs/Rrs the back surrounds, whatever
/// the mask of the wav says. ffmpeg's 7.1 has the back pair first. 5.0 and 7.0
/// inputs get a silent LFE, and `silent_lfe` empties it for 5.1 and 7.1.
pub fn channel_pan(channels: i64, silent_lfe: bool) -> Option<String> {
    let lfe = if silent_lfe || [5, 7].contains(&channels) {
        "0*c0"
    } else {
        "c3"
    };
    Some(match channels {
        5 => "pan=5.1|c0=c0|c1=c1|c2=c2|c3=0*c0|c4=c3|c5=c4".to_string(),
        6 if silent_lfe => "pan=5.1|c0=c0|c1=c1|c2=c2|c3=0*c0|c4=c4|c5=c5".to_string(),
        7 => "pan=7.1|c0=c0|c1=c1|c2=c2|c3=0*c0|c4=c5|c5=c6|c6=c3|c7=c4".to_string(),
        8 => format!("pan=7.1|c0=c0|c1=c1|c2=c2|c3={lfe}|c4=c6|c5=c7|c6=c4|c7=c5"),
        _ => return None,
    })
}

/// The filter chain that downmixes a TrueHD input to 5.1 (`out` 6) or 2.0
/// (`out` 2) with the coefficients of DEE's own downmix (the encoder can't
/// downmix): the input channels are in ffmpeg's native order, 5.0 and 7.0
/// have no LFE. Like DEE, the stereo downmix limits the peaks of each channel
/// on its own (-0.1 dBFS, `latency=1` compensates the lookahead delay so the
/// sync stays). alimiter would link the channels, a hot left channel would
/// pull the right one down. The 5.1 downmix can't clip (its weights add up
/// to 1) and stays untouched.
pub fn downmix_chain(channels: i64, out: i64, silent_lfe: bool) -> Option<String> {
    // indices of: back/side surrounds left, right (7.x: BL BR SL SR)
    let (lfe, bl, br, sl, sr) = match channels {
        5 => (None, 3, 4, 3, 4),
        6 => (Some(3), 4, 5, 4, 5),
        7 => (None, 3, 4, 5, 6),
        8 => (Some(3), 4, 5, 6, 7),
        _ => return None,
    };
    let pan = match (out, channels) {
        (6, 7 | 8) => {
            let lfe = match lfe {
                Some(i) if !silent_lfe => format!("c{i}"),
                _ => "0*c0".to_string(),
            };
            format!(
                "pan=5.1|c0=c0|c1=c1|c2=c2|c3={lfe}|c4=0.5*c{bl}+0.5*c{sl}|c5=0.5*c{br}+0.5*c{sr}"
            )
        }
        (2, 5 | 6) => {
            format!("pan=stereo|c0=c0+0.7071*c2+0.7071*c{bl}|c1=c1+0.7071*c2+0.7071*c{br}")
        }
        (2, 7 | 8) => format!(
            "pan=stereo|c0=c0+0.7071*c2+0.3536*c{bl}+0.3536*c{sl}|c1=c1+0.7071*c2+0.3536*c{br}+0.3536*c{sr}"
        ),
        _ => return None,
    };
    let limit = "alimiter=limit=0.98855:level=0:latency=1";
    Some(if out == 2 {
        format!(
            "aformat=sample_fmts=dbl,{pan},channelsplit=channel_layout=stereo[l][r];\
[l]{limit}[ll];[r]{limit}[rr];[ll][rr]join=inputs=2:channel_layout=stereo"
        )
    } else {
        format!("aformat=sample_fmts=dbl,{pan}")
    })
}

/// How an input is converted to the intermediate wav. Inputs share their
/// sample rate and channels, so only the bit depth can differ between plans.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioPlan {
    /// Bit depth of the intermediate (32 when resampling).
    pub bit_depth: u32,
    pub map_args: Vec<String>,
    pub map_print: String,
    pub swap_args: Vec<String>,
    pub swap_print: String,
    pub resample_args: Vec<String>,
    pub resample_print: String,
}

/// Target sample rate if the input has to be resampled.
fn resample_rate(format: Format, samplerate: i64) -> Option<&'static str> {
    match format {
        Format::Dd | Format::Ddp | Format::Ac4 | Format::Aac if samplerate != 48000 => {
            Some("48000")
        }
        Format::Thd if samplerate != 48000 && samplerate != 96000 => {
            Some(if samplerate < 72000 { "48000" } else { "96000" })
        }
        _ => None,
    }
}

/// What makes the intermediate wav of two runs of the same input differ,
/// as a part of its name (empty for the plain conversion): the sample rate
/// target of TrueHD (96 kHz stays), the TrueHD downmix, and a silent LFE that
/// is written into a 5.1 / 7.1 wav. Bit depth, channel swap and the rate of
/// the other formats follow from the input.
pub fn intermediate_variant(
    format: Format,
    samplerate: i64,
    channels: i64,
    silent_lfe: bool,
    downmix: Option<i64>,
) -> String {
    let mut parts = Vec::new();
    if resample_rate(format, samplerate) != resample_rate(Format::Ddp, samplerate) {
        parts.push("thd".to_string());
    }
    let downmix = downmix.filter(|_| format == Format::Thd);
    if let Some(d) = downmix {
        parts.push(format!("dm{d}"));
    }
    if silent_lfe && [6, 8].contains(&channels) && downmix != Some(2) {
        parts.push("dl".to_string());
    }
    parts.join(".")
}

pub fn audio_plan(
    format: Format,
    samplerate: i64,
    channels: i64,
    bit_depth: u32,
    track_index: i64,
    silent_lfe: bool,
    downmix: Option<i64>,
) -> AudioPlan {
    let ti = track_index;
    // Only TrueHD downmixes here, the other encoders do it themselves.
    let pan = match downmix.filter(|_| format == Format::Thd) {
        Some(out) => downmix_chain(channels, out, silent_lfe),
        None => channel_pan(channels, silent_lfe),
    };
    let rate = resample_rate(format, samplerate);
    let bit_depth = if rate.is_some() { 32 } else { bit_depth };

    let (resample_args, resample_print) = match rate {
        Some(rate) => {
            let swap = pan.as_ref().map_or(String::new(), |p| format!("{p},"));
            (
                [
                    "-filter_complex",
                    &format!("[a:{ti}]{swap}aresample=resampler=soxr"),
                    "-ar",
                    rate,
                    "-precision",
                    "28",
                    "-cutoff",
                    "1",
                    "-dither_scale",
                    "0",
                ]
                .map(String::from)
                .to_vec(),
                format!(
                    "-filter_complex [bold color(231)]{}[/bold color(231)] \
-ar [bold color(231)]{rate}[/bold color(231)] \
-precision [bold color(231)]28[/bold color(231)] \
-cutoff [bold color(231)]1[/bold color(231)] \
-dither_scale [bold color(231)]0[/bold color(231)] ",
                    escape(&shell_quote(&format!(
                        "[a:{ti}]{swap}aresample=resampler=soxr"
                    )))
                ),
            )
        }
        None => (Vec::new(), String::new()),
    };

    let (swap_args, swap_print) = if let (Some(pan), true) = (&pan, resample_args.is_empty()) {
        (
            vec!["-filter_complex".to_string(), format!("[a:{ti}]{pan}")],
            format!(
                "-filter_complex [bold color(231)]{}[/bold color(231)] ",
                escape(&shell_quote(&format!("[a:{ti}]{pan}")))
            ),
        )
    } else {
        (Vec::new(), String::new())
    };

    let (map_args, map_print) = if resample_args.is_empty() && swap_args.is_empty() {
        (
            vec!["-map".to_string(), format!("0:a:{ti}")],
            format!(
                "-map [bold color(231)]0:a[/bold color(231)][bold color(231)]:{ti}[/bold color(231)] "
            ),
        )
    } else {
        (Vec::new(), String::new())
    };

    AudioPlan {
        bit_depth,
        map_args,
        map_print,
        swap_args,
        swap_print,
        resample_args,
        resample_print,
    }
}

pub fn ffmpeg_args(ffmpeg: &str, plan: &AudioPlan, input: &str, wav: &str) -> Vec<String> {
    let mut args: Vec<String> = [ffmpeg, "-y", "-drc_scale", "0", "-i", input]
        .map(String::from)
        .to_vec();
    args.extend(plan.map_args.iter().cloned());
    args.push("-c".to_string());
    args.push(format!("pcm_s{}le", plan.bit_depth));
    args.extend(plan.swap_args.iter().cloned());
    args.extend(plan.resample_args.iter().cloned());
    args.extend(["-rf64", "always", wav].map(String::from));
    args
}

/// ffmpeg: copies the selected TrueHD track (of a container or a raw stream)
/// to stdout, as a raw stream for the stdin of truehdd. ffmpeg prints
/// timestamp warnings even with `-v error`, only its exit code counts.
pub fn pipe_args(ffmpeg: &str, input: &str, track_index: i64) -> Vec<String> {
    [
        ffmpeg,
        "-v",
        "error",
        "-nostdin",
        "-i",
        input,
        "-map",
        &format!("0:a:{track_index}"),
        "-c",
        "copy",
        "-f",
        "truehd",
        "-",
    ]
    .map(String::from)
    .to_vec()
}

fn ffmpeg_print_with(plan: &AudioPlan, depth: &str, input: &str, output: &str) -> String {
    format!(
        "[bold cyan]ffmpeg[/bold cyan] -y -drc_scale [bold color(231)]0[/bold color(231)] \
-i [bold green]{input}[/bold green] \
{}-c [bold color(231)]{}[/bold color(231)] \
{}{}-rf64 [bold color(231)]always[/bold color(231)] \
[bold magenta]{output}[/bold magenta]",
        plan.map_print, depth, plan.swap_print, plan.resample_print
    )
}

/// The command as printed with `-la` (markup).
pub fn ffmpeg_print(plan: &AudioPlan, input: &str, wav: &str) -> String {
    let depth = format!("pcm_s{}le", plan.bit_depth);
    ffmpeg_print_with(
        plan,
        &depth,
        &escape(&shell_quote(input)),
        &escape(&shell_quote(wav)),
    )
}

/// The codec of the short print: `pcm_s24le`, or for mixed bit depths the
/// distinct ones sorted, e.g. `pcm_s16le/pcm_s24le`.
fn short_codec(depths: &[u32]) -> String {
    let mut distinct = depths.to_vec();
    distinct.sort_unstable();
    distinct.dedup();
    let codecs: Vec<String> = distinct.iter().map(|d| format!("pcm_s{d}le")).collect();
    codecs.join("/")
}

/// The command as printed by default, with `[input]` and `[output]`.
/// `plans` are the plans of all inputs (not empty).
pub fn ffmpeg_print_short(plans: &[AudioPlan]) -> String {
    let depths: Vec<u32> = plans.iter().map(|p| p.bit_depth).collect();
    ffmpeg_print_with(&plans[0], &short_codec(&depths), "\\[input]", "\\[output]")
}

pub const INTERMEDIATE_EXISTS_PRINT: &str = "[green]Intermediate already exists[/green]";

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| s.to_string()).collect()
}

/// Everything the command line of an encoder depends on.
#[derive(Debug, Clone)]
pub struct EncodeParams<'a> {
    pub format: Format,
    /// Path of the encoder.
    pub tool: &'a str,
    /// The wav, or the `.atmos` file of the Atmos formats.
    pub input: &'a str,
    pub output: &'a str,
    pub bitrate: u32,
    pub drc: &'a str,
    pub outchannels: i64,
    /// Value of `-c`, see `plan::Layout`.
    pub layout_arg: Option<&'a str>,
    /// Loudness manifest with the custom dialnorm.
    pub input_manifest: Option<&'a str>,
    /// Custom dialnorm given to DEE with `--loudness-management
    /// skip:dialnorm=N`, for every format but AC4.
    pub skip_dialnorm: Option<i64>,
    /// Manifest the AC4 encoders write.
    pub output_manifest: &'a str,
    /// `--start` / `--add-silence` (see `util::delay_options`).
    pub delay: &'a [String],
    pub temp: &'a str,
    /// dee_shim: required for DDP and TrueHD Atmos, and for TrueHD without DRC.
    pub shim: Option<&'a Shim>,
    /// 7.1 DDP in the Blu-ray profile (native), else the standard profile.
    pub bluray: bool,
    /// The profile of DDP Atmos.
    pub atmos_mode: DdpAtmosMode,
    /// The output has no LFE channel (`lfe_on=0`, or a 7.0 layout of dee_shim).
    pub lfe_removed: bool,
}

fn thd_presentation(outchannels: i64) -> &'static str {
    match outchannels {
        2 => "2ch",
        6 => "6ch",
        _ => "8ch",
    }
}

/// The `-e` argument of `dee_ddp_encoder`.
fn ddp_encoder(format: Format, outchannels: i64, drc: &str, lfe_removed: bool) -> String {
    let mode = match (format, outchannels) {
        (Format::Dd, _) => "dd",
        (_, 8) => "bluray",
        _ => "ddp",
    };
    // `lfe_on=0` makes a real 5.0, 7.0 is a layout of dee_shim
    let lfe = if lfe_removed && outchannels == 6 {
        ":lfe_on=0"
    } else {
        ""
    };
    format!("{mode}:drc_profile={drc}{lfe}")
}

/// The command that encodes with DEE (the first item is the program).
pub fn encode_args(p: &EncodeParams) -> Vec<String> {
    let bitrate = p.bitrate.to_string();
    let common = ["--progress", "1", "--overwrite", "1"];
    let mut args = Vec::new();
    match p.format {
        Format::DdpAtmos => {
            // dee_shim runs the encoder, the caller made sure it is there.
            let shim = p.shim.expect("DDP Atmos is encoded with dee_shim");
            // streaming: 5.1 E-AC-3 with JOC (DEE's ddpjoc mode); the 7.1
            // layouts use DEE's bluray mode, /eac3 replaces the AC-3 core
            // with an E-AC-3 independent substream (standard).
            let (layout, mode) = match p.atmos_mode {
                DdpAtmosMode::Streaming => ("5.1", "ddpjoc"),
                DdpAtmosMode::Standard => ("7.1/eac3", "bluray"),
                DdpAtmosMode::Bluray => ("7.1", "bluray"),
            };
            args.extend(strings(&[
                &shim.run,
                "ddpjoc_encoder",
                "--binary",
                p.tool,
                "--layout",
                layout,
                "--oamd-as-is",
                "on",
                "--bitrate",
                &bitrate,
                "--",
            ]));
            args.extend(strings(&common));
            args.extend(strings(&["-e", &format!("{mode}:drc_profile={}", p.drc)]));
            args.extend(strings(&["--input-format", "atmos_mezz", "-i", p.input]));
            args.extend(strings(&["-o", p.output]));
        }
        Format::ThdAtmos => {
            // dee_shim runs the encoder, the caller made sure it is there.
            let shim = p.shim.expect("TrueHD Atmos is encoded with dee_shim");
            args.extend(strings(&[
                &shim.run,
                "dthd_encoder",
                "--binary",
                p.tool,
                "--oamd-as-is",
                "on",
                "--as-is",
                "on",
                "--drc",
                p.drc,
                "--",
            ]));
            args.extend(strings(&common));
            args.extend(strings(&[
                "--presentation",
                "atmos:number_of_elements=16:legacy_authoring=0",
                "--input-format",
                "atmos_mezz",
                "-i",
                p.input,
                "-o",
                p.output,
            ]));
        }
        Format::Ac4Atmos => {
            args.push(p.tool.to_string());
            args.extend(strings(&common));
            args.extend(strings(&["-i", p.input, "-o", p.output]));
            args.extend(strings(&["--output-manifest", p.output_manifest]));
            args.extend(strings(&["--level", "3", "--data-rate", &bitrate]));
            args.extend(strings(&["-e", &format!("drc_profile={}", p.drc)]));
        }
        Format::Thd => {
            // The TrueHD encoder has no `none` preset, dee_shim adds it (and
            // its --drc wins over the profile of the presentation).
            let shim = p.shim.filter(|_| p.drc == "none");
            match shim {
                Some(shim) => {
                    args.extend(strings(&[
                        &shim.run,
                        "dthd_encoder",
                        "--binary",
                        p.tool,
                        "--drc",
                        "none",
                        "--",
                    ]));
                }
                None => args.push(p.tool.to_string()),
            }
            args.extend(strings(&common));
            args.push("--presentation".to_string());
            // The shim overrides the profile, the encoder itself takes a valid one.
            let drc = if shim.is_some() { "film_light" } else { p.drc };
            args.push(format!(
                "{}:drc_profile={drc}",
                thd_presentation(p.outchannels)
            ));
            args.extend(strings(&["--input-format", "wav", "-i", p.input]));
            args.extend(strings(&["-o", p.output]));
        }
        Format::Ddp if p.outchannels == 8 && (!p.bluray || p.lfe_removed) => {
            // The standard 7.1 profile (E-AC-3 independent and dependent
            // substreams) is a dee_shim layout, which supplies the -e. So are
            // both 7.0 profiles (7.0 is the Blu-ray AC-3 core), they discard
            // the LFE slot.
            let shim = p
                .shim
                .expect("standard 7.1 and 7.0 DDP are encoded with dee_shim");
            let layout = match (p.bluray, p.lfe_removed) {
                (false, false) => "7.1/eac3",
                (false, true) => "7.0/eac3",
                (true, _) => "7.0",
            };
            args.extend(strings(&[
                &shim.run,
                "ddp_encoder",
                "--binary",
                p.tool,
                "--layout",
                layout,
                "--bitrate",
                &bitrate,
                "--",
            ]));
            args.extend(strings(&common));
            // DRC is DEE's own sub-option; the shim keeps it and sets the
            // mode of the layout (`ddp` for /eac3, `bluray` for 7.0).
            let mode = if layout == "7.0" { "bluray" } else { "ddp" };
            args.extend(strings(&["-e", &format!("{mode}:drc_profile={}", p.drc)]));
            args.extend(strings(&["--input-format", "wav", "-i", p.input]));
            args.extend(strings(&["-o", p.output]));
            match p.skip_dialnorm {
                Some(n) => args.extend(strings(&[
                    "--loudness-management",
                    &format!("skip:dialnorm={n}"),
                ])),
                None => args.extend(strings(&["--loudness-management", "measure_only"])),
            }
            if let Some(manifest) = p.input_manifest {
                args.extend(strings(&["--input-manifest", manifest]));
            }
        }
        Format::Dd | Format::Ddp | Format::Ac4 | Format::Aac => {
            args.push(p.tool.to_string());
            args.extend(strings(&common));
            args.extend(strings(&["--input-format", "wav", "-i", p.input]));
            args.extend(strings(&["-o", p.output]));
            match p.format {
                Format::Ac4 => {
                    args.extend(strings(&["--output-manifest", p.output_manifest]));
                    args.extend(strings(&["--data-rate", &bitrate]));
                    args.extend(strings(&["-e", &format!("drc_profile={}", p.drc)]));
                }
                Format::Aac => {
                    args.extend(strings(&["-e", "aaclc", "--data-rate", &bitrate]));
                }
                _ => {
                    args.extend(strings(&[
                        "-e",
                        &ddp_encoder(p.format, p.outchannels, p.drc, p.lfe_removed),
                        "--data-rate",
                        &bitrate,
                    ]));
                }
            }
            if let Some(layout) = p.layout_arg {
                args.extend(strings(&["-c", layout]));
            }
            let management = match (p.format, p.skip_dialnorm) {
                (Format::Aac, _) => "skip".to_string(),
                (_, Some(n)) => format!("skip:dialnorm={n}"),
                _ => "measure_only".to_string(),
            };
            args.extend(strings(&["--loudness-management", &management]));
            if let Some(manifest) = p.input_manifest {
                args.extend(strings(&["--input-manifest", manifest]));
            }
        }
    }
    if let Some(n) = p.skip_dialnorm
        && matches!(p.format, Format::DdpAtmos | Format::Thd | Format::ThdAtmos)
    {
        args.extend(strings(&[
            "--loudness-management",
            &format!("skip:dialnorm={n}"),
        ]));
    }
    args.extend(p.delay.iter().cloned());
    // The DDP and AC4 Atmos encoders don't know the option.
    if !matches!(p.format, Format::DdpAtmos | Format::Ac4Atmos) {
        args.extend(strings(&["--temp-dir", p.temp]));
    }
    args
}

/// `dee_measure_loudness`: measures the wav and writes the loudness manifest
/// (the tool has no `--temp-dir`).
pub fn measure_args(tool: &str, wav: &str, manifest: &str) -> Vec<String> {
    strings(&[
        tool,
        "--progress",
        "1",
        "--overwrite",
        "1",
        "--input-format",
        "wav",
        "-i",
        wav,
        "--output-manifest",
        manifest,
    ])
}

/// `-mo` for 8-channel input, which `dee_measure_loudness` rejects: the DDP
/// encoder in the Blu-ray profile with `measure_only`. The caller stops it
/// once it has printed the loudness, `output` is a throwaway file.
pub fn measure_with_encoder_args(tool: &str, wav: &str, output: &str, temp: &str) -> Vec<String> {
    strings(&[
        tool,
        "--progress",
        "1",
        "--overwrite",
        "1",
        "--input-format",
        "wav",
        "-i",
        wav,
        "-o",
        output,
        "-e",
        "bluray:drc_profile=none",
        "--data-rate",
        "1024",
        "--loudness-management",
        "measure_only",
        "--temp-dir",
        temp,
    ])
}

/// truehdd: decodes the Atmos presentation to `<output_prefix>.atmos` (and
/// `.atmos.metadata`, `.atmos.audio`).
pub fn decode_args(truehdd: &str, input: &str, output_prefix: &str) -> Vec<String> {
    strings(&[
        truehdd,
        "decode",
        "--presentation",
        "3",
        "--json",
        "--output-path",
        output_prefix,
        input,
    ])
}

/// How a path argument is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Shown {
    Input,
    Output,
    Manifest,
}

fn shown(option: &str) -> Option<Shown> {
    match option {
        "-i" => Some(Shown::Input),
        "-o" | "--output-path" => Some(Shown::Output),
        "--input-manifest" | "--output-manifest" => Some(Shown::Manifest),
        _ => None,
    }
}

/// The command as printed (markup). With `short` the files are shown as
/// `[input]`, `[output]` and `[manifest]` so one line describes every input.
pub fn print_command(args: &[String], short: bool) -> String {
    let program = crate::util::file_name(&args[0]);
    let mut out = format!("[bold cyan]{}[/bold cyan]", escape(&shell_quote(program)));
    // truehdd takes the input as its last argument.
    let positional_input = program.starts_with("truehdd");
    // ffmpeg takes the output as its last argument.
    let positional_output = program.starts_with("ffmpeg");
    let mut previous = "";
    for (i, arg) in args.iter().enumerate().skip(1) {
        out.push(' ');
        let kind = if positional_input && i == args.len() - 1 && arg != "-" {
            Some(Shown::Input)
        } else if positional_output && i == args.len() - 1 && arg != "-" {
            Some(Shown::Output)
        } else {
            shown(previous)
        };
        let text = match (kind, short) {
            (Some(Shown::Input), true) => "\\[input]".to_string(),
            (Some(Shown::Output), true) => "\\[output]".to_string(),
            (Some(Shown::Manifest), true) => "\\[manifest]".to_string(),
            _ => escape(&shell_quote(arg)),
        };
        match kind {
            Some(Shown::Input) => out.push_str(&format!("[bold green]{text}[/bold green]")),
            Some(_) => out.push_str(&format!("[bold magenta]{text}[/bold magenta]")),
            None if arg.starts_with('-') => out.push_str(&text),
            None => out.push_str(&format!("[bold color(231)]{text}[/bold color(231)]")),
        }
        previous = arg;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::markup::strip;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    const SWAP: &str = "pan=7.1|c0=c0|c1=c1|c2=c2|c3=c3|c4=c6|c5=c7|c6=c4|c7=c5";

    #[test]
    fn five_one_48k_maps_the_track() {
        let plan = audio_plan(Format::Ddp, 48000, 6, 16, 0, false, None);
        assert_eq!(
            ffmpeg_args("ffmpeg", &plan, "in.mkv", "/t/in.wav"),
            v(&[
                "ffmpeg",
                "-y",
                "-drc_scale",
                "0",
                "-i",
                "in.mkv",
                "-map",
                "0:a:0",
                "-c",
                "pcm_s16le",
                "-rf64",
                "always",
                "/t/in.wav"
            ])
        );
        let plan = audio_plan(Format::Ac4, 48000, 6, 24, 2, false, None);
        assert_eq!(plan.map_args, v(&["-map", "0:a:2"]));
        assert_eq!(plan.bit_depth, 24);
    }

    #[test]
    fn stereo_44k_resamples() {
        let plan = audio_plan(Format::Ddp, 44100, 2, 16, 0, false, None);
        assert_eq!(
            ffmpeg_args("ffmpeg", &plan, "in.flac", "/t/in.wav"),
            v(&[
                "ffmpeg",
                "-y",
                "-drc_scale",
                "0",
                "-i",
                "in.flac",
                "-c",
                "pcm_s32le",
                "-filter_complex",
                "[a:0]aresample=resampler=soxr",
                "-ar",
                "48000",
                "-precision",
                "28",
                "-cutoff",
                "1",
                "-dither_scale",
                "0",
                "-rf64",
                "always",
                "/t/in.wav"
            ])
        );
        assert!(plan.map_args.is_empty() && plan.swap_args.is_empty());
    }

    #[test]
    fn seven_one_48k_swaps_channels() {
        let plan = audio_plan(Format::Ddp, 48000, 8, 24, 1, false, None);
        assert_eq!(
            ffmpeg_args("ffmpeg", &plan, "in.mkv", "w.wav"),
            v(&[
                "ffmpeg",
                "-y",
                "-drc_scale",
                "0",
                "-i",
                "in.mkv",
                "-c",
                "pcm_s24le",
                "-filter_complex",
                &format!("[a:1]{SWAP}"),
                "-rf64",
                "always",
                "w.wav"
            ])
        );
        assert!(plan.map_args.is_empty() && plan.map_print.is_empty());
    }

    #[test]
    fn seven_one_44k_resamples_and_swaps() {
        let plan = audio_plan(Format::Ddp, 44100, 8, 24, 0, false, None);
        assert_eq!(plan.bit_depth, 32);
        assert!(plan.swap_args.is_empty() && plan.map_args.is_empty());
        assert_eq!(
            plan.resample_args[..2],
            v(&[
                "-filter_complex",
                &format!("[a:0]{SWAP},aresample=resampler=soxr")
            ])[..]
        );
    }

    #[test]
    fn pipe_command() {
        let args = pipe_args("ffmpeg", "in.mka", 1);
        assert_eq!(
            args,
            v(&[
                "ffmpeg", "-v", "error", "-nostdin", "-i", "in.mka", "-map", "0:a:1", "-c", "copy",
                "-f", "truehd", "-"
            ])
        );
        assert_eq!(
            strip(&print_command(&args, true)),
            "ffmpeg -v error -nostdin -i [input] -map 0:a:1 -c copy -f truehd -"
        );
        assert_eq!(
            strip(&print_command(&decode_args("truehdd", "-", "/t/a"), true)),
            "truehdd decode --presentation 3 --json --output-path [output] -"
        );
    }

    #[test]
    fn thd_rates() {
        let rate = |sr, ch| audio_plan(Format::Thd, sr, ch, 24, 0, false, None);
        assert!(rate(48000, 6).resample_args.is_empty());
        assert!(rate(96000, 6).resample_args.is_empty());
        assert_eq!(rate(88200, 6).resample_args[3], "96000");
        assert_eq!(rate(192000, 6).resample_args[3], "96000");
        assert_eq!(rate(72000, 6).resample_args[3], "96000");
        assert_eq!(rate(71999, 6).resample_args[3], "48000");
        assert_eq!(rate(44100, 6).resample_args[3], "48000");
        assert_eq!(rate(88200, 6).bit_depth, 32);
        // DD/DDP/AC4/AAC always go to 48 kHz, even from 96 kHz
        assert_eq!(
            audio_plan(Format::Aac, 44100, 2, 16, 0, false, None).resample_args[3],
            "48000"
        );
        assert!(
            audio_plan(Format::Aac, 48000, 2, 16, 0, false, None)
                .resample_args
                .is_empty()
        );
        assert_eq!(
            audio_plan(Format::Dd, 96000, 6, 24, 0, false, None).resample_args[3],
            "48000"
        );
    }

    #[test]
    fn print_strings() {
        let plan = audio_plan(Format::Ddp, 48000, 6, 24, 0, false, None);
        assert_eq!(
            ffmpeg_print(&plan, "a [1].mkv", "/t/a [1].wav"),
            format!(
                "[bold cyan]ffmpeg[/bold cyan] -y -drc_scale [bold color(231)]0[/bold color(231)] \
-i [bold green]{}[/bold green] \
-map [bold color(231)]0:a[/bold color(231)][bold color(231)]:0[/bold color(231)] \
-c [bold color(231)]pcm_s24le[/bold color(231)] \
-rf64 [bold color(231)]always[/bold color(231)] \
[bold magenta]{}[/bold magenta]",
                escape(&shell_quote("a [1].mkv")),
                escape(&shell_quote("/t/a [1].wav"))
            )
        );
        assert_eq!(
            strip(&ffmpeg_print_short(std::slice::from_ref(&plan))),
            "ffmpeg -y -drc_scale 0 -i [input] -map 0:a:0 -c pcm_s24le -rf64 always [output]"
        );
        let plan = audio_plan(Format::Ddp, 44100, 8, 24, 0, false, None);
        assert_eq!(
            strip(&ffmpeg_print_short(std::slice::from_ref(&plan))),
            format!(
                "ffmpeg -y -drc_scale 0 -i [input] -c pcm_s32le -filter_complex {} -ar 48000 -precision 28 -cutoff 1 -dither_scale 0 -rf64 always [output]",
                shell_quote(&format!("[a:0]{SWAP},aresample=resampler=soxr"))
            )
        );
        let plan = audio_plan(Format::Ddp, 48000, 8, 24, 0, false, None);
        assert_eq!(
            strip(&ffmpeg_print_short(std::slice::from_ref(&plan))),
            format!(
                "ffmpeg -y -drc_scale 0 -i [input] -c pcm_s24le -filter_complex {} -rf64 always [output]",
                shell_quote(&format!("[a:0]{SWAP}"))
            )
        );
    }

    #[test]
    fn printed_commands_are_shell_quoted() {
        let name = "a & b; $(x) 'q'.wav";
        let quoted = "'a & b; $(x) '\\''q'\\''.wav'";
        let args = v(&["ffmpeg", "-y", "-i", name, "-map", "0:a:0", "out dir/o.wav"]);
        let long = strip(&print_command(&args, false));
        if !cfg!(windows) {
            assert_eq!(
                long,
                format!("ffmpeg -y -i {quoted} -map 0:a:0 'out dir/o.wav'")
            );
        }
        let plan = audio_plan(Format::Ddp, 48000, 6, 24, 0, false, None);
        let line = strip(&ffmpeg_print(&plan, name, "/t/o.wav"));
        if !cfg!(windows) {
            assert_eq!(
                line,
                format!(
                    "ffmpeg -y -drc_scale 0 -i {quoted} -map 0:a:0 -c pcm_s24le -rf64 always /t/o.wav"
                )
            );
        }
        // placeholders stay as they are
        let short = strip(&print_command(&args, true));
        assert_eq!(short, "ffmpeg -y -i [input] -map 0:a:0 [output]");
    }

    #[test]
    fn short_print_with_mixed_depths() {
        let plan = |d| audio_plan(Format::Ddp, 48000, 6, d, 0, false, None);
        let short = |plans: &[AudioPlan]| strip(&ffmpeg_print_short(plans));
        assert_eq!(
            short(&[plan(24), plan(16), plan(24)]),
            "ffmpeg -y -drc_scale 0 -i [input] -map 0:a:0 -c pcm_s16le/pcm_s24le -rf64 always [output]"
        );
        assert!(short(&[plan(32), plan(24), plan(16)]).contains("pcm_s16le/pcm_s24le/pcm_s32le"));
        assert!(short(&[plan(16), plan(16)]).contains("-c pcm_s16le -rf64"));
        // resampling forces 32 bit for every input
        let rs = |d| audio_plan(Format::Ddp, 44100, 6, d, 0, false, None);
        assert!(short(&[rs(16), rs(24)]).contains("-c pcm_s32le -filter_complex"));
    }

    const LIM: &str = "channelsplit=channel_layout=stereo[l][r];\
[l]alimiter=limit=0.98855:level=0:latency=1[ll];[r]alimiter=limit=0.98855:level=0:latency=1[rr];\
[ll][rr]join=inputs=2:channel_layout=stereo";

    fn thd_chain(channels: i64, out: i64, silent_lfe: bool) -> String {
        let plan = audio_plan(Format::Thd, 48000, channels, 24, 0, silent_lfe, Some(out));
        assert!(plan.map_args.is_empty() && plan.resample_args.is_empty());
        assert_eq!(plan.swap_args[0], "-filter_complex");
        plan.swap_args[1].clone()
    }

    #[test]
    fn intermediate_variants() {
        let v = intermediate_variant;
        assert_eq!(v(Format::Ddp, 48000, 6, false, None), "");
        assert_eq!(v(Format::Thd, 48000, 8, false, None), "");
        assert_eq!(v(Format::Thd, 44100, 8, false, None), "");
        // TrueHD keeps 96 kHz, the others go to 48 kHz
        assert_eq!(v(Format::Thd, 96000, 6, false, None), "thd");
        assert_eq!(v(Format::Ddp, 96000, 6, false, None), "");
        assert_eq!(v(Format::Thd, 88200, 6, false, None), "thd");
        assert_eq!(v(Format::Thd, 96000, 2, false, Some(2)), "thd.dm2");
        assert_eq!(v(Format::Thd, 48000, 8, false, Some(2)), "dm2");
        assert_eq!(v(Format::Thd, 48000, 8, false, Some(6)), "dm6");
        // a silent LFE changes the wav of 5.1 / 7.1 (and -dm 6), not of the rest
        assert_eq!(v(Format::Thd, 48000, 6, true, None), "dl");
        assert_eq!(v(Format::Ac4, 48000, 8, true, None), "dl");
        assert_eq!(v(Format::Thd, 48000, 8, true, Some(6)), "dm6.dl");
        assert_eq!(v(Format::Thd, 48000, 8, true, Some(2)), "dm2");
        assert_eq!(v(Format::Thd, 48000, 7, true, None), "");
        // other formats downmix in DEE
        assert_eq!(v(Format::Ddp, 48000, 8, false, Some(2)), "");
    }

    #[test]
    fn thd_downmix_to_5_1() {
        let want = |lfe: &str, a: &str, b: &str, c: &str, d: &str| {
            format!(
                "[a:0]aformat=sample_fmts=dbl,pan=5.1|c0=c0|c1=c1|c2=c2|c3={lfe}|c4=0.5*c{a}+0.5*c{b}|c5=0.5*c{c}+0.5*c{d}"
            )
        };
        // 7.1: BL BR SL SR
        assert_eq!(thd_chain(8, 6, false), want("c3", "4", "6", "5", "7"));
        assert_eq!(thd_chain(8, 6, true), want("0*c0", "4", "6", "5", "7"));
        // 7.0 has no LFE: BL BR SL SR
        assert_eq!(thd_chain(7, 6, false), want("0*c0", "3", "5", "4", "6"));
        assert_eq!(thd_chain(7, 6, true), want("0*c0", "3", "5", "4", "6"));
    }

    #[test]
    fn thd_downmix_to_stereo() {
        let five = |a: i64, b: i64| {
            format!(
                "[a:0]aformat=sample_fmts=dbl,pan=stereo|c0=c0+0.7071*c2+0.7071*c{a}|c1=c1+0.7071*c2+0.7071*c{b},{LIM}"
            )
        };
        let seven = |a: i64, b: i64, c: i64, d: i64| {
            format!(
                "[a:0]aformat=sample_fmts=dbl,pan=stereo|c0=c0+0.7071*c2+0.3536*c{a}+0.3536*c{c}|c1=c1+0.7071*c2+0.3536*c{b}+0.3536*c{d},{LIM}"
            )
        };
        assert_eq!(thd_chain(5, 2, false), five(3, 4));
        assert_eq!(thd_chain(6, 2, false), five(4, 5));
        assert_eq!(thd_chain(7, 2, false), seven(3, 4, 5, 6));
        assert_eq!(thd_chain(8, 2, false), seven(4, 5, 6, 7));
        // the LFE is dropped, a silent one changes nothing
        assert_eq!(thd_chain(8, 2, true), thd_chain(8, 2, false));
        assert_eq!(downmix_chain(6, 6, false), None);
        assert_eq!(downmix_chain(2, 2, false), None);
    }

    #[test]
    fn thd_downmix_resamples_and_prints() {
        let plan = audio_plan(Format::Thd, 44100, 8, 24, 1, false, Some(2));
        assert_eq!(plan.bit_depth, 32);
        let chain = thd_chain(8, 2, false).replace("[a:0]", "[a:1]");
        assert_eq!(
            strip(&ffmpeg_print_short(std::slice::from_ref(&plan))),
            format!(
                "ffmpeg -y -drc_scale 0 -i [input] -c pcm_s32le -filter_complex {} -ar 48000 -precision 28 -cutoff 1 -dither_scale 0 -rf64 always [output]",
                shell_quote(&format!("{chain},aresample=resampler=soxr"))
            )
        );
        let plan = audio_plan(Format::Thd, 48000, 8, 16, 0, false, Some(6));
        let args = ffmpeg_args("ffmpeg", &plan, "a.thd", "a.wav");
        assert_eq!(
            args[6..],
            v(&[
                "-c",
                "pcm_s16le",
                "-filter_complex",
                &thd_chain(8, 6, false),
                "-rf64",
                "always",
                "a.wav"
            ])
        );
        assert_eq!(
            strip(&ffmpeg_print_short(std::slice::from_ref(&plan))),
            format!(
                "ffmpeg -y -drc_scale 0 -i [input] -c pcm_s16le -filter_complex {} -rf64 always [output]",
                shell_quote(&thd_chain(8, 6, false))
            )
        );
        // the other formats downmix in DEE, a downmix is not looked at
        let plan = audio_plan(Format::Ddp, 48000, 8, 24, 0, false, Some(2));
        assert_eq!(plan, audio_plan(Format::Ddp, 48000, 8, 24, 0, false, None));
    }

    #[test]
    fn thd_presentation_follows_the_output_channels() {
        let shim = Shim {
            run: "/shim/dee_shim".into(),
        };
        for (out, name) in [(2, "2ch"), (6, "6ch"), (8, "8ch")] {
            let mut p = params(Format::Thd);
            p.outchannels = out;
            p.layout_arg = None;
            assert!(encode_args(&p).contains(&format!("{name}:drc_profile=music_light")));
            p.drc = "none";
            p.shim = Some(&shim);
            let args = encode_args(&p);
            assert_eq!(args[0], "/shim/dee_shim");
            assert!(args.contains(&format!("{name}:drc_profile=film_light")));
            assert!(!args.contains(&"-c".to_string()));
        }
    }

    fn params<'a>(format: Format) -> EncodeParams<'a> {
        EncodeParams {
            format,
            tool: "/dee/tool",
            input: "/t/a.wav",
            output: "/out/a.ext",
            bitrate: 640,
            drc: "music_light",
            outchannels: 6,
            layout_arg: None,
            input_manifest: None,
            skip_dialnorm: None,
            output_manifest: "/t/a.ac4.json",
            delay: &[],
            temp: "/t",
            shim: None,
            bluray: false,
            atmos_mode: DdpAtmosMode::Streaming,
            lfe_removed: false,
        }
    }

    #[test]
    fn dd_and_ddp_commands() {
        assert_eq!(
            encode_args(&params(Format::Ddp)),
            v(&[
                "/dee/tool",
                "--progress",
                "1",
                "--overwrite",
                "1",
                "--input-format",
                "wav",
                "-i",
                "/t/a.wav",
                "-o",
                "/out/a.ext",
                "-e",
                "ddp:drc_profile=music_light",
                "--data-rate",
                "640",
                "--loudness-management",
                "measure_only",
                "--temp-dir",
                "/t"
            ])
        );
        let mut p = params(Format::Dd);
        p.layout_arg = Some("stereo");
        p.input_manifest = Some("/t/a.loudness.json");
        let delay = v(&["--start", "0.005333"]);
        p.delay = &delay;
        assert_eq!(
            encode_args(&p)[11..],
            v(&[
                "-e",
                "dd:drc_profile=music_light",
                "--data-rate",
                "640",
                "-c",
                "stereo",
                "--loudness-management",
                "measure_only",
                "--input-manifest",
                "/t/a.loudness.json",
                "--start",
                "0.005333",
                "--temp-dir",
                "/t"
            ])
        );
        // 8 channels are encoded in bluray mode
        let mut p = params(Format::Ddp);
        p.outchannels = 8;
        p.bluray = true;
        assert!(encode_args(&p).contains(&"bluray:drc_profile=music_light".to_string()));
    }

    #[test]
    fn pan_filters_for_the_dee_slot_order() {
        // 5.0 gets a silent LFE in slot 4
        assert_eq!(
            channel_pan(5, false).unwrap(),
            "pan=5.1|c0=c0|c1=c1|c2=c2|c3=0*c0|c4=c3|c5=c4"
        );
        // 7.0 (FL FR FC BL BR SL SR): side pair first, back pair last
        assert_eq!(
            channel_pan(7, false).unwrap(),
            "pan=7.1|c0=c0|c1=c1|c2=c2|c3=0*c0|c4=c5|c5=c6|c6=c3|c7=c4"
        );
        // 7.1 as before, or with the LFE emptied
        assert_eq!(
            channel_pan(8, false).unwrap(),
            "pan=7.1|c0=c0|c1=c1|c2=c2|c3=c3|c4=c6|c5=c7|c6=c4|c7=c5"
        );
        assert_eq!(
            channel_pan(8, true).unwrap(),
            "pan=7.1|c0=c0|c1=c1|c2=c2|c3=0*c0|c4=c6|c5=c7|c6=c4|c7=c5"
        );
        // 5.1 needs no filter unless the LFE is emptied
        assert_eq!(channel_pan(6, false), None);
        assert_eq!(
            channel_pan(6, true).unwrap(),
            "pan=5.1|c0=c0|c1=c1|c2=c2|c3=0*c0|c4=c4|c5=c5"
        );
        assert_eq!(channel_pan(2, true), None);
        assert_eq!(channel_pan(1, false), None);
        // the plan: the filter replaces the map, and goes in front of the resampler
        let plan = audio_plan(Format::Ddp, 48000, 5, 24, 0, false, None);
        assert!(plan.map_args.is_empty());
        assert_eq!(
            ffmpeg_args("ffmpeg", &plan, "in.mka", "w.wav")[6..],
            v(&[
                "-c",
                "pcm_s24le",
                "-filter_complex",
                "[a:0]pan=5.1|c0=c0|c1=c1|c2=c2|c3=0*c0|c4=c3|c5=c4",
                "-rf64",
                "always",
                "w.wav"
            ])
        );
        let plan = audio_plan(Format::Ddp, 44100, 7, 24, 1, false, None);
        assert_eq!(plan.bit_depth, 32);
        assert!(plan.resample_args[1].starts_with(
            "[a:1]pan=7.1|c0=c0|c1=c1|c2=c2|c3=0*c0|c4=c5|c5=c6|c6=c3|c7=c4,aresample="
        ));
        let plan = audio_plan(Format::Thd, 48000, 6, 24, 0, true, None);
        assert!(plan.map_args.is_empty());
        assert!(plan.swap_args[1].starts_with("[a:0]pan=5.1|"));
    }

    #[test]
    fn no_lfe_commands() {
        let shim = Shim {
            run: "/shim/dee_shim".into(),
        };
        // 5.0: lfe_on=0, also with a downmix from 7.0
        let mut p = params(Format::Ddp);
        p.lfe_removed = true;
        assert!(encode_args(&p).contains(&"ddp:drc_profile=music_light:lfe_on=0".to_string()));
        p.format = Format::Dd;
        p.layout_arg = Some("5.1");
        let args = encode_args(&p);
        assert!(args.contains(&"dd:drc_profile=music_light:lfe_on=0".to_string()));
        assert!(args.contains(&"-c".to_string()));
        // a stereo output has no LFE to turn off
        p.outchannels = 2;
        assert!(!encode_args(&p).iter().any(|a| a.contains("lfe_on")));
        // 7.0: dee_shim layouts, the standard one and the Blu-ray AC-3 core
        let mut p = params(Format::Ddp);
        p.outchannels = 8;
        p.bitrate = 1792;
        p.drc = "none";
        p.lfe_removed = true;
        p.shim = Some(&shim);
        let layout = |p: &EncodeParams| {
            let args = encode_args(p);
            assert_eq!(args[0], "/shim/dee_shim");
            assert_eq!(args[1], "ddp_encoder");
            assert!(
                !args.contains(&"--drc".to_string()) && !args.contains(&"--data-rate".to_string())
            );
            // DRC is DEE's own -e, with the mode of the layout
            let mode = if args.contains(&"7.0".to_string()) {
                "bluray"
            } else {
                "ddp"
            };
            assert_eq!(opt_value(&args, "-e"), format!("{mode}:drc_profile=none"));
            assert_eq!(
                args[args.iter().position(|a| a == "--bitrate").unwrap() + 1],
                "1792"
            );
            args[args.iter().position(|a| a == "--layout").unwrap() + 1].clone()
        };
        assert_eq!(layout(&p), "7.0/eac3");
        p.bluray = true;
        p.bitrate = 1792;
        assert_eq!(layout(&p), "7.0");
        // 7.1 with a silent LFE: the usual native Blu-ray
        p.lfe_removed = false;
        assert_eq!(encode_args(&p)[0], "/dee/tool");
    }

    #[test]
    fn seven_one_ddp_profiles() {
        let shim = Shim {
            run: "/shim/dee_shim".into(),
        };
        let mut p = params(Format::Ddp);
        p.outchannels = 8;
        p.bitrate = 1792;
        p.drc = "none";
        let delay = v(&["--start", "0.005333"]);
        p.delay = &delay;
        // standard: through the shim, which supplies the encoder mode
        p.shim = Some(&shim);
        assert_eq!(
            encode_args(&p),
            v(&[
                "/shim/dee_shim",
                "ddp_encoder",
                "--binary",
                "/dee/tool",
                "--layout",
                "7.1/eac3",
                "--bitrate",
                "1792",
                "--",
                "--progress",
                "1",
                "--overwrite",
                "1",
                "-e",
                "ddp:drc_profile=none",
                "--input-format",
                "wav",
                "-i",
                "/t/a.wav",
                "-o",
                "/out/a.ext",
                "--loudness-management",
                "measure_only",
                "--start",
                "0.005333",
                "--temp-dir",
                "/t"
            ])
        );
        // Blu-ray: native, also when dee_shim is there
        p.bluray = true;
        p.drc = "music_light";
        let args = encode_args(&p);
        assert_eq!(args[0], "/dee/tool");
        assert!(args.contains(&"bluray:drc_profile=music_light".to_string()));
        assert_eq!(
            args[args.iter().position(|a| a == "--data-rate").unwrap() + 1],
            "1792"
        );
        assert!(!args.contains(&"--layout".to_string()) && !args.contains(&"-c".to_string()));
    }

    #[test]
    fn ac4_aac_and_thd_commands() {
        let mut p = params(Format::Ac4);
        p.layout_arg = Some("stereo");
        assert_eq!(
            encode_args(&p)[5..],
            v(&[
                "--input-format",
                "wav",
                "-i",
                "/t/a.wav",
                "-o",
                "/out/a.ext",
                "--output-manifest",
                "/t/a.ac4.json",
                "--data-rate",
                "640",
                "-e",
                "drc_profile=music_light",
                "-c",
                "stereo",
                "--loudness-management",
                "measure_only",
                "--temp-dir",
                "/t"
            ])
        );
        let aac = encode_args(&params(Format::Aac));
        assert_eq!(
            aac[11..],
            v(&[
                "-e",
                "aaclc",
                "--data-rate",
                "640",
                "--loudness-management",
                "skip",
                "--temp-dir",
                "/t"
            ])
        );
        assert!(!aac.contains(&"-c".to_string()));
        let mut p = params(Format::Thd);
        p.drc = "film_standard";
        p.outchannels = 8;
        assert_eq!(
            encode_args(&p),
            v(&[
                "/dee/tool",
                "--progress",
                "1",
                "--overwrite",
                "1",
                "--presentation",
                "8ch:drc_profile=film_standard",
                "--input-format",
                "wav",
                "-i",
                "/t/a.wav",
                "-o",
                "/out/a.ext",
                "--temp-dir",
                "/t"
            ])
        );
        p.outchannels = 2;
        p.drc = "speech";
        assert_eq!(
            encode_args(&p)[5..7],
            v(&["--presentation", "2ch:drc_profile=speech"])
        );
    }

    #[test]
    fn thd_without_drc_goes_through_dee_shim() {
        let shim = Shim {
            run: "/shim/dee_shim".into(),
        };
        let mut p = params(Format::Thd);
        p.drc = "none";
        p.shim = Some(&shim);
        assert_eq!(
            encode_args(&p),
            v(&[
                "/shim/dee_shim",
                "dthd_encoder",
                "--binary",
                "/dee/tool",
                "--drc",
                "none",
                "--",
                "--progress",
                "1",
                "--overwrite",
                "1",
                "--presentation",
                "6ch:drc_profile=film_light",
                "--input-format",
                "wav",
                "-i",
                "/t/a.wav",
                "-o",
                "/out/a.ext",
                "--temp-dir",
                "/t"
            ])
        );
        // other profiles stay native, also with dee_shim
        p.drc = "speech";
        assert_eq!(encode_args(&p)[0], "/dee/tool");
        assert!(encode_args(&p).contains(&"6ch:drc_profile=speech".to_string()));
    }

    #[test]
    fn atmos_commands() {
        let mut p = params(Format::Ac4Atmos);
        p.input = "/t/a.atmos";
        let delay = v(&["--add-silence", "1:0"]);
        p.delay = &delay;
        assert_eq!(
            encode_args(&p),
            v(&[
                "/dee/tool",
                "--progress",
                "1",
                "--overwrite",
                "1",
                "-i",
                "/t/a.atmos",
                "-o",
                "/out/a.ext",
                "--output-manifest",
                "/t/a.ac4.json",
                "--level",
                "3",
                "--data-rate",
                "640",
                "-e",
                "drc_profile=music_light",
                "--add-silence",
                "1:0"
            ])
        );
        let mut p = params(Format::DdpAtmos);
        p.input = "/t/a.atmos";
        let shim = Shim {
            run: "/shim/dee_shim".into(),
        };
        p.shim = Some(&shim);
        let delay = v(&["--start", "0.005333"]);
        p.delay = &delay;
        assert_eq!(
            encode_args(&p),
            v(&[
                "/shim/dee_shim",
                "ddpjoc_encoder",
                "--binary",
                "/dee/tool",
                "--layout",
                "5.1",
                "--oamd-as-is",
                "on",
                "--bitrate",
                "640",
                "--",
                "--progress",
                "1",
                "--overwrite",
                "1",
                "-e",
                "ddpjoc:drc_profile=music_light",
                "--input-format",
                "atmos_mezz",
                "-i",
                "/t/a.atmos",
                "-o",
                "/out/a.ext",
                "--start",
                "0.005333"
            ])
        );
    }

    #[test]
    fn ddp_atmos_modes() {
        let shim = Shim {
            run: "/shim/dee_shim".into(),
        };
        for (mode, layout, native) in [
            (DdpAtmosMode::Streaming, "5.1", "ddpjoc:drc_profile=speech"),
            (
                DdpAtmosMode::Standard,
                "7.1/eac3",
                "bluray:drc_profile=speech",
            ),
            (DdpAtmosMode::Bluray, "7.1", "bluray:drc_profile=speech"),
        ] {
            let mut p = params(Format::DdpAtmos);
            p.input = "/t/a.atmos";
            p.drc = "speech";
            p.bitrate = 1152;
            p.shim = Some(&shim);
            p.atmos_mode = mode;
            let args = encode_args(&p);
            assert_eq!(opt_value(&args, "--layout"), layout);
            assert_eq!(opt_value(&args, "--bitrate"), "1152");
            assert_eq!(opt_value(&args, "--oamd-as-is"), "on");
            assert_eq!(opt_value(&args, "-e"), native);
            assert_eq!(opt_value(&args, "--input-format"), "atmos_mezz");
        }
    }

    #[test]
    fn thd_atmos_command() {
        let shim = Shim {
            run: "/shim/dee_shim".into(),
        };
        let mut p = params(Format::ThdAtmos);
        p.input = "/t/a.atmos";
        p.drc = "none";
        p.shim = Some(&shim);
        let delay = v(&["--add-silence", "1:0"]);
        p.delay = &delay;
        assert_eq!(
            encode_args(&p),
            v(&[
                "/shim/dee_shim",
                "dthd_encoder",
                "--binary",
                "/dee/tool",
                "--oamd-as-is",
                "on",
                "--as-is",
                "on",
                "--drc",
                "none",
                "--",
                "--progress",
                "1",
                "--overwrite",
                "1",
                "--presentation",
                "atmos:number_of_elements=16:legacy_authoring=0",
                "--input-format",
                "atmos_mezz",
                "-i",
                "/t/a.atmos",
                "-o",
                "/out/a.ext",
                "--add-silence",
                "1:0",
                "--temp-dir",
                "/t"
            ])
        );
        p.drc = "speech";
        assert_eq!(opt_value(&encode_args(&p), "--drc"), "speech");
    }

    fn opt_value(args: &[String], name: &str) -> String {
        let i = args.iter().position(|a| a == name).unwrap();
        args[i + 1].clone()
    }

    #[test]
    fn measure_and_decode_commands() {
        assert_eq!(
            measure_args("/dee/m", "/t/a.wav", "/t/a.loudness.json"),
            v(&[
                "/dee/m",
                "--progress",
                "1",
                "--overwrite",
                "1",
                "--input-format",
                "wav",
                "-i",
                "/t/a.wav",
                "--output-manifest",
                "/t/a.loudness.json"
            ])
        );
        assert_eq!(
            decode_args("truehdd", "/in/a.thd", "/t/a"),
            v(&[
                "truehdd",
                "decode",
                "--presentation",
                "3",
                "--json",
                "--output-path",
                "/t/a",
                "/in/a.thd"
            ])
        );
    }

    fn assert_skip(args: &[String], after: Option<&str>) {
        let hits: Vec<usize> = (0..args.len())
            .filter(|&i| args[i] == "--loudness-management")
            .collect();
        assert_eq!(hits.len(), 1, "{args:?}");
        assert_eq!(args[hits[0] + 1], "skip:dialnorm=-20", "{args:?}");
        assert!(!args.contains(&"measure_only".to_string()), "{args:?}");
        assert!(!args.contains(&"--input-manifest".to_string()), "{args:?}");
        if let Some(marker) = after {
            let at = args.iter().position(|a| a == marker).unwrap();
            assert!(hits[0] > at, "{args:?}");
        }
    }

    #[test]
    fn skip_dialnorm_commands() {
        let shim = Shim {
            run: "/shim/dee_shim".into(),
        };
        let mut p = params(Format::Dd);
        p.skip_dialnorm = Some(-20);
        assert_skip(&encode_args(&p), None);
        let mut p = params(Format::Ddp);
        p.skip_dialnorm = Some(-20);
        assert_skip(&encode_args(&p), None);
        let mut p = params(Format::Ddp);
        p.outchannels = 8;
        p.shim = Some(&shim);
        p.skip_dialnorm = Some(-20);
        assert_skip(&encode_args(&p), Some("--"));
        let mut p = params(Format::DdpAtmos);
        p.input = "/t/a.atmos";
        p.shim = Some(&shim);
        p.skip_dialnorm = Some(-20);
        assert_skip(&encode_args(&p), Some("--"));
        let mut p = params(Format::Thd);
        p.skip_dialnorm = Some(-20);
        assert_skip(&encode_args(&p), None);
        let mut p = params(Format::Thd);
        p.drc = "none";
        p.shim = Some(&shim);
        p.skip_dialnorm = Some(-20);
        assert_skip(&encode_args(&p), Some("--"));
        let mut p = params(Format::ThdAtmos);
        p.input = "/t/a.atmos";
        p.shim = Some(&shim);
        p.skip_dialnorm = Some(-20);
        assert_skip(&encode_args(&p), Some("--"));
        let args = encode_args(&params(Format::Thd));
        assert!(!args.contains(&"--loudness-management".to_string()));
    }

    #[test]
    fn ac4_keeps_the_manifest() {
        let mut p = params(Format::Ac4);
        p.input_manifest = Some("/t/a.loudness.json");
        let args = encode_args(&p);
        let at = args
            .iter()
            .position(|a| a == "--loudness-management")
            .unwrap();
        assert_eq!(args[at + 1], "measure_only");
        assert_eq!(args[at + 2], "--input-manifest");
        assert_eq!(args[at + 3], "/t/a.loudness.json");
    }

    #[test]
    fn measuring_with_the_encoder() {
        assert_eq!(
            measure_with_encoder_args("/dee/dee_ddp_encoder", "/t/a.wav", "/t/d/m.ec3", "/t/d"),
            v(&[
                "/dee/dee_ddp_encoder",
                "--progress",
                "1",
                "--overwrite",
                "1",
                "--input-format",
                "wav",
                "-i",
                "/t/a.wav",
                "-o",
                "/t/d/m.ec3",
                "-e",
                "bluray:drc_profile=none",
                "--data-rate",
                "1024",
                "--loudness-management",
                "measure_only",
                "--temp-dir",
                "/t/d",
            ])
        );
    }

    #[test]
    fn printed_commands() {
        let mut p = params(Format::Ac4);
        p.input = "/t/a [1].wav";
        let args = encode_args(&p);
        let long = strip(&print_command(&args, false));
        assert!(
            long.starts_with(&format!(
                "tool --progress 1 --overwrite 1 --input-format wav -i {} -o /out/a.ext",
                shell_quote("/t/a [1].wav")
            )),
            "{long}"
        );
        assert!(print_command(&args, false).contains(&format!(
            "[bold green]{}[/bold green]",
            escape(&shell_quote("/t/a [1].wav"))
        )));
        let short = strip(&print_command(&args, true));
        assert!(
            short.contains("-i [input] -o [output] --output-manifest [manifest] --data-rate"),
            "{short}"
        );
        assert!(short.ends_with("--temp-dir /t"), "{short}");
        assert_eq!(
            strip(&print_command(
                &v(&["/x/truehdd", "decode", "--output-path", "/t/a", "a.thd"]),
                true
            )),
            "truehdd decode --output-path [output] [input]"
        );
    }
}
