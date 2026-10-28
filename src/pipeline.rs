//! The encoding pipeline: checks, probing, planning, and running the jobs.

use crate::cli::Args;
use crate::commands::{
    AudioPlan, INTERMEDIATE_EXISTS_PRINT, audio_plan, dee_args, dee_print, dee_print_short,
    ffmpeg_args, ffmpeg_print, ffmpeg_print_short,
};
use crate::config::{Config, Paths};
use crate::data::ErrorKind;
use crate::encode::{self, Job, Options};
use crate::error::{error_line, escape, print_exit};
use crate::jobxml::{XmlParams, build_xml};
use crate::plan::{
    self, InputInfo, Layout, Validated, check_inputs, compute_instances, input_delay, notices,
    resolve_layout, select_bitrate, validate_args,
};
use crate::probe::{self, Probe};
use crate::summary::{self, SummaryData};
use crate::tools::{self, DeeKind};
use crate::ui::Ui;
use crate::util::{basename, file_name, wpc};
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

/// The job XML directory for a DEE that runs natively: the temp directory
/// with a `/` appended, also on Windows (so `C:\\temp` becomes `C:\\temp/`).
fn native_xml_dir(temp: &str) -> String {
    if temp.ends_with('/') {
        temp.to_string()
    } else {
        format!("{temp}/")
    }
}

/// Regular, non-hidden files of a directory (like `glob(dir/*)`), by name.
fn list_directory(dir: &str) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|entry| entry.path().is_file())
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| !name.starts_with('.'))
        .collect();
    names.sort();
    names
        .into_iter()
        .map(|name| format!("{dir}{}{name}", std::path::MAIN_SEPARATOR))
        .collect()
}

fn collect_inputs(inputs: &[String]) -> Vec<String> {
    let mut list = Vec::new();
    for input in inputs {
        let path = Path::new(input);
        if !path.exists() {
            print_exit(ErrorKind::Path, Some(input));
        }
        if path.is_dir() {
            list.extend(list_directory(input));
        } else {
            list.push(input.clone());
        }
    }
    list
}

fn probe_inputs(ffprobe: &str, track_index: i64, files: &[String]) -> Vec<Probe> {
    files
        .iter()
        .map(|file| {
            probe::run(ffprobe, track_index, file)
                .unwrap_or_else(|()| print_exit(ErrorKind::Ffprobe, None))
        })
        .collect()
}

fn show_notices(list: &[plan::Notice], no_prompt: bool) {
    for notice in list {
        if no_prompt {
            rprintln!("{}", notice.note);
        } else if !prompt::confirm(notice.question) {
            std::process::exit(1);
        }
    }
}

/// The output directory: `-o` (created) or the current directory.
fn output_directory(arg: Option<&str>) -> String {
    let cwd = std::env::current_dir().unwrap_or_default();
    match arg.filter(|o| !o.is_empty()) {
        Some(out) => {
            let out = tools::abspath(out, &cwd);
            create_dir(&out);
            out
        }
        None => cwd.to_string_lossy().into_owned(),
    }
}

fn dee_path_for_xml(path: &str, nonnative: bool) -> String {
    wpc(path, true, nonnative).unwrap_or_else(|()| print_exit(ErrorKind::WslPath, Some(path)))
}

struct Context<'a> {
    args: &'a Args,
    config: &'a Config,
    dee: DeeKind,
    validated: &'a Validated,
    info: InputInfo,
    layout: &'a Layout,
    bitrate: Option<u32>,
    temp: &'a str,
    output: &'a str,
    /// Quoted `output` and `temp` in the notation of DEE.
    output_xml: &'a str,
    temp_xml: &'a str,
}

/// Print the summary table (if enabled).
fn print_summary(ctx: &Context, files: &[String], instances: f64, versions: &[Option<String>; 3]) {
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
        dee_version: versions[0].as_deref(),
        ffmpeg_version: versions[1].as_deref(),
        ffprobe_version: versions[2].as_deref(),
        channels: ctx.info.channels,
        bit_depths: &ctx.info.bit_depths,
        measure_only: ctx.args.measure_only,
        format: ctx.validated.format,
        outchannels: ctx.layout.outchannels,
        bitrate: ctx.bitrate,
        dialnorm: ctx.validated.dialnorm,
        output_dir: ctx.output,
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

/// The job XMLs are written and the jobs built.
struct Prepared {
    jobs: Vec<Job>,
    ffmpeg_prints: Vec<String>,
    dee_prints: Vec<String>,
    intermediate_names: Vec<String>,
    /// The plan of every input.
    plans: Vec<AudioPlan>,
}

fn prepare_jobs(ctx: &Context, files: &[String], probes: &[Probe]) -> Prepared {
    let format = ctx.validated.format;
    let nonnative = ctx.dee.nonnative;
    let params = XmlParams {
        format,
        outchannels: ctx.layout.outchannels,
        bitrate: ctx.bitrate.unwrap_or(0),
        downmix_config: ctx.layout.downmix_config,
        drc: &ctx.args.drc,
        dialnorm: ctx.validated.dialnorm,
        force_standard: ctx.args.force_standard,
        force_bluray: ctx.args.force_bluray,
        output_path: ctx.output_xml,
        temp_path: ctx.temp_xml,
    };

    // The directory of the job XMLs as DEE sees it.
    let xml_dir = if nonnative {
        wpc(ctx.temp, false, true)
            .unwrap_or_else(|()| print_exit(ErrorKind::WslPath, Some(ctx.temp)))
    } else {
        native_xml_dir(ctx.temp)
    };

    let mut prepared = Prepared {
        jobs: Vec::new(),
        ffmpeg_prints: Vec::new(),
        dee_prints: Vec::new(),
        intermediate_names: Vec::new(),
        plans: Vec::new(),
    };
    for ((file, probe), &bit_depth) in files.iter().zip(probes).zip(&ctx.info.bit_depths) {
        let plan = audio_plan(
            format,
            ctx.info.samplerate,
            ctx.info.channels,
            bit_depth,
            ctx.validated.track_index,
        );
        let xml_name = basename(file, "xml", false, true, false);
        let dee_xml_input = format!("{xml_dir}{xml_name}");
        let wav_path = join(ctx.temp, &basename(file, "wav", false, false, false));

        let ffmpeg_args = ffmpeg_args(&ctx.config.ffmpeg_path, &plan, file, &wav_path);
        let dee_args = dee_args(&ctx.config.dee_path, &dee_xml_input, ctx.dee.is_exe);

        let intermediate_exists = Path::new(&wav_path).exists();
        let ffmpeg_print = if intermediate_exists {
            prepared
                .intermediate_names
                .push(file_name(file).to_string());
            INTERMEDIATE_EXISTS_PRINT.to_string()
        } else {
            ffmpeg_print(&plan, file, &wav_path)
        };
        prepared.ffmpeg_prints.push(ffmpeg_print);
        prepared
            .dee_prints
            .push(dee_print(&dee_xml_input, ctx.dee.is_exe));

        let delay = input_delay(file, ctx.args.delay.as_deref());
        let xml = build_xml(&params, file, &delay)
            .unwrap_or_else(|()| print_exit(ErrorKind::Delay, None));
        let xml_path = join(ctx.temp, &xml_name);
        if let Err(e) = std::fs::write(&xml_path, xml) {
            error_line(&escape(&format!("failed to write {xml_path}: {e}")));
        }

        prepared.plans.push(plan);
        prepared.jobs.push(Job {
            input: file.clone(),
            length: probe.duration,
            ffmpeg_args,
            dee_args,
            intermediate_exists,
            format,
            wav_path,
            xml_path,
            output_dir: ctx.output.to_string(),
        });
    }
    prepared
}

fn print_commands(prepared: &Prepared, long: bool, dee_is_exe: bool) {
    if long {
        rprintln!("[bold color(231)]Running the following commands:[/bold color(231)]");
        for (ffmpeg, dee) in prepared.ffmpeg_prints.iter().zip(&prepared.dee_prints) {
            rprintln!("{} && {}", ffmpeg, dee);
        }
    } else {
        if !prepared.intermediate_names.is_empty() {
            let names: Vec<String> = prepared
                .intermediate_names
                .iter()
                .map(|n| escape(n))
                .collect();
            rprintln!(
                "[bold color(231)]Intermediate already exists for the following file(s):[/bold color(231)] [bold magenta]{}[/bold magenta]",
                names.join("[not bold white],[/not bold white] ")
            );
        }
        rprintln!(
            "[bold color(231)]Running the following commands:[/bold color(231)]\n{} && {}",
            ffmpeg_print_short(&prepared.plans),
            dee_print_short(dee_is_exe)
        );
    }
    println!();
}

/// Run the jobs on `instances` workers; returns the error messages.
fn run_jobs(jobs: &[Job], options: &Options, instances: f64) -> Vec<String> {
    let ui = Ui::new();
    let workers = (instances.ceil() as usize).clamp(1, jobs.len().max(1));
    let next = AtomicUsize::new(0);
    let errors = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::SeqCst);
                    let Some(job) = jobs.get(index) else { break };
                    let bar = ui.add_bar();
                    if let Err(message) = encode::run_job(job, options, &bar, &ui) {
                        errors.lock().expect("errors").push(message);
                    }
                    ui.finish(index, bar);
                }
            });
        }
    });
    ui.flush();
    errors.into_inner().expect("errors")
}

pub fn run(args: Args, config: Config, paths: Paths) {
    let dee = tools::check_binaries(&config.dee_path, &config.ffmpeg_path, &config.ffprobe_path);

    let temp = tools::temp_path(&config.temp_path, dee.nonnative, &paths.exe_dir);
    create_dir(&temp);

    let cpu = std::thread::available_parallelism().map_or(1, usize::from);
    let instances = compute_instances(
        args.instances.as_deref(),
        &config.max_instances,
        cpu,
        dee.is_exe,
    )
    .unwrap_or_else(|message| error_line(&escape(&message)));

    let validated = validate_args(&args, dee.is_exe, cfg!(target_os = "linux"))
        .unwrap_or_else(|fail| fail.exit());

    let files = collect_inputs(args.input.as_deref().unwrap_or_default());
    if files.is_empty() {
        error_line("no input files found.");
    }
    let probes = probe_inputs(&config.ffprobe_path, validated.track_index, &files);
    let info = check_inputs(&probes).unwrap_or_else(|fail| fail.exit());
    let layout = resolve_layout(validated.format, validated.downmix, info.channels)
        .unwrap_or_else(|fail| fail.exit());

    show_notices(
        &notices(
            validated.format,
            layout.outchannels,
            args.measure_only,
            validated.dialnorm,
        ),
        args.no_prompt,
    );

    let bitrate = select_bitrate(
        validated.format,
        layout.outchannels,
        args.bitrate,
        &config.default_bitrates,
        args.force_standard,
        args.force_bluray,
    );
    let output = output_directory(args.output.as_deref());

    // The Python version converts these before anything is printed.
    let output_xml = dee_path_for_xml(&output, dee.nonnative);
    let temp_xml = dee_path_for_xml(&temp, dee.nonnative);

    let ctx = Context {
        args: &args,
        config: &config,
        dee,
        validated: &validated,
        info,
        layout: &layout,
        bitrate,
        temp: &temp,
        output: &output,
        output_xml: &output_xml,
        temp_xml: &temp_xml,
    };

    let versions = [
        tools::version_of(&config.dee_path, &[]),
        tools::version_of(&config.ffmpeg_path, &["-version"]),
        tools::version_of(&config.ffprobe_path, &["-version"]),
    ];
    print_summary(&ctx, &files, instances, &versions);
    let prepared = prepare_jobs(&ctx, &files, &probes);
    print_commands(&prepared, args.long_argument, dee.is_exe);

    let options = Options {
        dialnorm_set: validated.dialnorm != 0,
        measure_only: args.measure_only,
        keep_temp: args.keeptemp,
        dee_reports_stages: encode::reports_stages(versions[0].as_deref()),
    };
    let errors = run_jobs(&prepared.jobs, &options, instances);
    if !errors.is_empty() {
        for message in &errors {
            rprintln!(
                "[color(231) on red]ERROR:[/color(231) on red] {}",
                escape(message)
            );
        }
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xml_dir_gets_a_forward_slash() {
        assert_eq!(native_xml_dir("/var/tmp/deew"), "/var/tmp/deew/");
        assert_eq!(native_xml_dir("/var/tmp/deew/"), "/var/tmp/deew/");
        // Python appends '/' on Windows too, and only checks for '/'.
        assert_eq!(native_xml_dir("C:\\deew\\temp"), "C:\\deew\\temp/");
        assert_eq!(native_xml_dir("C:\\"), "C:\\/");
    }

    #[cfg(windows)]
    #[test]
    fn xml_input_path_uses_the_basename_of_either_separator() {
        let xml_dir = native_xml_dir("C:\\deew\\temp");
        for file in ["D:\\in\\My File.wav", "D:/in/My File.wav"] {
            let name = basename(file, "xml", false, true, false);
            assert_eq!(format!("{xml_dir}{name}"), "C:\\deew\\temp/My_File.xml");
        }
    }
}
