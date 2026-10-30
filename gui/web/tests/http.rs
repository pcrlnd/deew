#![cfg_attr(not(unix), allow(dead_code, unused_imports))]
use axum::Router;
use axum::body::Body;
use axum::http::{Request, Response, StatusCode, header};
use deew_web::config::Credentials;
use deew_web::{AppState, auth, router};
use futures_util::{SinkExt, StreamExt};
use http_body_util::BodyExt;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

fn state(auth_on: bool) -> Arc<AppState> {
    let creds = auth_on.then(|| Credentials {
        user: "bob".into(),
        hash: auth::hash_password("secret").unwrap(),
    });
    let mut state = AppState::new(creds, true, None);
    state.login_fail_delay = Duration::from_millis(10);
    Arc::new(state)
}

async fn send(app: &Router, req: Request<Body>) -> Response<Body> {
    app.clone().oneshot(req).await.unwrap()
}

fn get(path: &str) -> axum::http::request::Builder {
    Request::builder()
        .uri(path)
        .header(header::HOST, "localhost:8765")
}

async fn body_json(response: Response<Body>) -> serde_json::Value {
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    serde_json::from_slice(&bytes).unwrap()
}

fn login_request(user: &str, pass: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri("/api/login")
        .header(header::HOST, "localhost:8765")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(
            serde_json::json!({"user": user, "pass": pass}).to_string(),
        ))
        .unwrap()
}

#[tokio::test]
async fn session_without_auth() {
    let app = router(state(false));
    let r = send(&app, get("/api/session").body(Body::empty()).unwrap()).await;
    assert_eq!(r.status(), StatusCode::OK);
    assert_eq!(
        body_json(r).await,
        serde_json::json!({"authRequired": false, "authenticated": true})
    );
}

#[tokio::test]
async fn login_flow() {
    let app = router(state(true));
    let r = send(&app, get("/api/session").body(Body::empty()).unwrap()).await;
    assert_eq!(
        body_json(r).await,
        serde_json::json!({"authRequired": true, "authenticated": false})
    );

    // 401 without a cookie
    let r = send(&app, get("/api/fs").body(Body::empty()).unwrap()).await;
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        body_json(r).await,
        serde_json::json!({"error": "login required"})
    );
    let r = send(&app, get("/api/nothing").body(Body::empty()).unwrap()).await;
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);

    // wrong credentials
    let r = send(&app, login_request("bob", "wrong")).await;
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
    assert!(r.headers().get(header::SET_COOKIE).is_none());
    assert_eq!(
        body_json(r).await,
        serde_json::json!({"error": "wrong username or password"})
    );
    let r = send(&app, login_request("alice", "secret")).await;
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);

    // success
    let r = send(&app, login_request("bob", "secret")).await;
    assert_eq!(r.status(), StatusCode::NO_CONTENT);
    let cookie = r.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .to_string();
    assert!(cookie.starts_with("deew_session="), "{cookie}");
    assert!(cookie.contains("HttpOnly") && cookie.contains("SameSite=Strict"));
    assert!(cookie.contains("Path=/") && !cookie.contains("Max-Age"));
    let pair = cookie.split(';').next().unwrap().to_string();

    let r = send(
        &app,
        get("/api/session")
            .header(header::COOKIE, &pair)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(
        body_json(r).await,
        serde_json::json!({"authRequired": true, "authenticated": true})
    );
    let r = send(
        &app,
        get("/api/fs")
            .header(header::COOKIE, &pair)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(r.status(), StatusCode::OK);
    let json = body_json(r).await;
    assert!(json["entries"].is_array() && json["roots"].is_array());

    // logout
    let r = send(
        &app,
        Request::builder()
            .method("POST")
            .uri("/api/logout")
            .header(header::HOST, "localhost")
            .header(header::COOKIE, &pair)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(r.status(), StatusCode::NO_CONTENT);
    let r = send(
        &app,
        get("/api/fs")
            .header(header::COOKIE, &pair)
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn bad_host_and_origin_are_forbidden() {
    let app = router(state(false));
    let r = send(
        &app,
        Request::builder()
            .uri("/api/session")
            .header(header::HOST, "evil.com")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(r.status(), StatusCode::FORBIDDEN);
    assert!(r.headers().contains_key(header::CONTENT_SECURITY_POLICY));

    let r = send(
        &app,
        Request::builder().uri("/").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(r.status(), StatusCode::FORBIDDEN);

    // cross-origin POST
    let mut req = login_request("a", "b");
    req.headers_mut()
        .insert(header::ORIGIN, "http://evil.com".parse().unwrap());
    assert_eq!(send(&app, req).await.status(), StatusCode::FORBIDDEN);

    // same origin POST passes the origin check (no login configured -> 204)
    let mut req = login_request("a", "b");
    req.headers_mut()
        .insert(header::ORIGIN, "http://localhost:8765".parse().unwrap());
    assert_eq!(send(&app, req).await.status(), StatusCode::NO_CONTENT);

    // GET with a foreign Origin is not checked (not a websocket upgrade)
    let r = send(
        &app,
        get("/api/session")
            .header(header::ORIGIN, "http://evil.com")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(r.status(), StatusCode::OK);

    // websocket upgrade with a foreign Origin
    let r = send(
        &app,
        get("/api/run")
            .header(header::UPGRADE, "websocket")
            .header(header::CONNECTION, "upgrade")
            .header(header::ORIGIN, "http://evil.com")
            .header("sec-websocket-version", "13")
            .header("sec-websocket-key", "dGhlIHNhbXBsZSBub25jZQ==")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(r.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn non_loopback_bind_accepts_any_host() {
    let app = router(Arc::new(AppState::new(None, false, None)));
    let r = send(
        &app,
        Request::builder()
            .uri("/api/session")
            .header(header::HOST, "nas.lan:8765")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(r.status(), StatusCode::OK);
}

#[tokio::test]
async fn static_files_and_security_headers() {
    let app = router(state(true));
    for path in ["/", "/some/client/route"] {
        let r = send(&app, get(path).body(Body::empty()).unwrap()).await;
        assert_eq!(r.status(), StatusCode::OK, "{path}");
        assert_eq!(r.headers()[header::CONTENT_TYPE], "text/html");
        assert_eq!(
            r.headers()[header::CONTENT_SECURITY_POLICY],
            deew_web::security::CSP
        );
        assert_eq!(r.headers()["x-content-type-options"], "nosniff");
        assert_eq!(r.headers()["referrer-policy"], "no-referrer");
        assert_eq!(r.headers()["x-frame-options"], "DENY");
        let body = r.into_body().collect().await.unwrap().to_bytes();
        assert!(String::from_utf8_lossy(&body).contains("<div id=\"root\">"));
    }
    // an asset with its own mime type, and headers on API responses
    let html = String::from_utf8(
        send(&app, get("/").body(Body::empty()).unwrap())
            .await
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    let js = html
        .split('"')
        .find(|s| s.starts_with("/assets/") && s.ends_with(".js"))
        .unwrap();
    let r = send(&app, get(js).body(Body::empty()).unwrap()).await;
    assert_eq!(r.status(), StatusCode::OK);
    assert!(
        r.headers()[header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .contains("javascript")
    );
    let r = send(&app, get("/api/session").body(Body::empty()).unwrap()).await;
    assert!(r.headers().contains_key(header::CONTENT_SECURITY_POLICY));
}

// ------------------------------------------------------------------ settings

fn settings_request(method: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method(method)
        .uri("/api/settings")
        .header(header::HOST, "localhost:8765")
        .header(header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .unwrap()
}

fn state_with_config(auth_on: bool, config: &std::path::Path) -> Arc<AppState> {
    let creds = auth_on.then(|| Credentials {
        user: "bob".into(),
        hash: auth::hash_password("secret").unwrap(),
    });
    let mut state = AppState::new(creds, true, None);
    state.config_path = Some(config.to_path_buf());
    Arc::new(state)
}

#[tokio::test]
async fn settings_round_trip() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    std::fs::write(&config, "# my config\nkeep = 'me'\n").unwrap();
    let app = router(state_with_config(false, &config));

    let r = send(&app, get("/api/settings").body(Body::empty()).unwrap()).await;
    assert_eq!(r.status(), StatusCode::OK);
    assert_eq!(body_json(r).await, serde_json::json!({"progress": "cli"}));

    let r = send(&app, settings_request("POST", r#"{"progress":"native"}"#)).await;
    assert_eq!(r.status(), StatusCode::NO_CONTENT);
    let saved = std::fs::read_to_string(&config).unwrap();
    assert!(saved.starts_with("# my config\nkeep = 'me'\n"), "{saved}");
    assert!(saved.contains("progress = 'native'"), "{saved}");

    let r = send(&app, get("/api/settings").body(Body::empty()).unwrap()).await;
    assert_eq!(
        body_json(r).await,
        serde_json::json!({"progress": "native"})
    );
}

#[tokio::test]
async fn settings_errors() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    let app = router(state_with_config(false, &config));

    // no config yet: reading gives the default, saving explains what to do
    let r = send(&app, get("/api/settings").body(Body::empty()).unwrap()).await;
    assert_eq!(body_json(r).await, serde_json::json!({"progress": "cli"}));
    let r = send(&app, settings_request("POST", r#"{"progress":"native"}"#)).await;
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    let error = body_json(r).await;
    assert!(
        error["error"].as_str().unwrap().contains("does not exist"),
        "{error}"
    );
    assert!(!config.exists());

    // bad bodies
    for body in ["nope", r#"{"progress":"x"}"#, "{}"] {
        let r = send(&app, settings_request("POST", body)).await;
        assert_eq!(r.status(), StatusCode::BAD_REQUEST, "{body}");
        assert!(body_json(r).await["error"].is_string());
    }

    // invalid config.toml is reported and left alone
    std::fs::write(&config, "a = [\n").unwrap();
    let r = send(&app, settings_request("POST", r#"{"progress":"native"}"#)).await;
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    assert!(
        body_json(r).await["error"]
            .as_str()
            .unwrap()
            .contains("not valid TOML")
    );
    assert_eq!(std::fs::read_to_string(&config).unwrap(), "a = [\n");
}

#[tokio::test]
async fn settings_need_login_and_same_origin() {
    let dir = tempfile::tempdir().unwrap();
    let config = dir.path().join("config.toml");
    std::fs::write(&config, "a = 1\n").unwrap();
    let app = router(state_with_config(true, &config));

    let r = send(&app, get("/api/settings").body(Body::empty()).unwrap()).await;
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
    let r = send(&app, settings_request("POST", r#"{"progress":"native"}"#)).await;
    assert_eq!(r.status(), StatusCode::UNAUTHORIZED);

    // cross-origin POST is rejected before anything else
    let app = router(state_with_config(false, &config));
    let mut req = settings_request("POST", r#"{"progress":"native"}"#);
    req.headers_mut()
        .insert(header::ORIGIN, "http://evil.com".parse().unwrap());
    assert_eq!(send(&app, req).await.status(), StatusCode::FORBIDDEN);
    assert_eq!(std::fs::read_to_string(&config).unwrap(), "a = 1\n");
}

// ---------------------------------------------------------------- websocket

type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn serve(program: std::path::PathBuf) -> (String, Arc<AppState>) {
    serve_with(program, Duration::from_secs(3600)).await
}

async fn serve_with(program: std::path::PathBuf, ping_period: Duration) -> (String, Arc<AppState>) {
    let mut state = AppState::new(None, true, Some(program));
    state.login_fail_delay = Duration::ZERO;
    state.ping_period = ping_period;
    let state = Arc::new(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(axum::serve(listener, router(state.clone())).into_future());
    (format!("ws://{addr}/api/run"), state)
}

#[derive(Debug, PartialEq)]
enum Frame {
    Text(serde_json::Value),
    Binary(Vec<u8>),
}

async fn next_frame(ws: &mut Ws) -> Frame {
    use tokio_tungstenite::tungstenite::Message;
    loop {
        let msg = tokio::time::timeout(Duration::from_secs(20), ws.next())
            .await
            .expect("timed out waiting for a frame")
            .expect("socket closed")
            .unwrap();
        match msg {
            Message::Text(t) => return Frame::Text(serde_json::from_str(t.as_str()).unwrap()),
            Message::Binary(b) => return Frame::Binary(b.to_vec()),
            _ => continue,
        }
    }
}

async fn send_text(ws: &mut Ws, value: serde_json::Value) {
    ws.send(tokio_tungstenite::tungstenite::Message::Text(
        value.to_string().into(),
    ))
    .await
    .unwrap();
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn run_fake_cli_over_websocket_with_replay() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("fake-deew");
    std::fs::write(&script, "#!/bin/sh\necho hello \"$@\"\nexit 3\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (url, state) = serve(script).await;

    let (mut a, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    assert_eq!(
        next_frame(&mut a).await,
        Frame::Text(serde_json::json!({"type": "state", "running": false, "args": null}))
    );
    send_text(
        &mut a,
        serde_json::json!({"type": "start", "args": ["x", "y"], "cols": 80, "rows": 24}),
    )
    .await;
    assert_eq!(
        next_frame(&mut a).await,
        Frame::Text(serde_json::json!({"type": "started", "args": ["x", "y"]}))
    );
    let mut output = Vec::new();
    let exit = loop {
        match next_frame(&mut a).await {
            Frame::Binary(b) => output.extend(b),
            Frame::Text(v) => break v,
        }
    };
    assert_eq!(exit, serde_json::json!({"type": "exit", "code": 3}));
    let text = String::from_utf8_lossy(&output).to_string();
    assert!(text.contains("hello x y"), "{text:?}");

    // a socket connecting after the job ended gets the state but no replay
    let (mut b, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    assert_eq!(
        next_frame(&mut b).await,
        Frame::Text(serde_json::json!({"type": "state", "running": false, "args": ["x", "y"]}))
    );

    // no binary frame follows: the next frame is the answer to a bad message
    send_text(&mut b, serde_json::json!({"type": "bogus"})).await;
    match next_frame(&mut b).await {
        Frame::Text(v) => assert_eq!(v["type"], "error"),
        other => panic!("{other:?}"),
    }
    assert!(!state.job.is_running());
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn second_start_is_rejected_and_stop_works() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let script = dir.path().join("sleepy");
    std::fs::write(&script, "#!/bin/sh\necho up\nsleep 30\n").unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let (url, _state) = serve(script).await;

    let (mut a, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    let (mut b, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    next_frame(&mut a).await;
    next_frame(&mut b).await;
    send_text(&mut a, serde_json::json!({"type": "start", "args": []})).await;
    // both sockets learn about the start
    for ws in [&mut a, &mut b] {
        assert_eq!(
            next_frame(ws).await,
            Frame::Text(serde_json::json!({"type": "started", "args": []}))
        );
    }
    // wait for output, then try to start again from b
    loop {
        if let Frame::Binary(_) = next_frame(&mut b).await {
            break;
        }
    }
    send_text(&mut b, serde_json::json!({"type": "start", "args": []})).await;
    loop {
        if let Frame::Text(v) = next_frame(&mut b).await {
            assert_eq!(v["type"], "error");
            assert!(v["message"].as_str().unwrap().contains("already running"));
            break;
        }
    }
    // a socket attaching mid-job gets the state and the buffered output
    let (mut c, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    assert_eq!(
        next_frame(&mut c).await,
        Frame::Text(serde_json::json!({"type": "state", "running": true, "args": []}))
    );
    match next_frame(&mut c).await {
        Frame::Binary(b) => assert!(String::from_utf8_lossy(&b).contains("up")),
        other => panic!("{other:?}"),
    }
    drop(c);
    // closing a socket does not stop the job; stop does
    drop(a);
    send_text(&mut b, serde_json::json!({"type": "stop"})).await;
    loop {
        if let Frame::Text(v) = next_frame(&mut b).await {
            assert_eq!(v["type"], "exit");
            break;
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn socket_receives_pings() {
    let (url, _state) = serve_with("unused".into(), Duration::from_millis(50)).await;
    let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    assert_eq!(
        next_frame(&mut ws).await,
        Frame::Text(serde_json::json!({"type": "state", "running": false, "args": null}))
    );
    for _ in 0..2 {
        assert_eq!(
            next_frame(&mut ws).await,
            Frame::Text(serde_json::json!({"type": "ping"}))
        );
    }
}

#[cfg(unix)]
fn script(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = dir.join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn native_run_forwards_events_before_exit() {
    let dir = tempfile::tempdir().unwrap();
    let line = r#"{"type":"progress","job":0,"percent":42}"#;
    let program = script(
        dir.path(),
        "events",
        &format!(
            "echo '{line}' >> \"$DEEW_EVENTS_FILE\"\nprintf '{{\"type\":\"done\",\"job\":0}}' >> \"$DEEW_EVENTS_FILE\""
        ),
    );
    let (url, _state) = serve(program).await;

    let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    next_frame(&mut ws).await;
    send_text(
        &mut ws,
        serde_json::json!({"type": "start", "args": [], "native": true}),
    )
    .await;
    let mut progress = Vec::new();
    let exit = loop {
        match next_frame(&mut ws).await {
            Frame::Text(v) if v["type"] == "progress" => progress.push(v["data"].clone()),
            Frame::Text(v) if v["type"] == "exit" => break v,
            _ => {}
        }
    };
    assert_eq!(exit["code"], 0);
    assert_eq!(
        progress,
        [
            serde_json::json!(line),
            serde_json::json!(r#"{"type":"done","job":0}"#)
        ]
    );
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn events_are_off_without_native() {
    let dir = tempfile::tempdir().unwrap();
    let program = script(dir.path(), "plain", "echo \"events:[$DEEW_EVENTS_FILE]\"");
    let (url, _state) = serve(program).await;

    let (mut ws, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    next_frame(&mut ws).await;
    send_text(&mut ws, serde_json::json!({"type": "start", "args": []})).await;
    let mut output = Vec::new();
    loop {
        match next_frame(&mut ws).await {
            Frame::Binary(b) => output.extend(b),
            Frame::Text(v) if v["type"] == "exit" => break,
            Frame::Text(v) => assert_ne!(v["type"], "progress"),
        }
    }
    assert!(String::from_utf8_lossy(&output).contains("events:[]"));
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn reconnect_replays_compacted_events_before_output() {
    let dir = tempfile::tempdir().unwrap();
    let program = script(
        dir.path(),
        "long",
        "printf '%s\\n' \
           '{\"type\":\"progress\",\"job\":0,\"percent\":10}' \
           '{\"type\":\"progress\",\"job\":0,\"percent\":60}' \
           '{\"type\":\"warning\",\"text\":\"w\"}' >> \"$DEEW_EVENTS_FILE\"\n\
         echo up\nsleep 30",
    );
    let (url, _state) = serve(program).await;

    let (mut a, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    next_frame(&mut a).await;
    send_text(
        &mut a,
        serde_json::json!({"type": "start", "args": [], "native": true}),
    )
    .await;
    // all three lines (and the output) arrived
    let (mut lines, mut up) = (0, false);
    while lines < 3 || !up {
        match next_frame(&mut a).await {
            Frame::Text(v) if v["type"] == "progress" => lines += 1,
            Frame::Binary(_) => up = true,
            _ => {}
        }
    }

    let (mut b, _) = tokio_tungstenite::connect_async(&url).await.unwrap();
    let Frame::Text(state) = next_frame(&mut b).await else {
        panic!("expected the state")
    };
    assert_eq!(state["running"], true);
    let expected = [
        r#"{"type":"progress","job":0,"percent":60}"#,
        r#"{"type":"warning","text":"w"}"#,
    ];
    for line in expected {
        assert_eq!(
            next_frame(&mut b).await,
            Frame::Text(serde_json::json!({"type": "progress", "data": line}))
        );
    }
    // the binary replay comes after the stored lines
    assert!(matches!(next_frame(&mut b).await, Frame::Binary(_)));

    send_text(&mut a, serde_json::json!({"type": "stop"})).await;
    loop {
        if let Frame::Text(v) = next_frame(&mut a).await
            && v["type"] == "exit"
        {
            break;
        }
    }
}
