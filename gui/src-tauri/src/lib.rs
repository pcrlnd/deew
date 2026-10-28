use base64::Engine;
use base64::engine::general_purpose::STANDARD as BASE64;
use deew_gui_core::cli;
use deew_gui_core::probe::ProbeResult;
use deew_gui_core::pty_session::{self, PtySession, SpawnConfig};
use serde::Serialize;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use tauri::ipc::Channel;
use tauri::webview::PageLoadEvent;
use tauri::{AppHandle, Manager, State, WindowEvent};
use tauri_plugin_log::{Target, TargetKind};
use tauri_plugin_opener::OpenerExt;

/// Messages streamed to the frontend while deew runs.
#[derive(Clone, Serialize)]
#[serde(tag = "event", content = "data", rename_all = "camelCase")]
enum RunEvent {
    /// Raw terminal output, base64 encoded.
    Output(String),
    /// The process ended. `code` is its exit code.
    Exit { code: Option<i32> },
}

struct ActiveSession {
    id: u64,
    session: PtySession,
}

/// The one running deew process, if any.
#[derive(Default)]
struct Runner(Mutex<Option<ActiveSession>>);

static NEXT_SESSION_ID: AtomicU64 = AtomicU64::new(1);

fn lock(runner: &Runner) -> Result<std::sync::MutexGuard<'_, Option<ActiveSession>>, String> {
    runner
        .0
        .lock()
        .map_err(|_| "internal error: state poisoned".to_string())
}

/// Starts deew inside a pseudo-terminal and streams its output to
/// `on_output`. Returns as soon as the process is started; the end of the
/// process is reported as [`RunEvent::Exit`].
#[tauri::command]
async fn run_deew(
    app: AppHandle,
    runner: State<'_, Runner>,
    args: Vec<String>,
    cols: u16,
    rows: u16,
    on_output: Channel<RunEvent>,
) -> Result<(), String> {
    let mut active = lock(&runner)?;
    if active.is_some() {
        return Err("deew is already running".into());
    }
    let id = NEXT_SESSION_ID.fetch_add(1, Ordering::SeqCst);

    let config = SpawnConfig {
        program: cli::sidecar_path()?,
        args,
        cols,
        rows,
        // deew writes to the current directory by default; a GUI app has no
        // meaningful one (`/` on macOS, `/usr/bin` for a deb), so use $HOME.
        cwd: app.path().home_dir().ok(),
        env: vec![("COLORTERM".into(), "truecolor".into())],
    };

    let output = on_output.clone();
    let exit_app = app.clone();
    let session = pty_session::spawn(
        config,
        move |bytes| {
            let _ = output.send(RunEvent::Output(BASE64.encode(bytes)));
        },
        move |code| {
            // Free the slot before announcing the end, so that the frontend
            // can start the next run right away.
            if let Ok(mut active) = lock(&exit_app.state::<Runner>())
                && active.as_ref().is_some_and(|a| a.id == id)
            {
                *active = None;
            }
            let _ = on_output.send(RunEvent::Exit { code: Some(code) });
        },
    )?;
    *active = Some(ActiveSession { id, session });
    Ok(())
}

/// Sends keystrokes typed into the terminal to the running process.
#[tauri::command]
async fn write_stdin(runner: State<'_, Runner>, data: String) -> Result<(), String> {
    match lock(&runner)?.as_mut() {
        Some(active) => active.session.write(data.as_bytes()),
        None => Ok(()),
    }
}

/// Tells the running process the new size of the terminal.
#[tauri::command]
async fn resize(runner: State<'_, Runner>, cols: u16, rows: u16) -> Result<(), String> {
    match lock(&runner)?.as_ref() {
        Some(active) => active.session.resize(cols, rows),
        None => Ok(()),
    }
}

/// Stops the running process (Ctrl+C, then kill after a grace period).
#[tauri::command]
async fn stop(runner: State<'_, Runner>) -> Result<(), String> {
    if let Some(active) = lock(&runner)?.as_mut() {
        active.session.stop();
    }
    Ok(())
}

/// The `[default_bitrates]` of the config next to the CLI (key -> kbps).
/// Empty when there is no CLI, config or table: nothing the UI has to show.
#[tauri::command]
async fn default_bitrates() -> std::collections::BTreeMap<String, i64> {
    tauri::async_runtime::spawn_blocking(cli::default_bitrates)
        .await
        .unwrap_or_default()
}

/// Probes the inputs (directories are expanded like the CLI does) with the
/// ffprobe that the deew config points to. Runs on a blocking thread so the
/// UI never waits for it.
#[tauri::command]
async fn probe_inputs(inputs: Vec<String>, track_index: u32) -> Result<ProbeResult, String> {
    tauri::async_runtime::spawn_blocking(move || cli::probe_inputs(&inputs, track_index))
        .await
        .map_err(|e| format!("probing failed: {e}"))?
}

/// The version of the deew CLI that the GUI runs (`deew -v`).
#[tauri::command]
async fn cli_version() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(cli::cli_version)
        .await
        .map_err(|e| format!("version check failed: {e}"))?
}

fn external_navigation_plugin<R: tauri::Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::<R>::new("external-navigation")
        .on_navigation(|webview, url| {
            let is_internal_host = matches!(
                url.host_str(),
                Some("localhost") | Some("127.0.0.1") | Some("tauri.localhost") | Some("::1")
            );

            let is_internal = url.scheme() == "tauri" || is_internal_host;

            if is_internal {
                return true;
            }

            let is_external_link = matches!(url.scheme(), "http" | "https" | "mailto" | "tel");

            if is_external_link {
                log::info!("opening external link in system browser: {}", url);
                let _ = webview.opener().open_url(url.as_str(), None::<&str>);
                return false;
            }

            true
        })
        .build()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(
            tauri_plugin_log::Builder::new()
                .targets([
                    Target::new(TargetKind::Stdout),
                    Target::new(TargetKind::LogDir { file_name: None }),
                ])
                .build(),
        )
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(external_navigation_plugin())
        .manage(Runner::default())
        .invoke_handler(tauri::generate_handler![
            run_deew,
            write_stdin,
            resize,
            stop,
            probe_inputs,
            default_bitrates,
            cli_version
        ])
        .setup(|app| {
            match app.get_webview_window("main") {
                Some(window) => {
                    let title = format!("deew-gui {}", app.package_info().version);
                    if let Err(e) = window.set_title(&title) {
                        log::warn!("could not set the window title: {e}");
                    }
                }
                None => log::warn!("main window not found, keeping the default title"),
            }
            Ok(())
        })
        .on_page_load(|webview, payload| {
            if webview.label() == "main" && matches!(payload.event(), PageLoadEvent::Finished) {
                log::info!("main webview finished loading");
                let _ = webview.window().show();
            }
        })
        .on_window_event(|window, event| {
            // Do not leave an encode running behind a closed window.
            if matches!(event, WindowEvent::Destroyed)
                && let Ok(mut active) = lock(&window.state::<Runner>())
            {
                active.take();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_events_match_the_frontend_protocol() {
        let output = serde_json::to_value(RunEvent::Output("aGk=".into())).unwrap();
        assert_eq!(
            output,
            serde_json::json!({"event": "output", "data": "aGk="})
        );
        let exit = serde_json::to_value(RunEvent::Exit { code: Some(2) }).unwrap();
        assert_eq!(
            exit,
            serde_json::json!({"event": "exit", "data": {"code": 2}})
        );
    }
}
