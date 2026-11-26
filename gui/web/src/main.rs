use deew_gui_core::cli;
use deew_web::{AppState, auth, config, router};
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

mod cmdline;

use cmdline::{Command, Options, esc};

fn warning(text: &str) {
    cmdline::err(&format!(
        "[black on yellow]WARNING:[/black on yellow] {}",
        esc(text)
    ));
}

fn hash_password_command() -> Result<(), String> {
    let color = cmdline::color_enabled();
    let first =
        rpassword::prompt_password(cmdline::render("[bold cyan]Password:[/bold cyan] ", color))
            .map_err(|e| format!("cannot read the password: {e}"))?;
    if first.is_empty() {
        return Err("the password must not be empty".into());
    }
    let second = rpassword::prompt_password(cmdline::render(
        "[bold cyan]Repeat password:[/bold cyan] ",
        color,
    ))
    .map_err(|e| format!("cannot read the password: {e}"))?;
    if first != second {
        return Err("the passwords do not match".into());
    }
    let hash = auth::hash_password(&first)?;
    println!("{}", cmdline::hash_output(&hash, color));
    Ok(())
}

fn config_location() -> Option<PathBuf> {
    cli::config_path().or_else(|| {
        // Without a CLI the config next to this program still counts, so that
        // a configured login is never silently skipped.
        let exe = std::env::current_exe().ok()?;
        Some(exe.parent()?.join("config.toml"))
    })
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        match signal(SignalKind::terminate()) {
            Ok(mut term) => {
                tokio::select! {
                    _ = tokio::signal::ctrl_c() => {}
                    _ = term.recv() => {}
                }
            }
            Err(_) => {
                let _ = tokio::signal::ctrl_c().await;
            }
        }
    }
    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

async fn run(args: Options) -> Result<(), String> {
    let config_path = config_location();
    let credentials = config::load_credentials(config_path.as_deref())?;
    let cli_path = cli::sidecar_path();

    let listener = tokio::net::TcpListener::bind((args.host.as_str(), args.port))
        .await
        .map_err(|e| format!("cannot listen on {}:{}: {e}", args.host, args.port))?;
    let addr = listener
        .local_addr()
        .map_err(|e| format!("cannot get the listening address: {e}"))?;
    let loopback = addr.ip().is_loopback();
    let url_host = match addr.ip() {
        IpAddr::V6(ip) => format!("[{ip}]"),
        IpAddr::V4(ip) => ip.to_string(),
    };

    let path_markup = |p: &std::path::Path| {
        format!(
            "[bold yellow]{}[/bold yellow]",
            esc(&p.display().to_string())
        )
    };
    cmdline::out(&format!(
        "[bold cyan]deew-web[/bold cyan] listening on [bold yellow]{}[/bold yellow]",
        esc(&format!("http://{url_host}:{}", addr.port()))
    ));
    match &cli_path {
        Ok(path) => cmdline::out(&format!(
            "[bold cyan]deew CLI:[/bold cyan] {}",
            path_markup(path)
        )),
        Err(_) => warning(
            "deew CLI not found next to deew-web (Utilities -> Update and install all downloads it)",
        ),
    }
    match &config_path {
        Some(path) => cmdline::out(&format!(
            "[bold cyan]config:[/bold cyan] {}{}",
            path_markup(path),
            if path.is_file() {
                ""
            } else {
                " [red](not found)[/red]"
            }
        )),
        None => cmdline::out("[bold cyan]config:[/bold cyan] none"),
    }
    match &credentials {
        Some(c) => cmdline::out(&format!(
            "[bold cyan]login required:[/bold cyan] [green]yes[/green] (user '{}')",
            esc(&c.user)
        )),
        None => cmdline::out("[bold cyan]login required:[/bold cyan] [yellow]no[/yellow]"),
    }
    if !loopback {
        if credentials.is_none() {
            warning(
                "no login is configured and deew-web is reachable from the network: anyone who can connect can run deew and browse your files",
            );
        } else {
            cmdline::err(
                "[color(231) on blue]NOTE:[/color(231) on blue] plain HTTP sends the password unencrypted; put deew-web behind a TLS reverse proxy before exposing it to the internet",
            );
        }
    }

    let state = Arc::new(AppState::new(credentials, loopback, None));
    let app = router(Arc::clone(&state));
    let server = axum::serve(listener, app);
    tokio::select! {
        result = server => result.map_err(|e| format!("server error: {e}"))?,
        _ = shutdown_signal() => {
            if state.job.is_running() {
                cmdline::out("[yellow]stopping the running deew job...[/yellow]");
                state.job.stop();
                let deadline = Instant::now() + Duration::from_secs(10);
                while state.job.is_running() && Instant::now() < deadline {
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
            }
        }
    }
    Ok(())
}

// `deew -u` reads this string out of the binary to learn the installed
// version, so it must survive LTO and section GC (`#[used]` plus the
// `black_box` in `main`).
#[used]
static VERSION_MARK: &str = concat!("\0deew-web-version=", env!("CARGO_PKG_VERSION"), "\0");

fn main() {
    std::hint::black_box(VERSION_MARK);
    let args = match cmdline::parse(std::env::args().skip(1)) {
        Ok(Command::Help) => {
            cmdline::print_help();
            return;
        }
        Ok(Command::Version) => {
            cmdline::print_version();
            return;
        }
        Ok(Command::Run(options)) => options,
        Err(message) => cmdline::usage_error(&message),
    };
    let result = if args.hash_password {
        hash_password_command()
    } else {
        match tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime.block_on(run(args)),
            Err(e) => Err(format!("cannot start the async runtime: {e}")),
        }
    };
    if let Err(message) = result {
        cmdline::error(&message);
        std::process::exit(1);
    }
}
