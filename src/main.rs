mod cli;
mod commands;
mod config;
mod data;
mod encode;
mod error;
mod events;
mod help;
mod info;
mod jobxml;
mod markup;
mod pipeline;
mod plan;
mod probe;
mod prompt;
mod summary;
mod term;
mod toml_highlight;
mod tools;
mod ui;
mod util;
mod xml;

/// Exit quietly (like a process killed by SIGPIPE) when stdout is closed,
/// e.g. `deew -cl | head`, instead of printing a panic message.
fn exit_quietly_on_broken_pipe() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        if info.to_string().contains("Broken pipe") {
            std::process::exit(141);
        }
        default_hook(info);
    }));
}

fn main() {
    exit_quietly_on_broken_pipe();
    events::init();
    let argv: Vec<String> = std::env::args().skip(1).collect();

    if argv.is_empty() {
        cli::print_help();
        std::process::exit(1);
    }

    let args = cli::parse(&argv);

    if args.changelog {
        info::print_changelog();
    }
    if args.list_bitrates {
        info::list_bitrates();
    }
    if args.print_logos {
        info::print_logos();
    }

    let paths = config::Paths::resolve();

    if args.config {
        config::show_config(&paths);
    }
    if args.generate_config {
        config::generate_config(&paths, 0);
    }

    let config = config::load(&paths);

    if args.input.as_ref().is_none_or(|input| input.is_empty()) {
        cli::usage_error("the following arguments are required: -i/--input");
    }

    pipeline::run(args, config, paths);
}
