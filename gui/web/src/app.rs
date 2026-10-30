//! The HTTP application: routes, security middleware and the run WebSocket.

use crate::auth::{self, COOKIE_NAME, Sessions};
use crate::config::Credentials;
use crate::job::{Event, Job};
use crate::{assets, fsapi, security};
use axum::Router;
use axum::body::{Body, Bytes};
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Query, Request, State};
use axum::http::{HeaderMap, HeaderValue, Method, StatusCode, header};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use deew_gui_core::cli;
use deew_gui_core::settings::{self, GuiSettings};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Semaphore;

pub struct AppState {
    pub auth: Option<Credentials>,
    pub sessions: Sessions,
    pub job: Job,
    /// Config file of the settings API; `None` uses the one next to the CLI.
    pub config_path: Option<PathBuf>,
    /// Bound to a loopback address: enables the Host check.
    pub loopback: bool,
    /// Delay before a failed login is answered.
    pub login_fail_delay: Duration,
    /// Period of the keep-alive ping on the run socket.
    pub ping_period: Duration,
    /// Limits concurrent (CPU heavy) password verifications.
    verify_permits: Semaphore,
}

impl AppState {
    /// `program` overrides the deew CLI to run (tests); `None` uses the CLI
    /// next to the executable.
    pub fn new(auth: Option<Credentials>, loopback: bool, program: Option<PathBuf>) -> Self {
        Self {
            auth,
            sessions: Sessions::default(),
            job: Job::new(program),
            config_path: None,
            loopback,
            login_fail_delay: Duration::from_secs(1),
            ping_period: Duration::from_secs(5),
            verify_permits: Semaphore::new(2),
        }
    }
}

type Shared = Arc<AppState>;

pub struct ApiError(StatusCode, String);

impl ApiError {
    fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self(status, message.into())
    }
    fn bad_request(message: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, message)
    }
    fn internal(message: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, message)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, axum::Json(json!({ "error": self.1 }))).into_response()
    }
}

fn json_error(status: StatusCode, message: &str) -> Response {
    ApiError::new(status, message).into_response()
}

pub fn router(state: Shared) -> Router {
    Router::new()
        .route("/api/session", get(session))
        .route("/api/login", post(login))
        .route("/api/logout", post(logout))
        .route("/api/version", get(version))
        .route("/api/default-bitrates", get(default_bitrates))
        .route("/api/settings", get(get_settings).post(set_settings))
        .route("/api/probe", post(probe))
        .route("/api/fs", get(fs_list))
        .route("/api/run", get(run_socket))
        .fallback(fallback)
        .layer(middleware::from_fn_with_state(state.clone(), auth_layer))
        .layer(middleware::from_fn_with_state(state.clone(), guard_layer))
        .layer(middleware::from_fn(headers_layer))
        .with_state(state)
}

// ---------------------------------------------------------------- middleware

async fn headers_layer(request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    for (name, value) in security::security_headers() {
        headers.insert(name, HeaderValue::from_static(value));
    }
    response
}

fn header_str(headers: &HeaderMap, name: header::HeaderName) -> Option<&str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

fn is_websocket_upgrade(headers: &HeaderMap) -> bool {
    header_str(headers, header::UPGRADE).is_some_and(|v| v.eq_ignore_ascii_case("websocket"))
}

/// Host (DNS rebinding) and Origin (CSRF) checks.
async fn guard_layer(State(state): State<Shared>, request: Request, next: Next) -> Response {
    let headers = request.headers();
    let host = header_str(headers, header::HOST);
    if !security::host_allowed(host, state.loopback) {
        return json_error(StatusCode::FORBIDDEN, "forbidden: unexpected Host header");
    }
    let needs_origin_check = request.method() != Method::GET || is_websocket_upgrade(headers);
    if needs_origin_check {
        // An Origin header that is present but not valid text never matches.
        let origin = match headers.get(header::ORIGIN) {
            None => None,
            Some(v) => Some(v.to_str().unwrap_or("\u{0}")),
        };
        if !security::origin_allowed(origin, host) {
            return json_error(StatusCode::FORBIDDEN, "forbidden: cross-origin request");
        }
    }
    next.run(request).await
}

fn session_token(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find_map(auth::token_from_cookie_header)
        .map(str::to_owned)
}

fn is_authenticated(state: &AppState, headers: &HeaderMap) -> bool {
    state.auth.is_none()
        || session_token(headers).is_some_and(|token| state.sessions.is_valid(&token))
}

async fn auth_layer(State(state): State<Shared>, request: Request, next: Next) -> Response {
    let path = request.uri().path();
    let is_api = path == "/api" || path.starts_with("/api/");
    let public = (request.method() == Method::GET && path == "/api/session")
        || (request.method() == Method::POST && path == "/api/login");
    if is_api && !public && !is_authenticated(&state, request.headers()) {
        return json_error(StatusCode::UNAUTHORIZED, "login required");
    }
    next.run(request).await
}

// ------------------------------------------------------------------ handlers

async fn fallback(request: Request) -> Response {
    let path = request.uri().path();
    if path == "/api" || path.starts_with("/api/") {
        return json_error(StatusCode::NOT_FOUND, "not found");
    }
    match *request.method() {
        Method::GET => assets::serve(path),
        Method::HEAD => {
            let (parts, _) = assets::serve(path).into_parts();
            Response::from_parts(parts, Body::empty())
        }
        _ => json_error(StatusCode::METHOD_NOT_ALLOWED, "method not allowed"),
    }
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SessionInfo {
    auth_required: bool,
    authenticated: bool,
}

async fn session(State(state): State<Shared>, headers: HeaderMap) -> axum::Json<SessionInfo> {
    axum::Json(SessionInfo {
        auth_required: state.auth.is_some(),
        authenticated: is_authenticated(&state, &headers),
    })
}

fn parse_json<T: serde::de::DeserializeOwned>(body: &Bytes) -> Result<T, ApiError> {
    serde_json::from_slice(body).map_err(|e| ApiError::bad_request(format!("invalid request: {e}")))
}

#[derive(Deserialize)]
struct LoginBody {
    user: String,
    pass: String,
}

fn cookie_header(value: &str) -> Result<HeaderValue, ApiError> {
    HeaderValue::from_str(value).map_err(|_| ApiError::internal("invalid cookie"))
}

async fn login(State(state): State<Shared>, body: Bytes) -> Result<Response, ApiError> {
    let body: LoginBody = parse_json(&body)?;
    let Some(creds) = state.auth.clone() else {
        // No login configured: nothing to do, but keep the client flow working.
        return Ok(StatusCode::NO_CONTENT.into_response());
    };
    let permit = state
        .verify_permits
        .acquire()
        .await
        .map_err(|_| ApiError::internal("server shutting down"))?;
    let ok = tokio::task::spawn_blocking(move || {
        auth::check_credentials(&creds, &body.user, &body.pass)
    })
    .await
    .map_err(|_| ApiError::internal("login check failed"))?;
    drop(permit);
    if !ok {
        tokio::time::sleep(state.login_fail_delay).await;
        return Err(ApiError::new(
            StatusCode::UNAUTHORIZED,
            "wrong username or password",
        ));
    }
    let token = state.sessions.create().map_err(ApiError::internal)?;
    let cookie = cookie_header(&format!(
        "{COOKIE_NAME}={token}; HttpOnly; SameSite=Strict; Path=/"
    ))?;
    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().insert(header::SET_COOKIE, cookie);
    Ok(response)
}

async fn logout(State(state): State<Shared>, headers: HeaderMap) -> Result<Response, ApiError> {
    if let Some(token) = session_token(&headers) {
        state.sessions.remove(&token);
    }
    let cookie = cookie_header(&format!(
        "{COOKIE_NAME}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0; Expires=Thu, 01 Jan 1970 00:00:00 GMT"
    ))?;
    let mut response = StatusCode::NO_CONTENT.into_response();
    response.headers_mut().insert(header::SET_COOKIE, cookie);
    Ok(response)
}

async fn version() -> Result<axum::Json<serde_json::Value>, ApiError> {
    let version = tokio::task::spawn_blocking(cli::cli_version)
        .await
        .map_err(|e| ApiError::internal(format!("version check failed: {e}")))?
        .map_err(ApiError::internal)?;
    Ok(axum::Json(json!({ "version": version })))
}

async fn default_bitrates() -> axum::Json<std::collections::BTreeMap<String, i64>> {
    axum::Json(
        tokio::task::spawn_blocking(cli::default_bitrates)
            .await
            .unwrap_or_default(),
    )
}

fn config_path(state: &AppState) -> Option<PathBuf> {
    state.config_path.clone().or_else(cli::config_path)
}

async fn get_settings(State(state): State<Shared>) -> axum::Json<GuiSettings> {
    axum::Json(
        tokio::task::spawn_blocking(move || settings::load(config_path(&state).as_deref()))
            .await
            .unwrap_or_default(),
    )
}

async fn set_settings(State(state): State<Shared>, body: Bytes) -> Result<StatusCode, ApiError> {
    let new: GuiSettings = parse_json(&body)?;
    tokio::task::spawn_blocking(move || settings::save(config_path(&state).as_deref(), &new))
        .await
        .map_err(|e| ApiError::internal(format!("saving the settings failed: {e}")))?
        .map_err(ApiError::bad_request)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProbeBody {
    inputs: Vec<String>,
    track_index: u32,
}

async fn probe(body: Bytes) -> Result<Response, ApiError> {
    let body: ProbeBody = parse_json(&body)?;
    let result =
        tokio::task::spawn_blocking(move || cli::probe_inputs(&body.inputs, body.track_index))
            .await
            .map_err(|e| ApiError::internal(format!("probing failed: {e}")))?
            .map_err(ApiError::internal)?;
    Ok(axum::Json(result).into_response())
}

async fn fs_list(Query(query): Query<HashMap<String, String>>) -> Result<Response, ApiError> {
    let path = query.get("path").cloned();
    let listing = tokio::task::spawn_blocking(move || fsapi::list_dir(path.as_deref()))
        .await
        .map_err(|e| ApiError::internal(format!("listing failed: {e}")))?
        .map_err(ApiError::bad_request)?;
    Ok(axum::Json(listing).into_response())
}

// ----------------------------------------------------------------- websocket

#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
enum ClientMessage {
    Start {
        args: Vec<String>,
        #[serde(default = "default_cols")]
        cols: u16,
        #[serde(default = "default_rows")]
        rows: u16,
        #[serde(default)]
        native: bool,
    },
    Input {
        data: String,
    },
    Resize {
        cols: u16,
        rows: u16,
    },
    Stop,
}

fn default_cols() -> u16 {
    80
}
fn default_rows() -> u16 {
    24
}

async fn run_socket(State(state): State<Shared>, ws: WebSocketUpgrade) -> Response {
    ws.max_message_size(1024 * 1024)
        .on_upgrade(move |socket| run_connection(state, socket))
}

/// Sent as text; `None` when the socket is gone.
async fn send_json(socket: &mut WebSocket, value: serde_json::Value) -> Option<()> {
    socket
        .send(Message::Text(value.to_string().into()))
        .await
        .ok()
}

fn event_message(event: Event) -> Message {
    match event {
        Event::Output(bytes) => Message::Binary(bytes),
        Event::Progress(line) => {
            Message::Text(json!({"type": "progress", "data": line}).to_string().into())
        }
        Event::Started(args) => {
            Message::Text(json!({"type": "started", "args": args}).to_string().into())
        }
        Event::Exit(code) => {
            Message::Text(json!({"type": "exit", "code": code}).to_string().into())
        }
    }
}

async fn run_connection(state: Shared, mut socket: WebSocket) {
    let mut sub = state.job.subscribe();
    let id = sub.id;
    serve_socket(&state, &mut socket, &mut sub).await;
    state.job.unsubscribe(id);
    // A socket that fell behind was dropped by the job; say goodbye politely.
    let _ = socket
        .send(Message::Close(Some(CloseFrame {
            code: 1000,
            reason: "".into(),
        })))
        .await;
}

async fn serve_socket(state: &Shared, socket: &mut WebSocket, sub: &mut crate::job::Subscription) {
    let initial = json!({"type": "state", "running": sub.running, "args": sub.args});
    if send_json(socket, initial).await.is_none() {
        return;
    }
    for line in std::mem::take(&mut sub.progress) {
        if send_json(socket, json!({"type": "progress", "data": line}))
            .await
            .is_none()
        {
            return;
        }
    }
    for chunk in std::mem::take(&mut sub.replay).chunks(64 * 1024) {
        if socket
            .send(Message::Binary(Bytes::copy_from_slice(chunk)))
            .await
            .is_err()
        {
            return;
        }
    }
    // Lets the client notice a half-open connection.
    let mut ping = tokio::time::interval_at(
        tokio::time::Instant::now() + state.ping_period,
        state.ping_period,
    );
    ping.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            _ = ping.tick() => {
                if send_json(socket, json!({"type": "ping"})).await.is_none() {
                    return;
                }
            }
            event = sub.events.recv() => {
                // None: dropped by the job because this socket was too slow.
                let Some(event) = event else { return };
                if socket.send(event_message(event)).await.is_err() {
                    return;
                }
            }
            message = socket.recv() => {
                let text = match message {
                    Some(Ok(Message::Text(text))) => text,
                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return,
                    Some(Ok(_)) => continue,
                };
                if let Some(error) = handle_client_message(state, text.as_str()).await
                    && send_json(socket, json!({"type": "error", "message": error})).await.is_none()
                {
                    return;
                }
            }
        }
    }
}

/// Returns an error message for the requesting socket, if any.
async fn handle_client_message(state: &Shared, text: &str) -> Option<String> {
    let message: ClientMessage = match serde_json::from_str(text) {
        Ok(message) => message,
        Err(e) => return Some(format!("invalid message: {e}")),
    };
    let state = Arc::clone(state);
    // Starting and writing touch the pty and may block briefly.
    let result = tokio::task::spawn_blocking(move || match message {
        ClientMessage::Start {
            args,
            cols,
            rows,
            native,
        } => state.job.start(args, cols, rows, native),
        ClientMessage::Input { data } => state.job.write(data.as_bytes()),
        ClientMessage::Resize { cols, rows } => state.job.resize(cols, rows),
        ClientMessage::Stop => {
            state.job.stop();
            Ok(())
        }
    })
    .await;
    match result {
        Ok(Ok(())) => None,
        Ok(Err(e)) => Some(e),
        Err(_) => Some("internal error".into()),
    }
}
