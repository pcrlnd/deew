//! The encoding pipeline: checks, probing, planning, and running the jobs.

use crate::cli::Args;
use crate::commands::{
    AudioPlan, EncodeParams, INTERMEDIATE_EXISTS_PRINT, LfeSlot, audio_plan, decode_args,
    encode_args, ffmpeg_args, ffmpeg_pipe_args, ffmpeg_pipe_print, ffmpeg_pipe_print_short,
    ffmpeg_print, ffmpeg_print_short, harletty_decode_args, intermediate_variant, measure_args,
    measure_atmos_args, measure_shim_args, pcm_input_format, pcm_rate, pipe_args,
    pipe_sample_count, print_command,
};
use crate::config::{Ac4ImsMode, Config, DdpAtmosMode, Paths, PipeMode, WarpMode};
use crate::data::ErrorKind;
use crate::deewlog::LogSpec;
use crate::encode::{self, Decoder, Job, Options, Outcome};
use crate::error::{error_line, escape, format_error, print_exit};
use crate::events;
use crate::harletty::{self, Feed};
use crate::inputs::{self, Input, Problem};
use crate::interrupt;
use crate::markup;
use crate::master;
use crate::output_name::{self, Template, Values};
use crate::pipe;
use crate::plan::{
    self, Format, InputInfo, Layout, Lfe, Modes, Profile71, Validated, check_ac4_dialnorm_channels,
    check_atmos_inputs, check_ims_inputs, check_input_layouts, check_inputs, check_master_inputs,
    check_shim_for_ddp, compute_instances, input_delay, notices, profile_71, resolve_layout,
    select_bitrate, validate_args,
};
use crate::probe::{self, Probe, ProbeError, TrackIndex};
use crate::summary::{self, SummaryData};
use crate::tools::{self, Shim, Version, Versions};
use crate::ui::{End, Ui};
use crate::util::{basename, delay_options, file_name, file_stem};
use crate::{prompt, rprintln};
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

fn create_dir(path: &str) {
    if std::fs::create_dir_all(path).is_err() {
        print_exit(ErrorKind::CreateDir, Some(path));
    }
}

/// `os.path.join`.
fn join(dir: &str, name: &str) -> String {
    Path::new(dir).join(name).to_string_lossy().into_owned()
}

/// The input files of the command line: folders and patterns expanded,
/// duplicates left out. A path that doesn't exist and a pattern without a
/// match are errors (the message, as markup).
pub fn try_collect_inputs(inputs: &[String]) -> Result<Vec<Input>, String> {
    let mut list = Vec::new();
    for input in inputs {
        if inputs::is_pattern(input) {
            list.extend(inputs::expand_pattern(input).map_err(|message| escape(&message))?);
            continue;
        }
        if !Path::new(input).exists() {
            return Err(format_error(ErrorKind::Path, Some(input)));
        }
        list.extend(inputs::expand(input));
    }
    Ok(inputs::dedup(list))
}

fn collect_inputs(inputs: &[String]) -> Vec<Input> {
    try_collect_inputs(inputs).unwrap_or_else(|message| error_line(&message))
}

/// What the probe of one input gives: an Atmos master is detected by its
/// headers (`-ti` doesn't apply, it goes to DEE as it is), anything else is
/// probed with ffprobe.
pub fn probe_one(
    input: &Input,
    ffprobe: &str,
    atmos_info: Option<&str>,
    track_index: TrackIndex,
) -> Result<Probe, ProbeError> {
    if input.bad_name {
        return Err(ProbeError::BadName);
    }
    match master::detect(&input.path) {
        Ok(Some(found)) => Ok(Probe::of_master(master_duration(
            found,
            atmos_info,
            &input.path,
        ))),
        Ok(None) => probe::run(ffprobe, track_index, &input.path),
        Err(missing) => Err(ProbeError::DamfMissing(missing)),
    }
}

/// Probe all the inputs. Problems with them are collected first: errors
/// stop the run, files of folders that aren't audio are left out (after
/// asking, unless `no_prompt`). Returns the usable files with their probes.
fn probe_inputs(
    ffprobe: &str,
    atmos_info: Option<&str>,
    track_index: TrackIndex,
    no_prompt: bool,
    inputs: Vec<Input>,
) -> (Vec<String>, Vec<Probe>) {
    let mut files = Vec::new();
    let mut probes = Vec::new();
    let mut problems = Vec::new();
    for input in inputs {
        match probe_one(&input, ffprobe, atmos_info, track_index) {
            Ok(probe) => {
                files.push(input.path);
                probes.push(probe);
            }
            Err(error) => problems.push(Problem {
                name: input.name,
                folder: input.folder,
                error,
            }),
        }
    }
    let errors: Vec<&Problem> = problems.iter().filter(|p| !p.skippable()).collect();
    if !errors.is_empty() {
        error_line(&inputs::errors_text(&errors, track_index));
    }
    if problems.is_empty() {
        return (files, probes);
    }
    let skipped: Vec<&Problem> = problems.iter().collect();
    if files.is_empty() {
        error_line(&inputs::none_left_text(&skipped));
    }
    for block in inputs::skip_blocks(&skipped) {
        rprintln!("{}", block);
    }
    let (question, note) = inputs::skip_question(skipped.len(), files.len());
    if no_prompt {
        for p in &skipped {
            events::warning(&format!("{}: skipped, {}", p.name, p.error.message()));
        }
        rprintln!("{}", note);
    } else if !prompt::confirm(&question) {
        std::process::exit(1);
    }
    (files, probes)
}

/// A DAMF has no duration in its header, `atmos_info` (if installed) tells it.
fn master_duration(found: master::Master, atmos_info: Option<&str>, path: &str) -> master::Master {
    match (found.kind, atmos_info) {
        (master::MasterKind::Damf, Some(tool)) => master::Master {
            duration: master::damf_duration(tool, path),
            ..found
        },
        _ => found,
    }
}

/// 96 kHz TrueHD (also resampled to it): DEE's encoder would apply its
/// apodising filter, dee_shim turns it off.
fn apodise_off(format: Format, samplerate: i64) -> bool {
    format == Format::Thd && pcm_rate(format, samplerate) == 96000
}

fn show_notices(list: &[plan::Notice], no_prompt: bool) {
    for notice in list {
        if no_prompt {
            events::warning(&markup::strip(notice.note));
            rprintln!("{}", notice.note);
        } else if !prompt::confirm(notice.question) {
            std::process::exit(1);
        }
    }
}

/// The output directory: `-o`, else the config's `output.path` (created),
/// else the current directory.
fn output_directory(arg: Option<&str>, config_path: &str) -> String {
    let cwd = std::env::current_dir().unwrap_or_default();
    let configured = Some(config_path).filter(|p| !p.is_empty());
    match arg.filter(|o| !o.is_empty()).or(configured) {
        Some(out) => {
            let out = tools::abspath(out, &cwd);
            create_dir(&out);
            out
        }
        None => cwd.to_string_lossy().into_owned(),
    }
}

/// The programs a run needs.
struct Programs {
    ffmpeg: String,
    ffprobe: String,
    /// The DEE tool that encodes (or measures with `-mo`).
    dee: String,
    /// `dee_measure_loudness`, for `-mo` and a custom dialnorm.
    measure: Option<String>,
    /// `dee_measure_loudness`, measures the decoded TrueHD Atmos sources of `-mo`.
    measure_atmos: Option<String>,
    truehdd: Option<String>,
    /// Found without checking whether it is needed, see `detect_spatial`.
    harletty: Option<String>,
    shim: Option<Shim>,
}

/// Look up the DEE tools of the format; exits if one is missing.
fn find_programs(
    binaries: tools::Binaries,
    exe_dir: &Path,
    format: Format,
    drc: &str,
    needs_measure: bool,
    measure_only: bool,
) -> Programs {
    let cwd = std::env::current_dir().unwrap_or_default();
    let dee = |tool: &str| tools::require_dee_tool(tool, &cwd, exe_dir);
    // `-mo` finds its tool once the sources are known (Atmos sources have their own)
    let measure = (needs_measure && !measure_only).then(|| dee("dee_measure_loudness"));
    let encoder = if measure_only {
        String::new()
    } else {
        dee(format.dee_tool())
    };
    let shim = tools::find_shim(&cwd, exe_dir);
    if shim.is_none()
        && let Some(fail) = plan::needs_shim(format, drc, measure_only)
    {
        fail.exit();
    }
    Programs {
        ffmpeg: binaries.ffmpeg,
        ffprobe: binaries.ffprobe,
        dee: encoder,
        measure,
        measure_atmos: None,
        // required once the inputs are known: Atmos masters need no truehdd
        truehdd: tools::find_truehdd(exe_dir, &cwd),
        harletty: tools::find_harletty(exe_dir, &cwd),
        shim,
    }
}

/// Whether `-mo` measures this source as Atmos: an Atmos master, or TrueHD
/// with an Atmos presentation.
fn atmos_source(probe: &Probe) -> bool {
    probe.master.is_some() || (probe.is_truehd && probe.is_atmos)
}

/// Whether the input is decoded by truehdd: the Atmos formats (and the Atmos
/// source of AC4 IMS) take TrueHD, a master needs no decode and a DTS:X or
/// Auro-3D source goes to harletty.
fn needs_truehdd(format: Format, probe: &Probe) -> bool {
    probe.master.is_none()
        && probe.spatial.is_none()
        && (format.is_atmos() || (format == Format::Ac4Ims && plan::ims_atmos_source(probe)))
}

/// Whether the input is decoded by harletty: a DTS:X or Auro-3D source of
/// the Atmos formats and AC4 IMS. Any other format (and `-mo`) decodes DTS
/// with ffmpeg.
fn needs_harletty(format: Format, probe: &Probe) -> bool {
    probe.master.is_none()
        && probe.spatial.is_some()
        && (format.is_atmos() || format == Format::Ac4Ims)
}

/// How many inputs are checked by harletty at the same time.
const AURO_CHECKS_AT_ONCE: usize = 4;

/// Best effort: ask harletty about the inputs that can be Auro-3D (see
/// [`Probe::auro_candidate`]) so they are shown as such. Nothing is reported
/// when harletty is missing, too old or can't read an input; the result is
/// kept in `harletty_check` (only for the inputs it answered for), which the
/// strict detection of the Atmos formats reuses. The checks run in parallel.
pub fn check_auro(
    ffmpeg: &str,
    harletty: Option<&str>,
    inputs: Vec<(&str, &mut Probe)>,
    track: TrackIndex,
) {
    let Some(harletty) = harletty else {
        return;
    };
    let mut todo: Vec<(&str, &mut Probe)> = inputs
        .into_iter()
        .filter(|(_, p)| p.harletty_check.is_none() && p.auro_candidate())
        .collect();
    if todo.is_empty()
        || tools::check_min_version(&tools::harletty_version(harletty), tools::MIN_HARLETTY)
            .is_err()
    {
        return;
    }
    for batch in todo.chunks_mut(AURO_CHECKS_AT_ONCE) {
        let asked: Vec<(&str, Feed)> = batch.iter().map(|(f, p)| (*f, Feed::of(p))).collect();
        let answers: Vec<_> = std::thread::scope(|scope| {
            let handles: Vec<_> = asked
                .iter()
                .map(|&(file, feed)| {
                    scope.spawn(move || harletty::detect(harletty, ffmpeg, file, track, feed))
                })
                .collect();
            handles.into_iter().map(|h| h.join()).collect()
        });
        for ((_, probe), answer) in batch.iter_mut().zip(answers) {
            if let Ok(Ok(spatial)) = answer {
                probe.harletty_check = Some(spatial);
            }
        }
    }
}

/// Ask harletty whether the DTS inputs are DTS:X or Auro-3D, which makes them
/// Atmos sources. harletty is only looked for when there is a DTS input that
/// [`check_auro`] has not answered for. For the Atmos formats the run stops if
/// it is missing or too old, or can't read an input. AC4 IMS needs it only
/// for a track ffprobe identifies as DTS:X; otherwise it is best effort, and
/// without harletty the DTS tracks stay channel-based.
fn detect_spatial(
    programs: &mut Programs,
    exe_dir: &Path,
    cwd: &Path,
    format: Format,
    files: &[String],
    probes: &mut [Probe],
    track: TrackIndex,
) {
    // what [`check_auro`] found (Auro-3D in 24-bit PCM is only ever asked
    // there, best effort: an answer that failed leaves the track as it was)
    for probe in probes.iter_mut().filter(|p| p.master.is_none()) {
        if let Some(check) = &probe.harletty_check {
            probe.spatial = check.clone();
        }
    }
    let unchecked: Vec<usize> = (0..probes.len())
        .filter(|&i| {
            probes[i].master.is_none()
                && plan::is_dts(&probes[i])
                && probes[i].harletty_check.is_none()
        })
        .collect();
    if unchecked.is_empty() {
        return;
    }
    let strict = format.is_atmos() || unchecked.iter().any(|&i| probes[i].is_dtsx());
    let harletty = if strict {
        let found = match programs.harletty.take() {
            Some(found) => found,
            None => tools::require_harletty(exe_dir, cwd),
        };
        tools::require_harletty_version(&found);
        found
    } else {
        match programs.harletty.take() {
            Some(found)
                if tools::check_min_version(
                    &tools::harletty_version(&found),
                    tools::MIN_HARLETTY,
                )
                .is_ok() =>
            {
                found
            }
            other => {
                programs.harletty = other;
                return;
            }
        }
    };
    for i in unchecked {
        match harletty::detect(
            &harletty,
            &programs.ffmpeg,
            &files[i],
            track,
            Feed::of(&probes[i]),
        ) {
            Ok(spatial) => {
                probes[i].harletty_check = Some(spatial.clone());
                probes[i].spatial = spatial;
            }
            Err(message) => print_exit(
                ErrorKind::HarlettyInfo,
                Some(&escape(&format!("{}: {message}", file_name(&files[i])))),
            ),
        }
    }
    programs.harletty = Some(harletty);
}

/// Shown after the `-mo` of 7.1 and 7.0 input.
const NOTE_MEASURE_71: &str = "7.1 loudness of all channels; a 7.1 or 7.0 DDP encode sets its dialnorm from its 5.1 downmix, which can measure lower";

/// Whether `-mo` of this input is run through dee_shim: `dee_measure_loudness`
/// refuses 8 channels, `dee_shim measure_loudness --layout 7.1` measures them.
fn measure_by_shim(measure_only: bool, channels: i64) -> bool {
    measure_only && plan::wav_channels(channels) == 8
}

/// Whether a DD/DDP encode of a 5.0 source goes through dee_shim's 5.0 layout
/// (always: dee_shim is required). A wav (no pipe) then holds the 5 channels,
/// read with `--unpadded-wav`.
fn five_zero_by_shim(
    format: Format,
    layout: &Layout,
    source_channels: i64,
    measure_only: bool,
) -> bool {
    matches!(format, Format::Dd | Format::Ddp)
        && !measure_only
        && source_channels == 5
        && layout.lfe == Lfe::Removed
        && layout.outchannels == 6
}

/// Whether the run encodes or measures with dee_shim (`-v` aside). `by_shim`:
/// `-mo` of non-Atmos 8-channel input. `five_zero`: see [`five_zero_by_shim`].
#[allow(clippy::too_many_arguments)]
fn uses_shim(
    format: Format,
    drc: &str,
    apodise_off: bool,
    layout: &Layout,
    profile: Option<Profile71>,
    measure_only: bool,
    by_shim: bool,
    five_zero: bool,
) -> bool {
    if measure_only {
        return by_shim;
    }
    matches!(format, Format::DdpAtmos | Format::ThdAtmos)
        || (format == Format::Thd && (drc == "none" || apodise_off))
        || profile == Some(Profile71::Standard)
        || (format == Format::Ddp && layout.lfe == Lfe::Removed && layout.outchannels == 8)
        || five_zero
}

/// Before anything runs: two inputs must not make the same output file, and
/// no output may be an input. The paths are compared without regard to case
/// on every platform (they are the same file on Windows and macOS, and not
/// wanted on Linux either). `outputs` belongs to `files`, empty for `-mo`.
fn check_outputs(files: &[String], outputs: &[String]) {
    let cwd = std::env::current_dir().unwrap_or_default();
    let abs = |path: &String| tools::abspath(path, &cwd);
    let outs: Vec<String> = outputs.iter().map(abs).collect();
    let mut clashes = Vec::new();
    let mut seen = vec![false; outs.len()];
    for i in 0..outs.len() {
        if seen[i] {
            continue;
        }
        let same: Vec<usize> = (i..outs.len())
            .filter(|&j| output_name::same_path(&outs[i], &outs[j]))
            .collect();
        if same.len() > 1 {
            let names: Vec<String> = same.iter().map(|&j| escape(&files[j])).collect();
            clashes.push(format!("{}: {}", escape(&outputs[i]), names.join(", ")));
            for j in same {
                seen[j] = true;
            }
        }
    }
    if !clashes.is_empty() {
        print_exit(ErrorKind::OutputClash, Some(&clashes.join("\n")));
    }
    let inputs: Vec<String> = files.iter().map(abs).collect();
    let overwritten: Vec<String> = outs
        .iter()
        .enumerate()
        .flat_map(|(i, out)| {
            let outputs = &outputs;
            let files = &files;
            inputs
                .iter()
                .enumerate()
                .filter(|(_, input)| output_name::same_path(out, input))
                .map(move |(k, _)| {
                    format!(
                        "{} (the output of {})",
                        escape(&files[k]),
                        escape(&outputs[i])
                    )
                })
        })
        .collect();
    if !overwritten.is_empty() {
        print_exit(ErrorKind::OutputIsInput, Some(&overwritten.join("\n")));
    }
}

/// How the output files are named: the template (a literal name for `-on`).
struct OutputNaming {
    template: Template,
    /// `-on`: a single, literal name.
    single: bool,
}

/// The naming of this run, from `-on`, `-ot` and the config's template; exits
/// on an invalid or conflicting one. `-mo` writes no output, it names nothing.
fn output_naming(args: &Args, config: &Config) -> OutputNaming {
    let given = args.output_name.is_some() || args.output_template.is_some();
    if args.measure_only {
        if given {
            print_exit(ErrorKind::OutputNameMeasureOnly, None);
        }
        return OutputNaming {
            template: Template::parse(output_name::DEFAULT_TEMPLATE).expect("default template"),
            single: false,
        };
    }
    if args.output_name.is_some() && args.output_template.is_some() {
        print_exit(ErrorKind::OutputNameAndTemplate, None);
    }
    if let Some(name) = &args.output_name {
        if name.contains(['/', '\\']) {
            let message = escape("the name can't contain a path separator, use -o for the folder");
            print_exit(ErrorKind::OutputName, Some(&message));
        }
        if name.is_empty() {
            print_exit(ErrorKind::OutputName, Some(&escape("the name is empty")));
        }
        return OutputNaming {
            template: Template::literal(name),
            single: true,
        };
    }
    let text = args
        .output_template
        .as_deref()
        .unwrap_or(&config.output.template);
    match Template::parse(text) {
        Ok(template) => OutputNaming {
            template,
            single: false,
        },
        Err(message) => print_exit(ErrorKind::OutputTemplate, Some(&escape(&message))),
    }
}

struct Context<'a> {
    args: &'a Args,
    config: &'a Config,
    validated: &'a Validated,
    info: InputInfo,
    layout: &'a Layout,
    programs: &'a Programs,
    bitrate: Option<u32>,
    temp: &'a str,
    output: &'a str,
    /// `-oi`: every output goes to the folder of its input, `output` is unused.
    to_input: bool,
    naming: &'a OutputNaming,
    /// The 7.1 DDP profile, `None` for other outputs.
    profile: Option<Profile71>,
    versions: &'a Versions,
    /// The notices that were shown, for the report.
    notices: &'a [String],
    /// The run feeds DEE through the pipe where it can (decided once, see
    /// `PipeMode`); the jobs that can't pipe ignore it.
    pipe: bool,
}

/// Whether the job of `probe` can use the pipe at all: not TrueHD (reads wav
/// only), the Atmos formats, an Atmos master, nor AC4 IMS.
fn can_pipe(format: Format, measure_only: bool, probe: &Probe) -> bool {
    let plan_format = if measure_only { Format::Ddp } else { format };
    let format = if measure_only && atmos_source(probe) {
        Format::DdpAtmos
    } else {
        format
    };
    let atmos = format.is_atmos() || (format == Format::Ac4Ims && plan::ims_atmos_source(probe));
    !atmos && probe.master.is_none() && plan_format != Format::Thd && plan_format != Format::Ac4Ims
}

/// The "Piping" row of the summary and whether the run pipes.
fn pipe_decision(mode: PipeMode, windows: bool, instances: f64, pipeable: usize) -> (bool, String) {
    let parallel = (instances.ceil() as usize).min(pipeable);
    let pipe = mode.use_pipe(windows, parallel);
    let text = match (mode, pipe) {
        _ if pipeable == 0 => "not used".to_string(),
        (PipeMode::Never, _) => "off".to_string(),
        (PipeMode::Always, _) => "on (always)".to_string(),
        (PipeMode::Auto, true) => "on (auto)".to_string(),
        (PipeMode::Auto, false) => format!("off (auto: {parallel} at once)"),
    };
    (pipe, text)
}

fn versions(programs: &Programs) -> Versions {
    tools::probe_versions(
        Some(&programs.dee),
        programs.truehdd.as_deref(),
        programs.harletty.as_deref(),
        programs.shim.as_ref(),
        &programs.ffmpeg,
        &programs.ffprobe,
    )
}

/// The source row of the summary when every input is an Atmos master:
/// `ADM BWF Atmos master`, `DAMF Atmos master` or both.
fn master_source(probes: &[Probe]) -> Option<String> {
    let kinds: Vec<master::MasterKind> = probes.iter().filter_map(|p| p.master).collect();
    if kinds.is_empty() || kinds.len() != probes.len() {
        return None;
    }
    let mut labels: Vec<&str> = Vec::new();
    for kind in kinds {
        if !labels.contains(&kind.label()) {
            labels.push(kind.label());
        }
    }
    Some(labels.join(" / "))
}

/// The Auro-3D layout and carrier shared by all the inputs, `None` when one is
/// not Auro-3D or they differ.
fn common_auro(probes: &[Probe]) -> Option<(Option<String>, Option<String>)> {
    let key = |p: &Probe| {
        p.auro()
            .map(|s| (s.auro_layout.clone(), s.auro_carrier.clone()))
    };
    let first = key(probes.first()?)?;
    probes
        .iter()
        .all(|p| key(p).as_ref() == Some(&first))
        .then_some(first)
}

/// The "truehdd" row of the summary: the warp mode and the bed conformance
/// that are set, when the run decodes a TrueHD Atmos source with truehdd.
fn truehdd_summary(ctx: &Context, probes: &[Probe]) -> Option<String> {
    let uses_truehdd = (ctx.validated.format.has_truehdd() || ctx.args.measure_only)
        && probes.iter().any(|p| {
            p.master.is_none()
                && p.spatial.is_none()
                && (ctx.validated.format.is_atmos() || plan::ims_atmos_source(p))
        });
    if !uses_truehdd {
        return None;
    }
    let mut parts = Vec::new();
    if let Some(mode) = warp_mode(ctx) {
        parts.push(format!("warp mode {}", mode.name()));
    }
    if bed_conform_on(ctx) {
        parts.push("bed conform on".to_string());
    }
    (!parts.is_empty()).then(|| parts.join(", "))
}

/// The "harletty" row of the summary: the warp mode that is set and the bed
/// conformance (always on), when the run decodes a DTS:X or Auro-3D source.
fn harletty_summary(ctx: &Context, probes: &[Probe]) -> Option<String> {
    if !probes
        .iter()
        .any(|p| needs_harletty(ctx.validated.format, p))
    {
        return None;
    }
    let mut parts = Vec::new();
    if let Some(mode) = warp_mode(ctx) {
        parts.push(format!("warp mode {}", mode.name()));
    }
    parts.push("bed conform on".to_string());
    Some(parts.join(", "))
}

/// Print the summary table (if enabled).
fn print_summary(
    ctx: &Context,
    files: &[String],
    probes: &[Probe],
    outputs: &[String],
    instances: f64,
    piping: &str,
    versions: &Versions,
) {
    let sections = &ctx.config.summary_sections;
    if !summary::any_section(sections) {
        return;
    }
    let latest = if sections.deew_info {
        summary::latest_markup(env!("CARGO_PKG_VERSION"), summary::fetch_latest())
    } else {
        String::new()
    };
    let delay = if sections.other {
        summary::delay_summary(ctx.args.delay.as_deref(), files)
            .unwrap_or_else(|()| print_exit(ErrorKind::Delay, None))
    } else {
        String::new()
    };
    // only a name that is not the default one is worth showing
    let default_naming = !ctx.naming.single
        && Template::parse(output_name::DEFAULT_TEMPLATE).as_ref() == Ok(&ctx.naming.template);
    let output_names: Vec<String> = if default_naming {
        Vec::new()
    } else {
        outputs.iter().map(|o| file_name(o).to_string()).collect()
    };
    let data = SummaryData {
        sections,
        latest,
        versions: versions.clone(),
        channels: ctx.info.channels,
        master_source: master_source(probes),
        dtsx: !probes.is_empty() && probes.iter().all(Probe::is_dtsx),
        auro: common_auro(probes),
        mode: (ctx.validated.format == Format::Ac4Ims
            && ctx.validated.ims_mode == Ac4ImsMode::Music)
            .then_some("music"),
        sample_rates: &[ctx.info.samplerate],
        bit_depths: &ctx.info.bit_depths,
        measure_only: ctx.args.measure_only,
        format: ctx.validated.format,
        output_channels: if ctx.validated.format == Format::Ac4Ims {
            "immersive stereo".to_string()
        } else {
            ctx.layout.output_name()
        },
        profile: ctx.profile,
        bitrate: ctx.bitrate,
        dialnorm: ctx.validated.dialnorm,
        output_dir: if ctx.to_input {
            "next to the input files"
        } else {
            ctx.output
        },
        files: files.len(),
        output_names: &output_names,
        instances,
        piping: Some(piping.to_string()),
        truehdd: truehdd_summary(ctx, probes),
        harletty: harletty_summary(ctx, probes),
        delay,
        temp_path: ctx.temp,
    };
    let rows = summary::summary_rows(&data);
    let table = summary::render_table(
        "Encoding summary",
        &rows,
        crate::term::console_width().min(summary::MAX_WIDTH),
    );
    rprintln!("{}", table);
    println!();
}

/// The jobs built, with what is printed for them.
struct Prepared {
    jobs: Vec<Job>,
    /// The plan of every input that goes through ffmpeg.
    plans: Vec<AudioPlan>,
}

/// Paths of the files of a job in the temp directory.
struct TempNames {
    wav: String,
    atmos: String,
    loudness: String,
    ac4_manifest: String,
}

/// `variant` is `commands::intermediate_variant`: a wav made differently
/// has another name, so a kept one (`-k`) is never reused by mistake. The
/// same goes for `atmos_variant` (`truehdd_variant`) and the decoded `.atmos`.
fn temp_names(temp: &str, file: &str, variant: &str, atmos_variant: &str) -> TempNames {
    let name = |ext: &str, sanitize: bool| join(temp, &basename(file, ext, false, sanitize, false));
    let wav = if variant.is_empty() {
        "wav".to_string()
    } else {
        format!("{variant}.wav")
    };
    let atmos = if atmos_variant.is_empty() {
        "atmos".to_string()
    } else {
        format!("{atmos_variant}.atmos")
    };
    TempNames {
        wav: name(&wav, false),
        atmos: name(&atmos, true),
        loudness: name("loudness.json", false),
        ac4_manifest: name("ac4.json", false),
    }
}

/// The part of the name of a decoded `.atmos` that says how truehdd was
/// run: `wm-normal`, `bc`, `wm-loro.bc`; empty with neither option.
fn truehdd_variant(warp_mode: Option<WarpMode>, bed_conform: bool) -> String {
    let mut parts = Vec::new();
    if let Some(mode) = warp_mode {
        parts.push(format!("wm-{}", mode.name()));
    }
    if bed_conform {
        parts.push("bc".to_string());
    }
    parts.join(".")
}

/// truehdd's warp mode: `-wm`, else the config's `truehdd_warp_mode`.
fn warp_mode(ctx: &Context) -> Option<WarpMode> {
    ctx.validated.warp_mode.or(ctx.config.truehdd_warp_mode)
}

/// truehdd's bed conformance is on: `-bc`, else the config's `truehdd_bed_conform`.
fn bed_conform_on(ctx: &Context) -> bool {
    ctx.validated
        .bed_conform
        .unwrap_or(ctx.config.truehdd_bed_conform)
}

/// The files truehdd writes for `<name>.atmos`, and the `.caf` it writes
/// instead when the input has no Atmos presentation.
fn atmos_files(atmos: &str) -> Vec<String> {
    let prefix = atmos.strip_suffix(".atmos").unwrap_or(atmos);
    vec![
        atmos.to_string(),
        format!("{atmos}.metadata"),
        format!("{atmos}.audio"),
        format!("{prefix}.caf"),
    ]
}

impl Context<'_> {
    /// The output directory of an input.
    fn output_dir(&self, file: &str) -> String {
        if !self.to_input {
            return self.output.to_string();
        }
        let cwd = std::env::current_dir().unwrap_or_default();
        let full = tools::abspath(file, &cwd);
        Path::new(&full)
            .parent()
            .map_or(cwd.to_string_lossy().into_owned(), |p| {
                p.to_string_lossy().into_owned()
            })
    }

    /// The values of the placeholders of an output name.
    fn name_values(&self, file: &str, probe: &Probe) -> Values {
        let format = self.validated.format;
        let thd = matches!(format, Format::Thd | Format::ThdAtmos);
        Values {
            stem: file_stem(file, true),
            name: file_stem(file, false),
            format: format.name().to_string(),
            codec: format.summary_name().to_string(),
            channels: self.layout.name_label(format, self.validated.atmos_mode),
            bitrate: self
                .bitrate
                .filter(|_| !thd)
                .map(|b| b.to_string())
                .unwrap_or_default(),
            lang: probe.language.clone().unwrap_or_default(),
            title: probe.title.clone().unwrap_or_default(),
            track: self.args.track_index.to_string(),
        }
    }

    /// The path of the output of an input: the folder and the name.
    fn output_path(&self, file: &str, probe: &Probe) -> String {
        let ext = self
            .validated
            .format
            .extension(self.layout.outchannels, self.bluray());
        let name = self
            .naming
            .template
            .render(&self.name_values(file, probe), ext)
            .unwrap_or_else(|message| {
                let message = format!("{}: {message}", file_name(file));
                let kind = if self.naming.single {
                    ErrorKind::OutputName
                } else {
                    ErrorKind::OutputTemplate
                };
                print_exit(kind, Some(&escape(&message)))
            });
        join(&self.output_dir(file), &name)
    }

    /// Whether the output is the Blu-ray flavour (`.eb3`).
    fn bluray(&self) -> bool {
        self.profile == Some(Profile71::Bluray)
            || (self.validated.format == Format::DdpAtmos
                && self.validated.atmos_mode == DdpAtmosMode::Bluray)
    }
}

/// What the report of `-kl` shows besides the run itself.
fn log_spec(
    ctx: &Context,
    file: &str,
    probe: &Probe,
    bit_depth: &u32,
    output_path: &str,
) -> LogSpec {
    let row = |k: &str, v: String| (k.to_string(), v);
    let format = ctx.validated.format;
    let v = ctx.versions;
    let mut versions = vec![
        row("deew", env!("CARGO_PKG_VERSION").to_string()),
        row("DEE", v.dee.text()),
        row("truehdd", v.truehdd.text()),
    ];
    // only worth a line when it exists (it is optional, for DTS sources)
    if v.harletty != Version::NotFound {
        versions.push(row("harletty", v.harletty.text()));
    }
    versions.extend([
        row("dee_shim", v.shim.text()),
        row("ffmpeg", v.ffmpeg.text()),
        row("ffprobe", v.ffprobe.text()),
    ]);
    let duration = if probe.duration > 0.0 {
        format!("{:.3} s", probe.duration)
    } else {
        "unknown".to_string()
    };
    let input = if probe.master.is_some() {
        vec![
            row("Path", file.to_string()),
            row("Source", probe.codec.clone()),
            row("Duration", duration),
        ]
    } else {
        vec![
            row("Path", file.to_string()),
            row("Codec", probe.codec.clone()),
            row(
                "Channels",
                if probe.channel_layout.is_empty() {
                    probe.channels.to_string()
                } else {
                    format!("{} ({})", probe.channels, probe.channel_layout)
                },
            ),
            row("Sample rate", format!("{} Hz", probe.sample_rate)),
            row("Bit depth", bit_depth.to_string()),
            row("Duration", duration),
            row("Track index", ctx.validated.track_index.to_string()),
        ]
    };
    let shim_used = ctx.programs.shim.is_some()
        && (ctx.profile == Some(Profile71::Standard)
            || (format == Format::Ddp
                && ctx.layout.lfe == Lfe::Removed
                && ctx.layout.outchannels == 8)
            || matches!(format, Format::DdpAtmos | Format::ThdAtmos)
            || (format == Format::Thd && ctx.validated.drc == "none")
            || five_zero_by_shim(format, ctx.layout, ctx.info.channels, ctx.args.measure_only));
    let settings = if ctx.args.measure_only {
        let meter = if atmos_source(probe) {
            "dee_measure_loudness (Atmos master)"
        } else if measure_by_shim(true, ctx.info.channels) {
            "dee_shim measure_loudness (dee_measure_loudness)"
        } else {
            "dee_measure_loudness"
        };
        vec![
            row("Measure only", meter.to_string()),
            row("Temp path", ctx.temp.to_string()),
        ]
    } else {
        encode_settings(ctx, file, shim_used)
    };
    LogSpec {
        // next to the output, named like it without the extension
        path: format!(
            "{}.deew-log.html",
            output_path
                .rsplit_once('.')
                .map_or(output_path, |(stem, _)| stem)
        ),
        versions,
        input,
        settings,
        notices: ctx.notices.to_vec(),
    }
}

/// DEE's surround phase shift is off: `-sps` (else the config), DD/DDP/DDP Atmos only.
fn phase_shift_off(ctx: &Context) -> bool {
    ctx.validated.format.has_phase_shift()
        && !ctx
            .validated
            .surround_phase_shift
            .unwrap_or(ctx.config.surround_phase_shift)
}

/// DEE's LFE low-pass filter is off: `-lpf` (else the config), DD/DDP only.
fn low_pass_filter_off(ctx: &Context) -> bool {
    ctx.validated.format.has_dd_filters()
        && !ctx
            .validated
            .low_pass_filter
            .unwrap_or(ctx.config.low_pass_filter)
}

/// The surround 3 dB attenuation is on: `-s3a` (else the config), DD, DDP
/// and TrueHD only.
fn surround_3db_on(ctx: &Context) -> bool {
    ctx.validated.format.has_surround_3db()
        && ctx
            .validated
            .surround_3db_attenuation
            .unwrap_or(ctx.config.surround_3db_attenuation)
}

/// The settings table of the report of an encode.
fn encode_settings(ctx: &Context, file: &str, shim_used: bool) -> Vec<(String, String)> {
    let row = |k: &str, v: String| (k.to_string(), v);
    let format = ctx.validated.format;
    let mut rows = vec![
        row("Format", format.summary_name().to_string()),
        row(
            "Bitrate",
            ctx.bitrate
                .map_or("N/A".to_string(), |b| format!("{b} kbps")),
        ),
        row("DRC", ctx.validated.drc.clone()),
        row("Output channels", ctx.layout.output_name()),
        row(
            "LFE",
            match (ctx.layout.lfe, ctx.layout.outchannels) {
                (Lfe::Removed, _) => "removed",
                (Lfe::Silent, _) => "silent",
                (Lfe::Normal, 6 | 8) => "kept",
                _ => "n/a",
            }
            .to_string(),
        ),
        row(
            "Profile",
            match (format, ctx.profile) {
                (Format::DdpAtmos, _) => ctx.validated.atmos_mode.name().to_string(),
                (Format::Ac4Ims, _) => ctx.validated.ims_mode.name().to_string(),
                (_, profile) => profile.map_or("N/A".to_string(), |p| p.name().to_string()),
            },
        ),
        row(
            "Dialnorm",
            if ctx.validated.dialnorm == 0 {
                "auto (0)".to_string()
            } else {
                format!("{} dB", ctx.validated.dialnorm)
            },
        ),
        row(
            "Downmix",
            ctx.validated
                .downmix
                .map_or("none".to_string(), |d| d.to_string()),
        ),
        row("Delay", input_delay(file, ctx.args.delay.as_deref())),
        row("Temp path", ctx.temp.to_string()),
        row(
            "dee_shim used",
            if shim_used { "yes" } else { "no" }.to_string(),
        ),
    ];
    if phase_shift_off(ctx) {
        // after the Downmix row
        let at = rows
            .iter()
            .position(|(k, _)| k == "Downmix")
            .map_or(rows.len(), |i| i + 1);
        rows.insert(at, row("Surround phase shift", "off".to_string()));
    }
    let outchannels = ctx.layout.outchannels;
    if low_pass_filter_off(ctx) && outchannels >= 6 && ctx.layout.lfe != Lfe::Removed {
        let at = rows
            .iter()
            .position(|(k, _)| k == "Downmix")
            .map_or(rows.len(), |i| i + 1);
        rows.insert(at, row("LFE low-pass filter", "off".to_string()));
    }
    if surround_3db_on(ctx) && outchannels >= 6 {
        let at = rows
            .iter()
            .position(|(k, _)| k == "Downmix")
            .map_or(rows.len(), |i| i + 1);
        rows.insert(at, row("Surround 3 dB attenuation", "on".to_string()));
    }
    rows
}

/// DEE's own temp directory of job `index`: unique per deew process and job.
fn dee_temp_dir(temp: &str, pid: u32, index: usize) -> String {
    join(temp, &format!("dee-{pid}-{index}"))
}

/// The downmix that ffmpeg does for the encoders that can't: TrueHD (`-dm`)
/// and AC4 IMS (7.x input to its 5.1).
fn ffmpeg_downmix(ctx: &Context) -> Option<i64> {
    match ctx.validated.format {
        Format::Ac4Ims => {
            let out = ctx.layout.outchannels;
            (out != 0 && out != plan::wav_channels(ctx.info.channels)).then_some(out)
        }
        _ => ctx.validated.downmix,
    }
}

fn build_job(
    ctx: &Context,
    index: usize,
    file: &str,
    probe: &Probe,
    bit_depth: u32,
) -> (Job, Option<AudioPlan>) {
    let dee_temp = dee_temp_dir(ctx.temp, std::process::id(), index);
    let format = ctx.validated.format;
    let measure_only = ctx.args.measure_only;
    let dialnorm = ctx.validated.dialnorm;
    // `-mo` measures a wav as made for DDP, no downmix; a TrueHD Atmos source
    // is decoded like for DDP Atmos (`format` is DDP then, see `validate_args`).
    let plan_format = if measure_only { Format::Ddp } else { format };
    let measure_atmos = measure_only && atmos_source(probe);
    // an Atmos master goes to DEE as it is: no ffmpeg, no truehdd, nothing in the temp directory
    let master = probe.master.is_some();
    let downmix = if measure_only {
        None
    } else {
        ffmpeg_downmix(ctx)
    };
    // the 7.0 layouts of dee_shim read a wav of 7 channels, without the LFE
    // slot (`--unpadded-wav`). DEE takes no raw PCM of 7 channels, the pipe
    // keeps the silent LFE slot.
    let five_zero = five_zero_by_shim(plan_format, ctx.layout, ctx.info.channels, measure_only);
    let lfe_slot = if (five_zero && !ctx.pipe)
        || (ctx.layout.lfe == Lfe::Removed
            && plan_format == Format::Ddp
            && ctx.layout.outchannels == 8
            && !measure_only
            && !ctx.pipe)
    {
        LfeSlot::Dropped
    } else {
        (ctx.layout.lfe == Lfe::Silent).into()
    };
    let variant = intermediate_variant(
        plan_format,
        ctx.info.samplerate,
        ctx.info.channels,
        lfe_slot,
        downmix,
    );
    // a DTS:X or Auro-3D source is decoded by harletty, always with the bed conformed
    let harletty = needs_harletty(format, probe);
    let names = temp_names(
        ctx.temp,
        file,
        &variant,
        &truehdd_variant(warp_mode(ctx), harletty || bed_conform_on(ctx)),
    );
    let format = if measure_atmos {
        Format::DdpAtmos
    } else {
        format
    };
    // the audio is the `.atmos` file of truehdd (or the master): the Atmos
    // formats, and AC4 IMS from an Atmos source
    let atmos = format.is_atmos() || (format == Format::Ac4Ims && plan::ims_atmos_source(probe));
    let decoded = atmos && !master;
    let intermediate: &str = if master {
        file
    } else if atmos {
        &names.atmos
    } else {
        &names.wav
    };
    let via_manifest = dialnorm != 0 && format.dialnorm_via_manifest();
    // ffmpeg feeds DEE directly, there is no wav: not for TrueHD (its encoder
    // reads wav only), nor for the Atmos formats. When the audio is read
    // twice (AC4's manifest for a custom dialnorm: the measurement and then
    // the encode), ffmpeg feeds each DEE run
    let pcm_pipe = ctx.pipe
        && !atmos
        && !master
        && plan_format != Format::Thd
        // DEE's AC4 IMS reads no raw PCM
        && plan_format != Format::Ac4Ims;
    let pipe_path = pcm_pipe.then(|| pipe::path(index));
    let intermediate_exists = !pcm_pipe && !master && Path::new(intermediate).exists();
    let plan = (!atmos && !master).then(|| {
        let plan = audio_plan(
            plan_format,
            ctx.info.samplerate,
            ctx.info.channels,
            bit_depth,
            ctx.validated.track_index,
            lfe_slot,
            // the TrueHD and AC4 IMS encoders can't downmix, ffmpeg does
            downmix,
        );
        plan.with_dts_downmix(probe.dts_downmix)
    });
    let pcm_format = pcm_pipe.then(|| {
        let rate = pcm_rate(plan_format, ctx.info.samplerate);
        pcm_input_format(
            plan.as_ref().map_or(24, |plan| plan.bit_depth),
            rate,
            if lfe_slot == LfeSlot::Dropped {
                7
            } else {
                plan::wav_channels(ctx.info.channels)
            },
            pipe_sample_count(probe.duration, rate),
        )
    });
    let input_format = if atmos || master {
        "atmos_mezz"
    } else {
        pcm_format.as_deref().unwrap_or("wav")
    };
    // what DEE reads: the wav, the pipe, or the `.atmos` file
    let dee_input = pipe_path.as_deref().unwrap_or(intermediate);
    // truehdd and harletty read raw streams only: a raw .thd or .dts goes to
    // them directly, a track of a container is copied into their stdin by ffmpeg.
    let feed = Feed::of(probe);
    let piped = decoded
        && !(if harletty {
            !feed.piped()
        } else {
            probe.raw_truehd
        });
    let ffmpeg_args = if piped {
        if harletty {
            feed.ffmpeg_args(&ctx.programs.ffmpeg, file, ctx.validated.track_index)
                .unwrap_or_default()
        } else {
            pipe_args(
                &ctx.programs.ffmpeg,
                file,
                ctx.validated.track_index,
                "truehd",
            )
        }
    } else if pcm_pipe {
        plan.as_ref()
            .map(|plan| {
                ffmpeg_pipe_args(
                    &ctx.programs.ffmpeg,
                    plan,
                    file,
                    &pipe::ffmpeg_output(index),
                )
            })
            .unwrap_or_default()
    } else {
        plan.as_ref()
            .map(|plan| ffmpeg_args(&ctx.programs.ffmpeg, plan, file, &names.wav))
            .unwrap_or_default()
    };
    let decode_args = if decoded {
        let prefix = names.atmos.strip_suffix(".atmos").unwrap_or(&names.atmos);
        let input = if piped { "-" } else { file };
        if harletty {
            let tool = ctx.programs.harletty.as_deref().unwrap_or_default();
            harletty_decode_args(tool, input, prefix, warp_mode(ctx), feed.is_wav())
        } else {
            let truehdd = ctx.programs.truehdd.as_deref().unwrap_or_default();
            decode_args(truehdd, input, prefix, warp_mode(ctx), bed_conform_on(ctx))
        }
    } else {
        Vec::new()
    };
    let decoder = match &probe.spatial {
        Some(spatial) if harletty => Decoder::Harletty {
            experimental: spatial.experimental_warning(file_name(file)),
            // the duration of a raw stream is an estimate from its bitrate
            check_length: feed != Feed::RawDts,
        },
        _ => Decoder::Truehdd,
    };

    let output_path = ctx.output_path(file, probe);
    let measuring = measure_only || via_manifest;
    let skip_dialnorm =
        (dialnorm != 0 && !measure_only && !format.dialnorm_via_manifest()).then_some(dialnorm);
    let by_shim = !measure_atmos && measure_by_shim(measure_only, ctx.info.channels);
    let measure_args = match (&ctx.programs.measure, measuring) {
        _ if measure_atmos => {
            let tool = ctx.programs.measure_atmos.as_deref().unwrap_or_default();
            measure_atmos_args(tool, intermediate)
        }
        (Some(tool), true) if by_shim => {
            let shim = ctx.programs.shim.as_ref().map_or("", |shim| &shim.run);
            measure_shim_args(shim, tool, input_format, dee_input, &names.loudness)
        }
        (Some(tool), true) => measure_args(tool, input_format, dee_input, &names.loudness),
        _ => Vec::new(),
    };
    let encode_args = if measure_only {
        Vec::new()
    } else {
        let delay = input_delay(file, ctx.args.delay.as_deref());
        let delay = delay_options(
            &delay,
            format.encoder_delay(ctx.bitrate.unwrap_or(0), ctx.layout.dee_downmix),
        )
        .unwrap_or_else(|()| print_exit(ErrorKind::Delay, None));
        encode_args(&EncodeParams {
            format,
            tool: &ctx.programs.dee,
            input: dee_input,
            input_format,
            output: &output_path,
            bitrate: ctx.bitrate.unwrap_or(0),
            drc: &ctx.validated.drc,
            outchannels: ctx.layout.outchannels,
            layout_arg: ctx.layout.layout_arg,
            input_manifest: via_manifest.then_some(names.loudness.as_str()),
            skip_dialnorm,
            output_manifest: &names.ac4_manifest,
            delay: &delay,
            temp: &dee_temp,
            shim: ctx.programs.shim.as_ref().filter(|_| {
                matches!(
                    format,
                    Format::Dd | Format::Ddp | Format::DdpAtmos | Format::Thd | Format::ThdAtmos
                )
            }),
            bluray: ctx.profile != Some(Profile71::Standard),
            atmos_mode: ctx.validated.atmos_mode,
            lfe_removed: ctx.layout.lfe == Lfe::Removed,
            five_zero,
            no_phase_shift: phase_shift_off(ctx),
            no_lfe_filter: low_pass_filter_off(ctx),
            surround_3db: surround_3db_on(ctx),
            ims_music: ctx.validated.ims_mode == Ac4ImsMode::Music,
            apodise_off: apodise_off(format, ctx.info.samplerate),
        })
    };

    let mut temp_files = if master || (pcm_pipe && !atmos) {
        // a master is the user's file, never removed
        Vec::new()
    } else if atmos {
        atmos_files(&names.atmos)
    } else {
        vec![names.wav.clone()]
    };
    if !measure_args.is_empty() && !measure_atmos {
        temp_files.push(names.loudness.clone());
    }
    if matches!(format, Format::Ac4 | Format::Ac4Atmos) && !encode_args.is_empty() {
        temp_files.push(names.ac4_manifest.clone());
    }
    let log = ctx
        .args
        .keep_log
        .then(|| log_spec(ctx, file, probe, &bit_depth, &output_path));
    let job = Job {
        input: file.to_string(),
        length: probe.duration,
        format,
        ffmpeg_args,
        pipe: pipe_path,
        decode_args,
        decoder,
        measure_args,
        encode_args,
        measure_only,
        intermediate_exists,
        intermediate: intermediate.to_string(),
        loudness_path: names.loudness,
        dialnorm,
        skip_dialnorm,
        bitrate: ctx.bitrate,
        output_path,
        temp_files,
        decoded_files: Default::default(),
        dee_temp,
        log,
    };
    (job, plan)
}

fn prepare_jobs(ctx: &Context, files: &[String], probes: &[Probe]) -> Prepared {
    let mut prepared = Prepared {
        jobs: Vec::new(),
        plans: Vec::new(),
    };
    for (index, ((file, probe), &bit_depth)) in files
        .iter()
        .zip(probes)
        .zip(&ctx.info.bit_depths)
        .enumerate()
    {
        let (job, plan) = build_job(ctx, index, file, probe, bit_depth);
        prepared.jobs.push(job);
        prepared.plans.extend(plan);
    }
    prepared
}

/// The commands of one job as printed, the intermediate step replaced by a
/// note if it already exists.
fn job_commands(
    job: &Job,
    plans: &[AudioPlan],
    short: bool,
    ffmpeg_print: Option<String>,
) -> String {
    if job.pipe.is_some() {
        // ffmpeg and DEE run at the same time, the one DEE command of the job
        // reads the pipe
        let output = job.ffmpeg_args.last().map_or("-", String::as_str);
        let ffmpeg = ffmpeg_print.unwrap_or_else(|| ffmpeg_pipe_print_short(&plans[0], output));
        let dee = [&job.measure_args, &job.encode_args]
            .into_iter()
            .find(|args| !args.is_empty())
            .map_or_else(String::new, |args| print_command(args, short));
        return if pipe::NAMED {
            // DEE creates the pipe, so it starts first
            format!("{dee} && {ffmpeg}")
        } else {
            // the stdout of ffmpeg is the stdin of DEE
            format!("{ffmpeg} | {dee}")
        };
    }
    let mut parts = Vec::new();
    if job.intermediate_exists && !short {
        parts.push(INTERMEDIATE_EXISTS_PRINT.to_string());
    } else if !job.decode_args.is_empty() {
        let decode = print_command(&job.decode_args, short);
        if job.ffmpeg_args.is_empty() {
            parts.push(decode);
        } else {
            parts.push(format!(
                "{} | {decode}",
                print_command(&job.ffmpeg_args, short)
            ));
        }
    } else if job.ffmpeg_args.is_empty() {
        // an Atmos master goes straight into DEE
    } else if let Some(print) = ffmpeg_print {
        parts.push(print);
    } else {
        parts.push(ffmpeg_print_short(plans));
    }
    for args in [&job.measure_args, &job.encode_args] {
        if !args.is_empty() {
            parts.push(print_command(args, short));
        }
    }
    parts.join(" && ")
}

fn print_commands(prepared: &Prepared, long: bool) {
    if long {
        rprintln!("[bold color(231)]Running the following commands:[/bold color(231)]");
        let mut plans = prepared.plans.iter();
        for job in &prepared.jobs {
            let ffmpeg = if !job.decode_args.is_empty() || job.ffmpeg_args.is_empty() {
                None
            } else {
                plans.next().map(|plan| {
                    if job.pipe.is_some() {
                        ffmpeg_pipe_print(
                            plan,
                            &job.input,
                            job.ffmpeg_args.last().map_or("-", String::as_str),
                        )
                    } else {
                        ffmpeg_print(plan, &job.input, &job.intermediate)
                    }
                })
            };
            rprintln!("{}", job_commands(job, &prepared.plans, false, ffmpeg));
        }
    } else {
        let names: Vec<String> = prepared
            .jobs
            .iter()
            .filter(|job| job.intermediate_exists)
            .map(|job| escape(file_name(&job.input)))
            .collect();
        if !names.is_empty() {
            rprintln!(
                "[bold color(231)]Intermediate already exists for the following file(s):[/bold color(231)] [bold magenta]{}[/bold magenta]",
                names.join("[not bold white],[/not bold white] ")
            );
        }
        rprintln!(
            "[bold color(231)]Running the following commands:[/bold color(231)]\n{}",
            job_commands(&prepared.jobs[0], &prepared.plans, true, None)
        );
    }
    println!();
}

/// What happened to the jobs.
#[derive(Debug, Default)]
struct JobResults {
    /// Messages of the failed jobs.
    errors: Vec<String>,
    finished: usize,
    /// Interrupted while running.
    stopped: usize,
}

/// Run the jobs on `instances` workers. After a signal no job is started.
fn run_jobs(jobs: &[Job], options: &Options, instances: f64) -> JobResults {
    let names: Vec<&str> = jobs.iter().map(|job| file_name(&job.input)).collect();
    events::jobs(&names);
    let ui = Ui::new(jobs.len());
    // no sleep while encoding or measuring
    let _awake = crate::awake::keep_awake();
    interrupt::jobs_started(ui.printer());
    let workers = (instances.ceil() as usize).clamp(1, jobs.len().max(1));
    let next = AtomicUsize::new(0);
    let results = Mutex::new(JobResults::default());
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    if interrupt::is_interrupted() {
                        break;
                    }
                    let index = next.fetch_add(1, Ordering::SeqCst);
                    let Some(job) = jobs.get(index) else { break };
                    let bar = ui.add_bar(index, names[index]);
                    let outcome = encode::run_job(job, options, &bar, &ui);
                    let end = match &outcome {
                        Outcome::Done => End::Done,
                        Outcome::Failed(message) => End::Failed(message),
                        Outcome::Interrupted => End::Stopped,
                    };
                    ui.finish(bar, end);
                    let mut results = results.lock().expect("results");
                    match outcome {
                        Outcome::Done => results.finished += 1,
                        Outcome::Failed(message) => results.errors.push(message),
                        Outcome::Interrupted => results.stopped += 1,
                    }
                }
            });
        }
    });
    interrupt::jobs_ended();
    ui.flush();
    results.into_inner().expect("results")
}

pub fn run(args: Args, config: Config, paths: Paths) {
    let cwd = std::env::current_dir().unwrap_or_default();
    let binaries = tools::require_binaries(&paths.exe_dir, &cwd);

    let temp = tools::temp_path(&config.temp_path, &paths.exe_dir);
    create_dir(&temp);

    let cpu = std::thread::available_parallelism().map_or(1, usize::from);
    let instances = compute_instances(args.instances.as_deref(), &config.max_instances, cpu)
        .unwrap_or_else(|message| error_line(&escape(&message)));

    let validated = validate_args(&args, &config.default_modes).unwrap_or_else(|fail| fail.exit());
    let naming = output_naming(&args, &config);
    let format = validated.format;
    let inputs = collect_inputs(args.input.as_deref().unwrap_or_default());
    if inputs.is_empty() {
        error_line("no input files found.");
    }
    if naming.single && inputs.len() != 1 {
        print_exit(ErrorKind::OutputNameSingle, Some(&inputs.len().to_string()));
    }
    let needs_measure =
        args.measure_only || (validated.dialnorm != 0 && validated.format.dialnorm_via_manifest());
    let mut programs = find_programs(
        binaries,
        &paths.exe_dir,
        format,
        &validated.drc,
        needs_measure,
        args.measure_only,
    );

    let atmos_info = tools::dee_tool("atmos_info", &cwd, &paths.exe_dir);
    let (files, mut probes) = probe_inputs(
        &programs.ffprobe,
        atmos_info.as_deref(),
        validated.track_index,
        args.no_prompt,
        inputs,
    );
    check_auro(
        &programs.ffmpeg,
        programs.harletty.as_deref(),
        files
            .iter()
            .map(String::as_str)
            .zip(probes.iter_mut())
            .collect(),
        validated.track_index,
    );
    if (format.is_atmos() || format == Format::Ac4Ims) && !args.measure_only {
        detect_spatial(
            &mut programs,
            &paths.exe_dir,
            &cwd,
            format,
            &files,
            &mut probes,
            validated.track_index,
        );
    }
    let info = check_inputs(&probes).unwrap_or_else(|fail| fail.exit());
    if format.is_atmos() && !args.measure_only {
        check_atmos_inputs(&files, &probes).unwrap_or_else(|fail| fail.exit());
    }
    check_master_inputs(format, &probes, args.measure_only).unwrap_or_else(|fail| fail.exit());
    // an Atmos source of AC4 IMS is laid out like AC4 Atmos
    let ims_atmos = format == Format::Ac4Ims && probes.iter().any(plan::ims_atmos_source);
    if format == Format::Ac4Ims && !args.measure_only {
        check_ims_inputs(&probes).unwrap_or_else(|fail| fail.exit());
    }
    if probes.iter().any(|p| needs_truehdd(format, p)) && programs.truehdd.is_none() {
        programs.truehdd = Some(tools::require_truehdd(&paths.exe_dir, &cwd));
    }
    check_input_layouts(&files, &probes).unwrap_or_else(|fail| fail.exit());
    let layout = resolve_layout(
        if ims_atmos { Format::Ac4Atmos } else { format },
        validated.downmix,
        info.channels,
        args.disable_lfe,
    )
    .unwrap_or_else(|fail| fail.exit());
    check_shim_for_ddp(
        format,
        &layout,
        validated.ddp_mode,
        args.measure_only,
        programs.shim.is_some(),
        info.channels,
    )
    .unwrap_or_else(|fail| fail.exit());
    if !args.measure_only {
        for note in &layout.notes {
            events::warning(&markup::strip(note));
            rprintln!("{}", note);
        }
    }
    plan::check_shim_for_thd96(
        apodise_off(format, info.samplerate),
        args.measure_only,
        programs.shim.is_some(),
    )
    .unwrap_or_else(|fail| fail.exit());
    if args.measure_only {
        let atmos = probes.iter().any(atmos_source);
        if atmos {
            let tool = tools::require_dee_tool("dee_measure_loudness", &cwd, &paths.exe_dir);
            programs.dee = tool.clone();
            programs.measure_atmos = Some(tool);
            // only TrueHD is decoded, the masters are measured as they are
            if probes.iter().any(|p| p.master.is_none() && atmos_source(p)) {
                programs.truehdd = Some(tools::require_truehdd(&paths.exe_dir, &cwd));
            }
        }
        if !probes.iter().all(atmos_source) {
            plan::check_shim_for_measure(
                measure_by_shim(true, info.channels),
                programs.shim.is_some(),
            )
            .unwrap_or_else(|fail| fail.exit());
            let tool = tools::require_dee_tool("dee_measure_loudness", &cwd, &paths.exe_dir);
            programs.dee = tool.clone();
            programs.measure = Some(tool);
        }
    }
    // only the AC4 `-dn` is bound to dee_measure_loudness
    check_ac4_dialnorm_channels(info.channels, needs_measure && !args.measure_only)
        .unwrap_or_else(|fail| fail.exit());

    let profile = if args.measure_only {
        None
    } else {
        profile_71(format, layout.outchannels, validated.ddp_mode)
    };
    if let Some(truehdd) = &programs.truehdd
        && (probes.iter().any(|p| needs_truehdd(format, p))
            || (args.measure_only && probes.iter().any(|p| p.master.is_none() && atmos_source(p))))
    {
        tools::require_truehdd_version(truehdd);
    }
    if !args.measure_only {
        let ignored = [
            (
                args.ddp_mode.is_some() && profile.is_none(),
                "--ddp-mode",
                "7.1 DDP",
            ),
            (
                args.ddp_atmos_mode.is_some() && format != Format::DdpAtmos,
                "--ddp-atmos-mode",
                "DDP Atmos",
            ),
            (
                args.ac4_ims_mode.is_some() && format != Format::Ac4Ims,
                "--ac4-ims-mode",
                "AC4 IMS",
            ),
        ];
        for (is_ignored, flag, what) in ignored {
            if is_ignored {
                let note = format!(
                    "[bold yellow]{flag}[/bold yellow] only applies to {what} output, it is ignored."
                );
                events::warning(&markup::strip(&note));
                rprintln!("{}", note);
            }
        }
    }
    let shown = notices(
        format,
        layout.outchannels,
        args.measure_only,
        validated.dialnorm,
        validated.format.has_surround_3db()
            && validated
                .surround_3db_attenuation
                .unwrap_or(config.surround_3db_attenuation),
    );
    show_notices(&shown, args.no_prompt);
    let notice_texts: Vec<String> = shown.iter().map(|n| markup::strip(n.note)).collect();
    let versions = versions(&programs);

    let bitrate = select_bitrate(
        format,
        layout.outchannels,
        Modes {
            ddp: validated.ddp_mode,
            atmos: validated.atmos_mode,
        },
        args.bitrate,
        &config.default_bitrates,
    );
    let to_input = args.output_to_input || config.output.to_input;
    let output = if to_input {
        String::new()
    } else {
        output_directory(args.output.as_deref(), &config.output.path)
    };

    let pipeable = probes
        .iter()
        .filter(|probe| can_pipe(format, args.measure_only, probe))
        .count();
    let (pipe, piping) = pipe_decision(
        validated.pipe_mode.unwrap_or(config.pipe),
        pipe::NAMED,
        instances,
        pipeable,
    );
    // the versions of the tools that run, checked once (`-v` only reports)
    let atmos_sources = probes.iter().all(atmos_source);
    if uses_shim(
        format,
        &validated.drc,
        apodise_off(format, info.samplerate),
        &layout,
        profile,
        args.measure_only,
        !atmos_sources && measure_by_shim(true, info.channels),
        five_zero_by_shim(format, &layout, info.channels, args.measure_only),
    ) && let Some(shim) = &programs.shim
    {
        tools::require_shim_version(shim);
    }
    let ctx = Context {
        args: &args,
        config: &config,
        validated: &validated,
        info,
        layout: &layout,
        programs: &programs,
        bitrate,
        temp: &temp,
        output: &output,
        to_input,
        naming: &naming,
        profile,
        versions: &versions,
        notices: &notice_texts,
        pipe,
    };

    let outputs: Vec<String> = if args.measure_only {
        Vec::new()
    } else {
        files
            .iter()
            .zip(&probes)
            .map(|(file, probe)| ctx.output_path(file, probe))
            .collect()
    };
    check_outputs(&files, &outputs);
    print_summary(
        &ctx, &files, &probes, &outputs, instances, &piping, &versions,
    );
    let prepared = prepare_jobs(&ctx, &files, &probes);
    print_commands(&prepared, args.long_argument);

    let options = Options {
        keep_temp: args.keeptemp,
        keep_log: args.keep_log,
    };
    let results = run_jobs(&prepared.jobs, &options, instances);
    if args.measure_only
        && results.finished > 0
        && !atmos_sources
        && measure_by_shim(true, ctx.info.channels)
    {
        rprintln!("[dim]{}[/dim]", escape(NOTE_MEASURE_71));
    }
    for message in &results.errors {
        // the `failed:` line of the job already showed it
        events::error(message);
    }
    if interrupt::is_interrupted() {
        let failed = results.errors.len();
        let not_started = prepared.jobs.len() - results.finished - failed - results.stopped;
        let line = interrupt::final_line(results.finished, failed, results.stopped, not_started);
        rprintln!("[bold red]{}[/bold red]", escape(&line));
        std::process::exit(interrupt::EXIT_CODE);
    }
    if !results.errors.is_empty() {
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_files_are_named_after_the_input() {
        let names = temp_names("/t", "/in/My Song DELAY -5ms.flac", "", "");
        assert_eq!(names.wav, join("/t", "My Song DELAY -5ms.wav"));
        assert_eq!(names.atmos, join("/t", "My_Song_DELAY_-5ms.atmos"));
        assert_eq!(
            names.loudness,
            join("/t", "My Song DELAY -5ms.loudness.json")
        );
        assert_eq!(
            names.ac4_manifest,
            join("/t", "My Song DELAY -5ms.ac4.json")
        );
        let names = temp_names("/t", "/in/a [1].flac", "thd.dm6.dl", "");
        assert_eq!(names.wav, join("/t", "a [1].thd.dm6.dl.wav"));
        assert_eq!(names.atmos, join("/t", "a_[1].atmos"));
    }

    #[test]
    fn decoded_atmos_name_has_the_truehdd_options() {
        let atmos =
            |warp, bed| temp_names("/t", "/in/My Song.thd", "", &truehdd_variant(warp, bed)).atmos;
        assert_eq!(atmos(None, false), join("/t", "My_Song.atmos"));
        assert_eq!(
            atmos(Some(WarpMode::Normal), false),
            join("/t", "My_Song.wm-normal.atmos")
        );
        assert_eq!(atmos(None, true), join("/t", "My_Song.bc.atmos"));
        assert_eq!(
            atmos(Some(WarpMode::Loro), true),
            join("/t", "My_Song.wm-loro.bc.atmos")
        );
        // the files truehdd writes follow from the name
        let files = atmos_files(&atmos(Some(WarpMode::Loro), true));
        assert!(files.contains(&join("/t", "My_Song.wm-loro.bc.atmos.audio")));
        assert!(files.contains(&join("/t", "My_Song.wm-loro.bc.atmos.metadata")));
    }

    #[test]
    fn dee_temp_dir_is_per_process_and_job() {
        assert_eq!(dee_temp_dir("/t", 42, 3), join("/t", "dee-42-3"));
        assert_ne!(dee_temp_dir("/t", 42, 0), dee_temp_dir("/t", 43, 0));
    }

    #[test]
    fn truehdd_writes_three_files() {
        assert_eq!(
            atmos_files("/t/a.atmos"),
            [
                "/t/a.atmos",
                "/t/a.atmos.metadata",
                "/t/a.atmos.audio",
                "/t/a.caf"
            ]
        );
    }

    fn test_probe(channels: i64, atmos: bool) -> Probe {
        Probe {
            sample_rate: 48000,
            channels,
            duration: 10.0,
            bit_depth: 24,
            is_truehd: atmos,
            channel_layout: String::new(),
            codec: if atmos { "truehd" } else { "pcm_s24le" }.into(),
            raw_truehd: false,
            raw_dts: false,
            profile: if atmos {
                "Dolby TrueHD + Dolby Atmos"
            } else {
                ""
            }
            .into(),
            is_atmos: atmos,
            master: None,
            language: None,
            title: None,
            dts_downmix: None,
            spatial: None,
            harletty_check: None,
        }
    }

    /// The job `-mo` makes of `file`, with the given other options.
    fn measure_job(file: &str, probe: &Probe, edit: impl FnOnce(&mut Args)) -> Job {
        let mut args = Args {
            measure_only: true,
            ..Args::default()
        };
        edit(&mut args);
        test_job(file, probe, args, true)
    }

    /// The job of `file` for `args`, with `pipe` in the config.
    fn test_job(file: &str, probe: &Probe, args: Args, pipe: bool) -> Job {
        test_job_with(file, probe, args, pipe, None)
    }

    /// `test_job` with the 7.1 profile of the run; a profile comes with dee_shim.
    fn test_job_with(
        file: &str,
        probe: &Probe,
        args: Args,
        pipe: bool,
        profile: Option<Profile71>,
    ) -> Job {
        let mut config: Config = toml::from_str(crate::config::CONFIG_TEMPLATE).unwrap();
        config.pipe = if pipe {
            PipeMode::Always
        } else {
            PipeMode::Never
        };
        let validated = validate_args(&args, &config.default_modes).unwrap();
        let layout = resolve_layout(
            validated.format,
            validated.downmix,
            probe.channels,
            args.disable_lfe,
        )
        .unwrap();
        let programs = Programs {
            ffmpeg: "ffmpeg".into(),
            ffprobe: "ffprobe".into(),
            dee: "dee_x".into(),
            measure: Some("dee_measure_loudness".into()),
            measure_atmos: Some("dee_measure_loudness".into()),
            truehdd: Some("truehdd".into()),
            harletty: Some("harletty".into()),
            shim: (profile.is_some() || args.measure_only).then(|| crate::tools::Shim {
                run: "dee_shim".into(),
            }),
        };
        let versions = Versions::default();
        let naming = output_naming(&args, &config);
        let ctx = Context {
            info: check_inputs(std::slice::from_ref(probe)).unwrap(),
            args: &args,
            config: &config,
            validated: &validated,
            layout: &layout,
            programs: &programs,
            bitrate: None,
            temp: "/t",
            output: "/o",
            to_input: false,
            naming: &naming,
            profile,
            versions: &versions,
            notices: &[],
            pipe,
        };
        build_job(&ctx, 0, file, probe, 24).0
    }

    #[test]
    fn pipe_decision_texts() {
        let d = |mode, windows, instances, n| pipe_decision(mode, windows, instances, n);
        assert_eq!(
            d(PipeMode::Auto, true, 4.0, 4),
            (false, "off (auto: 4 at once)".into())
        );
        assert_eq!(d(PipeMode::Auto, true, 4.0, 1), (true, "on (auto)".into()));
        assert_eq!(d(PipeMode::Auto, true, 1.0, 4), (true, "on (auto)".into()));
        assert_eq!(
            d(PipeMode::Auto, true, 2.5, 4),
            (false, "off (auto: 3 at once)".into())
        );
        assert_eq!(d(PipeMode::Auto, false, 8.0, 8), (true, "on (auto)".into()));
        assert_eq!(
            d(PipeMode::Always, true, 8.0, 8),
            (true, "on (always)".into())
        );
        assert_eq!(d(PipeMode::Never, false, 1.0, 1), (false, "off".into()));
        assert_eq!(d(PipeMode::Auto, true, 4.0, 0).1, "not used");
    }

    #[test]
    fn the_run_uses_dee_shim_for_its_layouts() {
        let layout = |outchannels, lfe| Layout {
            outchannels,
            layout_arg: None,
            lfe,
            dee_downmix: false,
            notes: Vec::new(),
        };
        let uses = |format, drc, apodise, layout: &Layout, profile, measure_only, by_shim| {
            uses_shim(
                format,
                drc,
                apodise,
                layout,
                profile,
                measure_only,
                by_shim,
                false,
            )
        };
        let l51 = layout(6, Lfe::Normal);
        let l71 = layout(8, Lfe::Normal);
        let l70 = layout(8, Lfe::Removed);
        let standard = Some(Profile71::Standard);
        let bluray = Some(Profile71::Bluray);
        // a 5.0 source of DD/DDP does (`five_zero`), -mo does not
        let l50 = layout(6, Lfe::Removed);
        assert!(uses_shim(
            Format::Ddp,
            "none",
            false,
            &l50,
            None,
            false,
            false,
            true
        ));
        assert!(!uses_shim(
            Format::Ddp,
            "none",
            false,
            &l50,
            None,
            true,
            false,
            true
        ));
        // plain encodes don't
        assert!(!uses(Format::Ddp, "none", false, &l51, None, false, false));
        assert!(!uses(
            Format::Ddp,
            "none",
            false,
            &l71,
            bluray,
            false,
            false
        ));
        assert!(!uses(
            Format::Thd,
            "speech",
            false,
            &l51,
            None,
            false,
            false
        ));
        // standard 7.1 and both 7.0 profiles, TrueHD without DRC or at 96 kHz, Atmos
        assert!(uses(
            Format::Ddp,
            "none",
            false,
            &l71,
            standard,
            false,
            false
        ));
        assert!(uses(Format::Ddp, "none", false, &l70, bluray, false, false));
        assert!(uses(Format::Thd, "none", false, &l51, None, false, false));
        assert!(uses(Format::Thd, "speech", true, &l51, None, false, false));
        assert!(uses(
            Format::DdpAtmos,
            "none",
            false,
            &l51,
            None,
            false,
            false
        ));
        // -mo only for 8 channels
        assert!(uses(Format::Ddp, "none", false, &l71, None, true, true));
        assert!(!uses(Format::Ddp, "none", false, &l51, None, true, false));
    }

    #[test]
    fn apodise_off_follows_the_encoded_rate() {
        for (rate, off) in [(44100, false), (48000, false), (88200, true), (96000, true)] {
            assert_eq!(apodise_off(Format::Thd, rate), off, "{rate}");
        }
        assert!(!apodise_off(Format::ThdAtmos, 96000));
        assert!(!apodise_off(Format::Ddp, 96000));
    }

    #[test]
    fn thd_at_96khz_is_encoded_without_the_apodising_filter() {
        for (rate, off) in [
            (44100, false),
            (48000, false),
            (88200, true),
            (96000, true),
            (176400, true),
            (192000, true),
        ] {
            let mut probe = test_probe(6, false);
            probe.sample_rate = rate;
            let args = Args {
                format: "thd".into(),
                drc: Some("music_light".into()),
                ..Args::default()
            };
            let job = test_job_with("/in/a.wav", &probe, args, false, Some(Profile71::Standard));
            assert_eq!(
                job.encode_args.contains(&"--apodise".to_string()),
                off,
                "{rate}"
            );
            assert!(
                job.encode_args
                    .contains(&"6ch:drc_profile=music_light".to_string()),
                "{rate}"
            );
        }
    }

    #[test]
    fn measuring_an_atmos_source_decodes_and_measures_the_master() {
        let probe = test_probe(8, true);
        for format in ["ddp", "ddp-atmos", "thd"] {
            let job = measure_job("/in/a.mkv", &probe, |a| a.format = format.into());
            assert!(job.format.is_atmos(), "{format}");
            // a container track is copied into truehdd
            assert_eq!(job.ffmpeg_args[0], "ffmpeg");
            assert_eq!(job.decode_args[0], "truehdd");
            assert_eq!(job.decode_args.last().map(String::as_str), Some("-"));
            assert!(job.decode_args.contains(&join("/t", "a")));
            assert_eq!(
                job.measure_args,
                measure_atmos_args("dee_measure_loudness", &join("/t", "a.atmos"))
            );
            assert!(!job.measure_args.contains(&"--temp-dir".to_string()));
            assert!(job.encode_args.is_empty());
            assert!(job.measure_only);
            assert!(job.temp_files.contains(&join("/t", "a.atmos")));
            assert!(!job.temp_files.contains(&job.loudness_path));
        }
        // a raw stream goes to truehdd directly
        let mut raw = probe.clone();
        raw.raw_truehd = true;
        let job = measure_job("/in/a.thd", &raw, |_| {});
        assert!(job.ffmpeg_args.is_empty());
        assert!(job.decode_args.contains(&"/in/a.thd".to_string()));
    }

    fn auro_probe(layout: Option<&str>, carrier: &str) -> Probe {
        let mut probe = dts_probe(true, true);
        let mut spatial = probe.spatial.take().unwrap();
        spatial.kind = "auro".into();
        spatial.auro_layout = layout.map(str::to_string);
        spatial.auro_carrier = Some(carrier.into());
        probe.harletty_check = Some(Some(spatial));
        probe
    }

    #[test]
    fn auro_is_shown_when_all_inputs_agree() {
        let a = auro_probe(Some("13.1"), "7.1");
        assert_eq!(
            common_auro(&[a.clone(), a.clone()]),
            Some((Some("13.1".into()), Some("7.1".into())))
        );
        assert_eq!(
            common_auro(&[a.clone(), auro_probe(Some("11.1"), "7.1")]),
            None
        );
        assert_eq!(common_auro(&[a.clone(), dts_probe(false, true)]), None);
        assert_eq!(common_auro(&[]), None);
        // no harletty: nothing is asked, nothing changes
        let mut plain = dts_probe(false, true);
        plain.profile = "DTS-HD MA".into();
        plain.bit_depth = 24;
        assert!(plain.auro_candidate());
        check_auro(
            "ffmpeg",
            None,
            vec![("a.dts", &mut plain)],
            TrackIndex::default(),
        );
        assert_eq!(plain.harletty_check, None);
        // a harletty that is not there
        check_auro(
            "ffmpeg",
            Some("/nonexistent/harletty"),
            vec![("a.dts", &mut plain)],
            TrackIndex::default(),
        );
        assert_eq!(plain.harletty_check, None);
    }

    /// A DTS track that harletty found DTS:X in (`kind` `dtsx`) or not.
    fn dts_probe(spatial: bool, raw: bool) -> Probe {
        Probe {
            is_truehd: false,
            codec: "dts".into(),
            raw_dts: raw,
            is_atmos: false,
            profile: "DTS-HD MA".into(),
            dts_downmix: None,
            spatial: spatial.then(|| crate::harletty::Spatial {
                label: "DTS:X-7.1.4+3".into(),
                kind: "dtsx".into(),
                objects: Some(3),
                experimental: true,
                presentation: Some("ObjectsD0".into()),
                auro_layout: None,
                auro_carrier: None,
            }),
            harletty_check: None,
            ..test_probe(8, false)
        }
    }

    /// The job of an Atmos format (DDP Atmos needs dee_shim, a profile has it).
    fn atmos_job(file: &str, probe: &Probe, edit: impl FnOnce(&mut Args)) -> Job {
        let mut args = Args::default();
        edit(&mut args);
        test_job_with(file, probe, args, false, Some(Profile71::Standard))
    }

    #[test]
    fn auro_in_pcm_is_decoded_by_harletty_through_a_wav_pipe() {
        for (codec, truehd) in [("flac", false), ("truehd", true)] {
            let mut probe = dts_probe(true, false);
            probe.codec = codec.into();
            probe.is_truehd = truehd;
            probe.profile = String::new();
            probe.spatial.as_mut().unwrap().kind = "auro".into();
            probe.spatial.as_mut().unwrap().experimental = false;
            for format in ["ddp-atmos", "thd-atmos", "ac4-atmos", "ac4-ims"] {
                let job = atmos_job("a.mkv", &probe, |a| a.format = format.into());
                let ffmpeg = &job.ffmpeg_args;
                assert!(
                    ffmpeg.ends_with(&["-c:a", "pcm_s24le", "-f", "wav", "-"].map(String::from)),
                    "{format}"
                );
                assert_eq!(ffmpeg.contains(&"-drc_scale".to_string()), truehd);
                assert!(!ffmpeg.contains(&"copy".to_string()));
                assert_eq!(job.decode_args[0], "harletty");
                assert_eq!(job.decode_args[1..3], ["--codec", "wav"].map(String::from));
                assert!(!job.decode_args.contains(&"dts".to_string()));
                assert_eq!(job.decode_args.last().map(String::as_str), Some("-"));
                assert_eq!(
                    job.decoder,
                    Decoder::Harletty {
                        experimental: None,
                        check_length: true
                    }
                );
            }
        }
    }

    #[test]
    fn dtsx_sources_of_the_atmos_formats_are_decoded_by_harletty() {
        for format in ["ddp-atmos", "thd-atmos", "ac4-atmos"] {
            // a track of a container is copied into harletty
            let job = atmos_job("/in/a.mkv", &dts_probe(true, false), |a| {
                a.format = format.into()
            });
            assert_eq!(job.ffmpeg_args.last().map(String::as_str), Some("-"));
            assert!(job.ffmpeg_args.contains(&"dts".to_string()), "{format}");
            assert!(!job.ffmpeg_args.contains(&"truehd".to_string()));
            assert_eq!(job.decode_args[0], "harletty");
            assert_eq!(job.decode_args.last().map(String::as_str), Some("-"));
            assert!(job.decode_args.contains(&"--bed-conform".to_string()));
            assert!(job.decode_args.contains(&join("/t", "a.bc")));
            assert_eq!(
                job.decoder,
                Decoder::Harletty {
                    experimental: Some(
                        "a.mkv: DTS:X-7.1.4+3 (ObjectsD0) is decoded experimentally by harletty, check the result."
                            .into()
                    ),
                    check_length: true,
                }
            );
            // from here on it is a decoded Atmos master as for truehdd
            assert_eq!(job.intermediate, join("/t", "a.bc.atmos"));
            assert!(job.temp_files.contains(&join("/t", "a.bc.atmos.audio")));
            assert!(job.encode_args.contains(&job.intermediate), "{format}");
        }
        // a raw stream goes to harletty directly, its duration is an estimate
        let job = atmos_job("/in/a.dts", &dts_probe(true, true), |a| {
            a.format = "ddp-atmos".into()
        });
        assert!(job.ffmpeg_args.is_empty());
        assert_eq!(
            job.decode_args.last().map(String::as_str),
            Some("/in/a.dts")
        );
        assert!(matches!(
            job.decoder,
            Decoder::Harletty {
                check_length: false,
                ..
            }
        ));
        // the bed is conformed whatever -bc says, the warp mode is passed
        let job = atmos_job("/in/a.mkv", &dts_probe(true, false), |a| {
            a.format = "thd-atmos".into();
            a.bed_conform = Some("off".into());
            a.warp_mode = Some("loro".into());
        });
        let i = job
            .decode_args
            .iter()
            .position(|a| a == "--warp-mode")
            .unwrap();
        assert_eq!(job.decode_args[i + 1], "loro");
        assert!(job.decode_args.contains(&"--bed-conform".to_string()));
        assert!(job.intermediate.ends_with("a.wm-loro.bc.atmos"));
    }

    #[test]
    fn dtsx_sources_of_ac4_ims_are_decoded_by_harletty() {
        let job = encode_job("/in/a.mkv", &dts_probe(true, false), false, |a| {
            a.format = "ac4-ims".into();
            a.bed_conform = Some("off".into());
            a.warp_mode = Some("loro".into());
        });
        assert_eq!(job.decode_args[0], "harletty");
        assert!(job.decode_args.contains(&"--bed-conform".to_string()));
        assert!(job.decode_args.contains(&"--warp-mode".to_string()));
        assert!(matches!(job.decoder, Decoder::Harletty { .. }));
        assert!(job.intermediate.ends_with("a.wm-loro.bc.atmos"));
        assert!(job.encode_args.contains(&job.intermediate));
        assert!(needs_harletty(Format::Ac4Ims, &dts_probe(true, false)));
        assert!(!needs_truehdd(Format::Ac4Ims, &dts_probe(true, false)));
        // a plain DTS track stays channel-based
        let job = encode_job("/in/a.mkv", &dts_probe(false, false), false, |a| {
            a.format = "ac4-ims".into()
        });
        assert!(job.decode_args.is_empty());
        assert!(!needs_harletty(Format::Ac4Ims, &dts_probe(false, false)));
        assert!(plan::ims_atmos_source(&dts_probe(true, false)));
        assert!(!plan::ims_atmos_source(&dts_probe(false, false)));
    }

    #[test]
    fn dts_goes_through_ffmpeg_for_the_other_formats_and_measuring() {
        for format in ["dd", "ddp", "thd", "ac4", "ac4-ims", "aac"] {
            let job = encode_job("/in/a.mkv", &dts_probe(false, false), false, |a| {
                a.format = format.into()
            });
            assert!(job.decode_args.is_empty(), "{format}");
            assert_eq!(job.decoder, Decoder::Truehdd);
            assert!(!needs_harletty(job.format, &dts_probe(false, false)));
        }
        // even a DTS:X track: harletty is not run for them (AC4 IMS has its own test)
        let probe = dts_probe(true, false);
        for format in [Format::Dd, Format::Ddp, Format::Thd] {
            assert!(!needs_harletty(format, &probe));
        }
        let job = measure_job("/in/a.mkv", &probe, |_| {});
        assert!(job.decode_args.is_empty());
        assert_eq!(job.ffmpeg_args[0], "ffmpeg");
        assert!(!job.ffmpeg_args.contains(&"copy".to_string()));
        // the Atmos formats of a TrueHD source stay with truehdd
        assert!(needs_truehdd(Format::DdpAtmos, &test_probe(8, true)));
        assert!(!needs_truehdd(Format::DdpAtmos, &probe));
        assert!(needs_harletty(Format::DdpAtmos, &probe));
        assert!(!needs_harletty(Format::DdpAtmos, &dts_probe(false, false)));
        assert!(!needs_harletty(Format::DdpAtmos, &test_probe(8, true)));
    }

    #[test]
    fn measuring_other_sources_is_chosen_by_the_source() {
        // E-AC-3 JOC is channel based
        let mut joc = test_probe(6, true);
        joc.is_truehd = false;
        joc.codec = "eac3".into();
        let job = measure_job("/in/a.ec3", &joc, |a| a.format = "ddp-atmos".into());
        assert!(!job.format.is_atmos() && job.decode_args.is_empty());
        assert_eq!(job.measure_args[0], "dee_measure_loudness");
        // 8 channels go through dee_shim
        let job = measure_job("/in/a.wav", &test_probe(8, false), |a| {
            a.format = "ac4".into()
        });
        assert_eq!(
            job.measure_args[..3],
            ["dee_shim", "measure_loudness", "--binary"]
        );
        assert!(!job.measure_args.contains(&"--temp-dir".to_string()));
    }

    #[test]
    fn measuring_ignores_the_downmix() {
        let plain = measure_job("/in/a.wav", &test_probe(6, false), |_| {});
        let downmixed = measure_job("/in/a.wav", &test_probe(6, false), |a| {
            a.downmix = Some(2);
            a.format = "thd".into();
        });
        assert_eq!(plain.ffmpeg_args, downmixed.ffmpeg_args);
        assert!(
            !downmixed.ffmpeg_args.iter().any(|a| a.contains("pan=")),
            "{:?}",
            downmixed.ffmpeg_args
        );
        assert_eq!(plain.intermediate, downmixed.intermediate);
    }

    fn encode_job(file: &str, probe: &Probe, pipe: bool, edit: impl FnOnce(&mut Args)) -> Job {
        let mut args = Args::default();
        edit(&mut args);
        test_job(file, probe, args, pipe)
    }

    fn pipe_input_format(args: &[String]) -> &str {
        let i = args.iter().position(|a| a == "--input-format").unwrap();
        &args[i + 1]
    }

    #[test]
    fn dd_ddp_ac4_and_aac_are_piped() {
        for (format, channels) in [("ddp", 6), ("dd", 6), ("ac4", 6), ("aac", 2), ("ddp", 8)] {
            let probe = test_probe(channels, false);
            let job = encode_job("/in/a.wav", &probe, true, |a| a.format = format.into());
            assert_eq!(job.pipe.as_deref(), Some(crate::pipe::path(0).as_str()));
            assert!(!job.intermediate_exists, "{format}");
            // no wav to make or to clean up
            assert!(
                job.temp_files.iter().all(|f| !f.ends_with(".wav")),
                "{format}: {:?}",
                job.temp_files
            );
            assert!(job.ffmpeg_args.ends_with(&[
                "-f".into(),
                "s24le".into(),
                crate::pipe::ffmpeg_output(0)
            ]));
            assert!(!job.ffmpeg_args.contains(&"-rf64".to_string()));
            let i = job.encode_args.iter().position(|a| a == "-i").unwrap();
            assert_eq!(job.encode_args[i + 1], crate::pipe::path(0));
            // 10 s and a second at 48 kHz
            assert_eq!(
                pipe_input_format(&job.encode_args),
                format!(
                    "pcm_s24le:sample_rate=48000:channels={channels}:streaming=1:sample_count=528000"
                )
            );
            assert!(job.measure_args.is_empty());
        }
    }

    #[test]
    fn dee_shim_layouts_take_the_pipe() {
        let probe = test_probe(8, false);
        let job = |profile| {
            let args = Args {
                format: "ddp".into(),
                ..Args::default()
            };
            test_job_with("/in/a.wav", &probe, args, true, Some(profile))
        };
        // standard 7.1 is a shim layout, dee_shim keeps the pipe path
        let standard = job(Profile71::Standard);
        assert_eq!(standard.encode_args[0], "dee_shim");
        assert!(standard.pipe.is_some());
        // Blu-ray 7.1 is native
        let bluray = job(Profile71::Bluray);
        assert_eq!(bluray.encode_args[0], "dee_x");
        assert!(bluray.pipe.is_some());
    }

    #[test]
    fn pipe_false_keeps_the_wav() {
        let job = encode_job("/in/a.wav", &test_probe(6, false), false, |_| {});
        assert!(job.pipe.is_none());
        assert!(
            job.ffmpeg_args
                .ends_with(&["-rf64".into(), "always".into(), join("/t", "a.wav")])
        );
        assert_eq!(pipe_input_format(&job.encode_args), "wav");
        assert_eq!(job.temp_files, [join("/t", "a.wav")]);
        let job = measure_job("/in/a.wav", &test_probe(6, false), |_| {});
        assert!(job.pipe.is_some());
        let mut args = Args {
            measure_only: true,
            ..Args::default()
        };
        args.format = "ddp".into();
        let job = test_job("/in/a.wav", &test_probe(6, false), args, false);
        assert!(job.pipe.is_none());
        assert_eq!(pipe_input_format(&job.measure_args), "wav");
    }

    #[test]
    fn truehd_keeps_the_wav() {
        let probe = test_probe(6, false);
        // TrueHD, also the downmixes
        for edit in [
            (|a: &mut Args| a.format = "thd".into()) as fn(&mut Args),
            |a| {
                a.format = "thd".into();
                a.downmix = Some(2);
            },
        ] {
            let job = encode_job("/in/a.wav", &probe, true, edit);
            assert!(job.pipe.is_none());
            assert_eq!(pipe_input_format(&job.encode_args), "wav");
            assert!(job.temp_files.iter().any(|f| f.ends_with(".wav")));
        }
        // DD takes the dialnorm directly, one read
        let job = encode_job("/in/a.wav", &probe, true, |a| {
            a.format = "dd".into();
            a.dialnorm = -20;
        });
        assert!(job.pipe.is_some());
    }

    #[test]
    fn ac4_with_a_dialnorm_pipes_both_runs() {
        let probe = test_probe(6, false);
        let job = encode_job("/in/a.wav", &probe, true, |a| {
            a.format = "ac4".into();
            a.dialnorm = -20;
        });
        let pipe = crate::pipe::path(0);
        assert_eq!(job.pipe.as_deref(), Some(pipe.as_str()));
        assert!(!job.intermediate_exists);
        let format = pipe_input_format(&job.measure_args).to_string();
        assert!(format.starts_with("pcm_"), "{format}");
        assert_eq!(pipe_input_format(&job.encode_args), format);
        assert!(job.measure_args.contains(&pipe));
        assert!(job.encode_args.contains(&pipe));
        assert!(job.encode_args.contains(&"--input-manifest".to_string()));
        assert!(job.temp_files.iter().all(|f| !f.ends_with(".wav")));
        assert!(job.temp_files.iter().any(|f| f.ends_with(".json")));
        // pipe = false: one wav for both runs
        let job = encode_job("/in/a.wav", &probe, false, |a| {
            a.format = "ac4".into();
            a.dialnorm = -20;
        });
        assert!(job.pipe.is_none());
        assert_eq!(pipe_input_format(&job.measure_args), "wav");
        assert_eq!(pipe_input_format(&job.encode_args), "wav");
        assert!(job.temp_files.contains(&join("/t", "a.wav")));
    }

    #[test]
    fn atmos_is_not_piped() {
        let probe = test_probe(8, true);
        let job = measure_job("/in/a.mkv", &probe, |_| {});
        assert!(job.pipe.is_none());
        let job = encode_job("/in/a.mkv", &probe, true, |a| a.format = "ac4-atmos".into());
        assert!(job.pipe.is_none());
        // the ffmpeg of truehdd copies the track
        assert!(job.ffmpeg_args.contains(&"truehd".to_string()));
    }

    #[test]
    fn measuring_is_piped_with_the_channels_of_the_wav() {
        // dee_measure_loudness up to 5.1, the DDP encoder for 7.1
        let job = measure_job("/in/a.wav", &test_probe(6, false), |_| {});
        assert_eq!(job.measure_args[0], "dee_measure_loudness");
        assert_eq!(
            pipe_input_format(&job.measure_args),
            "pcm_s24le:sample_rate=48000:channels=6:streaming=1:sample_count=528000"
        );
        let job = measure_job("/in/a.wav", &test_probe(8, false), |_| {});
        assert_eq!(job.measure_args[1], "measure_loudness");
        assert_eq!(
            pipe_input_format(&job.measure_args),
            "pcm_s24le:sample_rate=48000:channels=8:streaming=1:sample_count=528000"
        );
        // 5.0 and 7.0 are written as 5.1 and 7.1
        let job = measure_job("/in/a.wav", &test_probe(5, false), |_| {});
        assert!(pipe_input_format(&job.measure_args).contains("channels=6"));
        let job = measure_job("/in/a.wav", &test_probe(7, false), |_| {});
        assert!(pipe_input_format(&job.measure_args).contains("channels=8"));
    }

    #[test]
    fn the_pipe_rate_is_the_output_rate() {
        let mut probe = test_probe(6, false);
        probe.sample_rate = 44100;
        let job = encode_job("/in/a.wav", &probe, true, |_| {});
        assert!(pipe_input_format(&job.encode_args).starts_with("pcm_f32le:sample_rate=48000:"));
        assert!(pipe_input_format(&job.encode_args).ends_with(":sample_count=528000"));
        assert!(job.ffmpeg_args.ends_with(&[
            "-f".into(),
            "f32le".into(),
            crate::pipe::ffmpeg_output(0)
        ]));
        // without a duration there is no sample count
        probe.duration = -1.0;
        let job = encode_job("/in/a.wav", &probe, true, |_| {});
        assert_eq!(
            pipe_input_format(&job.encode_args),
            "pcm_f32le:sample_rate=48000:channels=6:streaming=1"
        );
    }

    #[test]
    fn piped_jobs_print_one_command() {
        let probe = test_probe(6, false);
        let prepared = |pipe: bool| {
            let job = encode_job("/in/a.wav", &probe, pipe, |_| {});
            let plan = audio_plan(Format::Ddp, 48000, 6, 24, 0, false, None);
            Prepared {
                jobs: vec![job],
                plans: vec![plan],
            }
        };
        let piped = prepared(true);
        let long = markup::strip(&job_commands(
            &piped.jobs[0],
            &piped.plans,
            false,
            Some(ffmpeg_pipe_print(
                &piped.plans[0],
                "/in/a.wav",
                piped.jobs[0].ffmpeg_args.last().unwrap(),
            )),
        ));
        let out = crate::pipe::ffmpeg_output(0);
        let dee_input = format!("-i {} -o ", crate::pipe::path(0));
        if cfg!(windows) {
            // DEE creates the pipe and starts first; no shell pipe
            assert!(long.starts_with("dee_x --progress 1"), "{long}");
            assert!(
                long.contains(&format!(
                    " && ffmpeg -y -drc_scale 0 -i /in/a.wav -map 0:a:0 -c pcm_s24le -f s24le {out}"
                )),
                "{long}"
            );
            assert!(long.contains("-i named_pipe://"), "{long}");
            assert!(!long.contains('|'), "{long}");
        } else {
            assert!(
                long.starts_with("ffmpeg -y -drc_scale 0 -i /in/a.wav "),
                "{long}"
            );
            assert!(long.contains(" -f s24le - | dee_x --progress 1"), "{long}");
            assert!(!long.contains("&&"), "{long}");
        }
        assert!(long.contains(&dee_input), "{long}");
        let short = markup::strip(&job_commands(&piped.jobs[0], &piped.plans, true, None));
        assert!(
            short.contains("ffmpeg -y -drc_scale 0 -i [input] "),
            "{short}"
        );
        assert!(short.contains(&format!("-f s24le {out}")), "{short}");
        assert!(
            short.contains(&format!("-i {} -o [output]", crate::pipe::path(0))),
            "{short}"
        );
        // without the pipe: the wav, then the encoder
        let wav = prepared(false);
        let long = markup::strip(&job_commands(
            &wav.jobs[0],
            &wav.plans,
            false,
            Some(ffmpeg_print(
                &wav.plans[0],
                "/in/a.wav",
                &join("/t", "a.wav"),
            )),
        ));
        assert!(
            long.contains(&format!("-rf64 always {} && dee_x", join("/t", "a.wav"))),
            "{long}"
        );
    }
}
