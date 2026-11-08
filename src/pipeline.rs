//! The encoding pipeline: checks, probing, planning, and running the jobs.

use crate::cli::Args;
use crate::commands::{
    AudioPlan, EncodeParams, INTERMEDIATE_EXISTS_PRINT, audio_plan, decode_args, encode_args,
    ffmpeg_args, ffmpeg_print, ffmpeg_print_short, intermediate_variant, measure_args, pipe_args,
    print_command,
};
use crate::config::{Config, DdpAtmosMode, Paths};
use crate::data::ErrorKind;
use crate::deewlog::LogSpec;
use crate::encode::{self, Job, Options, Outcome};
use crate::error::{error_line, escape, print_exit};
use crate::events;
use crate::inputs::{self, Input, Problem};
use crate::interrupt;
use crate::markup;
use crate::plan::{
    self, Format, InputInfo, Layout, Lfe, Modes, Profile71, Validated, check_atmos_inputs,
    check_input_layouts, check_inputs, check_measurable, check_shim_for_ddp, compute_instances,
    input_delay, notices, profile_71, resolve_layout, select_bitrate, validate_args,
};
use crate::probe::{self, Probe, ProbeError};
use crate::summary::{self, SummaryData};
use crate::tools::{self, Shim, Versions};
use crate::ui::{End, Ui};
use crate::util::{basename, delay_options, file_name};
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

fn collect_inputs(inputs: &[String]) -> Vec<Input> {
    let mut list = Vec::new();
    for input in inputs {
        if !Path::new(input).exists() {
            print_exit(ErrorKind::Path, Some(input));
        }
        list.extend(inputs::expand(input));
    }
    list
}

/// Probe all the inputs. Problems with them are collected first: errors
/// stop the run, files of folders that aren't audio are left out (after
/// asking, unless `no_prompt`). Returns the usable files with their probes.
fn probe_inputs(
    ffprobe: &str,
    track_index: i64,
    no_prompt: bool,
    inputs: Vec<Input>,
) -> (Vec<String>, Vec<Probe>) {
    let mut files = Vec::new();
    let mut probes = Vec::new();
    let mut problems = Vec::new();
    for input in inputs {
        let result = if input.bad_name {
            Err(ProbeError::BadName)
        } else {
            probe::run(ffprobe, track_index, &input.path)
        };
        match result {
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
    truehdd: Option<String>,
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
    let measure = needs_measure.then(|| dee("dee_measure_loudness"));
    let encoder = if measure_only {
        measure.clone().unwrap_or_default()
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
        truehdd: if format.is_atmos() {
            Some(tools::require_truehdd(exe_dir, &cwd))
        } else {
            tools::find_truehdd(exe_dir, &cwd)
        },
        shim,
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
    /// The 7.1 DDP profile, `None` for other outputs.
    profile: Option<Profile71>,
    versions: &'a Versions,
    /// The notices that were shown, for the report.
    notices: &'a [String],
}

fn versions(programs: &Programs) -> Versions {
    tools::probe_versions(
        Some(&programs.dee),
        programs.truehdd.as_deref(),
        programs.shim.as_ref(),
        &programs.ffmpeg,
        &programs.ffprobe,
    )
}

/// Print the summary table (if enabled).
fn print_summary(ctx: &Context, files: &[String], instances: f64, versions: &Versions) {
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
    let data = SummaryData {
        sections,
        latest,
        versions: versions.clone(),
        channels: ctx.info.channels,
        sample_rates: &[ctx.info.samplerate],
        bit_depths: &ctx.info.bit_depths,
        measure_only: ctx.args.measure_only,
        format: ctx.validated.format,
        output_channels: ctx.layout.output_name(),
        profile: ctx.profile,
        bitrate: ctx.bitrate,
        dialnorm: ctx.validated.dialnorm,
        output_dir: if ctx.to_input {
            "next to the input files"
        } else {
            ctx.output
        },
        files: files.len(),
        instances,
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
/// has another name, so a kept one (`-k`) is never reused by mistake.
fn temp_names(temp: &str, file: &str, variant: &str) -> TempNames {
    let name = |ext: &str, sanitize: bool| join(temp, &basename(file, ext, false, sanitize, false));
    let wav = if variant.is_empty() {
        "wav".to_string()
    } else {
        format!("{variant}.wav")
    };
    TempNames {
        wav: name(&wav, false),
        atmos: name("atmos", true),
        loudness: name("loudness.json", false),
        ac4_manifest: name("ac4.json", false),
    }
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

    /// Whether the output is the Blu-ray flavour (`.eb3`).
    fn bluray(&self) -> bool {
        self.profile == Some(Profile71::Bluray)
            || (self.validated.format == Format::DdpAtmos
                && self.validated.atmos_mode == DdpAtmosMode::Bluray)
    }
}

/// What the report of `-kl` shows besides the run itself.
fn log_spec(ctx: &Context, file: &str, probe: &Probe, bit_depth: &u32) -> LogSpec {
    let row = |k: &str, v: String| (k.to_string(), v);
    let format = ctx.validated.format;
    let v = ctx.versions;
    let versions = vec![
        row("deew", env!("CARGO_PKG_VERSION").to_string()),
        row("DEE", v.dee.text()),
        row("truehdd", v.truehdd.text()),
        row("dee_shim", v.shim.text()),
        row("ffmpeg", v.ffmpeg.text()),
        row("ffprobe", v.ffprobe.text()),
    ];
    let duration = if probe.duration > 0.0 {
        format!("{:.3} s", probe.duration)
    } else {
        "unknown".to_string()
    };
    let input = vec![
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
    ];
    let shim_used = ctx.programs.shim.is_some()
        && (ctx.profile == Some(Profile71::Standard)
            || (format == Format::Ddp
                && ctx.layout.lfe == Lfe::Removed
                && ctx.layout.outchannels == 8)
            || matches!(format, Format::DdpAtmos | Format::ThdAtmos)
            || (format == Format::Thd && ctx.validated.drc == "none"));
    let settings = vec![
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
        row("Measure only", ctx.args.measure_only.to_string()),
        row("Temp path", ctx.temp.to_string()),
        row(
            "dee_shim used",
            if shim_used { "yes" } else { "no" }.to_string(),
        ),
    ];
    LogSpec {
        path: join(
            &ctx.output_dir(file),
            &basename(file, "deew-log.html", false, false, true),
        ),
        versions,
        input,
        settings,
        notices: ctx.notices.to_vec(),
    }
}

/// DEE's own temp directory of job `index`: unique per deew process and job.
fn dee_temp_dir(temp: &str, pid: u32, index: usize) -> String {
    join(temp, &format!("dee-{pid}-{index}"))
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
    // `-mo` measures a wav as made for DDP.
    let plan_format = if measure_only { Format::Ddp } else { format };
    let downmix = if measure_only {
        None
    } else {
        ctx.validated.downmix
    };
    let silent_lfe = ctx.layout.lfe == Lfe::Silent;
    let variant = intermediate_variant(
        plan_format,
        ctx.info.samplerate,
        ctx.info.channels,
        silent_lfe,
        downmix,
    );
    let names = temp_names(ctx.temp, file, &variant);
    let atmos = format.is_atmos();
    let intermediate = if atmos { &names.atmos } else { &names.wav };
    let intermediate_exists = Path::new(intermediate).exists();

    let plan = (!atmos).then(|| {
        audio_plan(
            plan_format,
            ctx.info.samplerate,
            ctx.info.channels,
            bit_depth,
            ctx.validated.track_index,
            ctx.layout.lfe == Lfe::Silent,
            // the TrueHD encoder can't downmix, ffmpeg does
            if measure_only {
                None
            } else {
                ctx.validated.downmix
            },
        )
    });
    // truehdd reads raw streams only: a raw .thd goes to it directly, a track
    // of a container is copied into its stdin by ffmpeg.
    let piped = atmos && !probe.raw_truehd;
    let ffmpeg_args = if piped {
        pipe_args(&ctx.programs.ffmpeg, file, ctx.validated.track_index)
    } else {
        plan.as_ref()
            .map(|plan| ffmpeg_args(&ctx.programs.ffmpeg, plan, file, &names.wav))
            .unwrap_or_default()
    };
    let decode_args = if atmos {
        let prefix = names.atmos.strip_suffix(".atmos").unwrap_or(&names.atmos);
        let truehdd = ctx.programs.truehdd.as_deref().unwrap_or_default();
        decode_args(truehdd, if piped { "-" } else { file }, prefix)
    } else {
        Vec::new()
    };

    let output_dir = ctx.output_dir(file);
    let output_path = join(
        &output_dir,
        &basename(
            file,
            format.extension(ctx.layout.outchannels, ctx.bluray()),
            false,
            false,
            true,
        ),
    );
    let measuring = measure_only || dialnorm != 0;
    let measure_args = match (&ctx.programs.measure, measuring) {
        (Some(tool), true) => measure_args(tool, &names.wav, &names.loudness),
        _ => Vec::new(),
    };
    let encode_args = if measure_only {
        Vec::new()
    } else {
        let delay = input_delay(file, ctx.args.delay.as_deref());
        let delay = delay_options(&delay, format.encoder_delay())
            .unwrap_or_else(|()| print_exit(ErrorKind::Delay, None));
        encode_args(&EncodeParams {
            format,
            tool: &ctx.programs.dee,
            input: intermediate,
            output: &output_path,
            bitrate: ctx.bitrate.unwrap_or(0),
            drc: &ctx.validated.drc,
            outchannels: ctx.layout.outchannels,
            layout_arg: ctx.layout.layout_arg,
            input_manifest: (dialnorm != 0).then_some(names.loudness.as_str()),
            output_manifest: &names.ac4_manifest,
            delay: &delay,
            temp: &dee_temp,
            shim: ctx.programs.shim.as_ref().filter(|_| {
                matches!(
                    format,
                    Format::Ddp | Format::DdpAtmos | Format::Thd | Format::ThdAtmos
                )
            }),
            bluray: ctx.profile != Some(Profile71::Standard),
            atmos_mode: ctx.validated.atmos_mode,
            lfe_removed: ctx.layout.lfe == Lfe::Removed,
        })
    };

    let mut temp_files = if atmos {
        atmos_files(&names.atmos)
    } else {
        vec![names.wav.clone()]
    };
    if !measure_args.is_empty() {
        temp_files.push(names.loudness.clone());
    }
    if matches!(format, Format::Ac4 | Format::Ac4Atmos) && !encode_args.is_empty() {
        temp_files.push(names.ac4_manifest.clone());
    }
    let log = ctx
        .args
        .keep_log
        .then(|| log_spec(ctx, file, probe, &bit_depth));
    let job = Job {
        input: file.to_string(),
        length: probe.duration,
        format,
        ffmpeg_args,
        decode_args,
        measure_args,
        encode_args,
        intermediate_exists,
        intermediate: intermediate.clone(),
        loudness_path: names.loudness,
        dialnorm,
        output_path,
        output_dir,
        temp_files,
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
    let mut parts = Vec::new();
    if job.intermediate_exists && !short {
        parts.push(INTERMEDIATE_EXISTS_PRINT.to_string());
    } else if job.format.is_atmos() {
        let decode = print_command(&job.decode_args, short);
        if job.ffmpeg_args.is_empty() {
            parts.push(decode);
        } else {
            parts.push(format!(
                "{} | {decode}",
                print_command(&job.ffmpeg_args, short)
            ));
        }
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
            let ffmpeg = if job.format.is_atmos() {
                None
            } else {
                plans
                    .next()
                    .map(|plan| ffmpeg_print(plan, &job.input, &job.intermediate))
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
    let format = validated.format;
    let needs_measure = args.measure_only || validated.dialnorm != 0;
    let programs = find_programs(
        binaries,
        &paths.exe_dir,
        format,
        &validated.drc,
        needs_measure,
        args.measure_only,
    );

    let inputs = collect_inputs(args.input.as_deref().unwrap_or_default());
    if inputs.is_empty() {
        error_line("no input files found.");
    }
    let (files, probes) = probe_inputs(
        &programs.ffprobe,
        validated.track_index,
        args.no_prompt,
        inputs,
    );
    let info = check_inputs(&probes).unwrap_or_else(|fail| fail.exit());
    if format.is_atmos() {
        check_atmos_inputs(&files, &probes).unwrap_or_else(|fail| fail.exit());
    }
    check_input_layouts(&files, &probes).unwrap_or_else(|fail| fail.exit());
    let layout = resolve_layout(format, validated.downmix, info.channels, args.disable_lfe)
        .unwrap_or_else(|fail| fail.exit());
    check_shim_for_ddp(
        format,
        &layout,
        validated.ddp_mode,
        args.measure_only,
        programs.shim.is_some(),
    )
    .unwrap_or_else(|fail| fail.exit());
    if !args.measure_only {
        for note in &layout.notes {
            events::warning(&markup::strip(note));
            rprintln!("{}", note);
        }
    }
    check_measurable(info.channels, needs_measure).unwrap_or_else(|fail| fail.exit());

    let profile = if args.measure_only {
        None
    } else {
        profile_71(format, layout.outchannels, validated.ddp_mode)
    };
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
        profile,
        versions: &versions,
        notices: &notice_texts,
    };

    print_summary(&ctx, &files, instances, &versions);
    let prepared = prepare_jobs(&ctx, &files, &probes);
    print_commands(&prepared, args.long_argument);

    let options = Options {
        keep_temp: args.keeptemp,
        keep_log: args.keep_log,
    };
    let results = run_jobs(&prepared.jobs, &options, instances);
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
        let names = temp_names("/t", "/in/My Song DELAY -5ms.flac", "");
        assert_eq!(names.wav, "/t/My Song DELAY -5ms.wav");
        assert_eq!(names.atmos, "/t/My_Song_DELAY_-5ms.atmos");
        assert_eq!(names.loudness, "/t/My Song DELAY -5ms.loudness.json");
        assert_eq!(names.ac4_manifest, "/t/My Song DELAY -5ms.ac4.json");
        let names = temp_names("/t", "/in/a [1].flac", "thd.dm6.dl");
        assert_eq!(names.wav, "/t/a [1].thd.dm6.dl.wav");
        assert_eq!(names.atmos, "/t/a_[1].atmos");
    }

    #[test]
    fn dee_temp_dir_is_per_process_and_job() {
        assert_eq!(dee_temp_dir("/t", 42, 3), "/t/dee-42-3");
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
}
