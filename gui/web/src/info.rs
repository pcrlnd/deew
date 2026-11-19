//! `GET /api/info`: a WebSocket that runs one Utilities command (`deew -v`
//! and the like) in its own pseudo-terminal, independent of the encode.
//!
//! The route sits behind the same auth, Host and Origin checks as the run
//! socket (see `app.rs`). Output goes only to the socket that asked, and the
//! command ends with the socket. Only allowlisted flags are accepted (see
//! `deew_gui_core::info`), never arbitrary arguments.

use crate::app::{AppState, Shared};
use axum::body::Bytes;
use axum::extract::State;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::response::Response;
use deew_gui_core::cli;
use deew_gui_core::info::spawn_info;
use deew_gui_core::pty_session::PtySession;
use serde::Deserialize;
use serde_json::json;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::sync::mpsc;

static NEXT_RUN_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum ClientMessage {
    Start {
        flag: String,
        #[serde(default = "default_cols")]
        cols: u16,
        #[serde(default = "default_rows")]
        rows: u16,
    },
    Resize {
        cols: u16,
        rows: u16,
    },
    /// Terminal replies and keystrokes for the running command.
    Input {
        data: String,
    },
    Stop,
}

fn default_cols() -> u16 {
    80
}
fn default_rows() -> u16 {
    24
}

/// What the pty threads report; `run` tells the runs of one socket apart.
enum RunEvent {
    Output { run: u64, bytes: Vec<u8> },
    Exit { run: u64, code: i32 },
}

pub async fn info_socket(State(state): State<Shared>, ws: WebSocketUpgrade) -> Response {
    ws.max_message_size(64 * 1024)
        .on_upgrade(move |socket| info_connection(state, socket))
}

async fn send_json(socket: &mut WebSocket, value: serde_json::Value) -> bool {
    socket
        .send(Message::Text(value.to_string().into()))
        .await
        .is_ok()
}

async fn info_connection(state: Shared, mut socket: WebSocket) {
    // Bounded: a slow socket slows the reader thread of the pty down.
    let (tx, mut rx) = mpsc::channel::<RunEvent>(256);
    // Dropping the session (when the socket goes away) terminates the command.
    let mut session: Option<(u64, PtySession)> = None;
    loop {
        tokio::select! {
            event = rx.recv() => {
                let Some(event) = event else { return };
                match event {
                    RunEvent::Output { run, bytes } => {
                        if session.as_ref().is_some_and(|(id, _)| *id == run)
                            && socket.send(Message::Binary(Bytes::from(bytes))).await.is_err()
                        {
                            return;
                        }
                    }
                    RunEvent::Exit { run, code } => {
                        if session.as_ref().is_some_and(|(id, _)| *id == run) {
                            session = None;
                            if !send_json(&mut socket, json!({"type": "exit", "code": code})).await {
                                return;
                            }
                        }
                    }
                }
            }
            message = socket.recv() => {
                let text = match message {
                    Some(Ok(Message::Text(text))) => text,
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return,
                    Some(Ok(_)) => continue,
                };
                let error = handle_message(&state, &tx, &mut session, text.as_str()).await;
                if let Some(error) = error
                    && !send_json(&mut socket, json!({"type": "error", "message": error})).await
                {
                    return;
                }
            }
        }
    }
}

/// Returns an error message for the socket, if any.
async fn handle_message(
    state: &AppState,
    tx: &mpsc::Sender<RunEvent>,
    session: &mut Option<(u64, PtySession)>,
    text: &str,
) -> Option<String> {
    let message: ClientMessage = match serde_json::from_str(text) {
        Ok(message) => message,
        Err(e) => return Some(format!("invalid message: {e}")),
    };
    match message {
        ClientMessage::Start { flag, cols, rows } => {
            let run = NEXT_RUN_ID.fetch_add(1, Ordering::Relaxed);
            let gate = state.gate.clone();
            let program = state.program.clone();
            let output_tx = tx.clone();
            let exit_tx = tx.clone();
            // Spawning may block briefly.
            let started = tokio::task::spawn_blocking(move || {
                let program = match program {
                    Some(program) => program,
                    None => cli::sidecar_path()?,
                };
                spawn_info(
                    &gate,
                    program,
                    &flag,
                    (cols, rows),
                    std::env::home_dir(),
                    move |bytes| {
                        let _ = output_tx.blocking_send(RunEvent::Output {
                            run,
                            bytes: bytes.to_vec(),
                        });
                    },
                    move |code| {
                        let _ = exit_tx.blocking_send(RunEvent::Exit { run, code });
                    },
                )
            })
            .await;
            match started {
                Ok(Ok(new)) => {
                    *session = Some((run, new));
                    None
                }
                Ok(Err(e)) => Some(e),
                Err(_) => Some("internal error".into()),
            }
        }
        ClientMessage::Resize { cols, rows } => match session {
            Some((_, session)) => session.resize(cols, rows).err(),
            None => None,
        },
        ClientMessage::Input { data } => match session {
            Some((_, session)) => session.write(data.as_bytes()).err(),
            None => None,
        },
        ClientMessage::Stop => {
            if let Some((_, session)) = session.as_mut() {
                session.stop();
            }
            None
        }
    }
}
