use tokio::io::AsyncBufReadExt as _;
use tokio::io::AsyncWriteExt as _;

const PIPELINES: &str = r#"[{"author": "jruz", "branch": "main", "commit": "abc123", "event": "push", "message": "break things", "number": 3, "status": "failure"}]"#;

const DETAIL: &str = r#"{"workflows": [{"children": [{"id": 2, "state": "failure"}]}]}"#;

const LOGS: &str =
    r#"[{"data": "Y2FyZ28gdGVzdA==", "line": 0}, {"data": "dGVzdCBmYWlsZWQ=", "line": 1}]"#;

pub struct Session {
    pub child: tokio::process::Child,
    pub lines: tokio::io::Lines<tokio::io::BufReader<tokio::process::ChildStdout>>,
    pub stdin: tokio::process::ChildStdin,
}

pub fn json(text: &str) -> serde_json::Value {
    serde_json::from_str(text).expect("JSON fixture")
}

pub fn at<'value>(value: &'value serde_json::Value, pointer: &str) -> &'value serde_json::Value {
    value.pointer(pointer).expect(pointer)
}

async fn mount_json(server: &wiremock::MockServer, route: &str, body: &str) {
    let get = wiremock::matchers::method("GET");
    let path = wiremock::matchers::path(route);
    let template = wiremock::ResponseTemplate::new(200);
    let response = template.set_body_raw(body, "application/json");
    let mock = wiremock::Mock::given(get).and(path).respond_with(response);
    mock.mount(server).await;
}

pub async fn failing_pipeline_mock() -> wiremock::MockServer {
    let server = wiremock::MockServer::start().await;
    mount_json(&server, "/api/repos/lookup/jruz/demo", r#"{"id": 7}"#).await;
    mount_json(&server, "/api/repos/7/pipelines", PIPELINES).await;
    mount_json(&server, "/api/repos/7/pipelines/3", DETAIL).await;
    mount_json(&server, "/api/repos/7/logs/3/2", LOGS).await;
    server
}

pub fn spawn(server: &wiremock::MockServer) -> Session {
    let binary = env!("CARGO_BIN_EXE_woodpecker-channel");
    let home = env!("CARGO_TARGET_TMPDIR");
    let uri = server.uri();
    let input = std::process::Stdio::piped();
    let output = std::process::Stdio::piped();
    let errors = std::process::Stdio::piped();
    let mut command = tokio::process::Command::new(binary);
    command
        .arg("serve")
        .env("WOODPECKER_SERVER", uri)
        .env("WOODPECKER_TOKEN", "secret")
        .env("HOME", home)
        .stdin(input)
        .stdout(output)
        .stderr(errors)
        .kill_on_drop(true);
    let mut child = command.spawn().expect("spawn server");
    let stdin = child.stdin.take().expect("stdin");
    let stdout = child.stdout.take().expect("stdout");
    let reader = tokio::io::BufReader::new(stdout);
    Session {
        child,
        lines: reader.lines(),
        stdin,
    }
}

pub async fn send(session: &mut Session, message: &str) {
    let line = format!("{message}\n");
    let bytes = line.as_bytes();
    session.stdin.write_all(bytes).await.expect("write stdin");
}

pub async fn receive(session: &mut Session) -> serde_json::Value {
    let limit = std::time::Duration::from_secs(10);
    let next = session.lines.next_line();
    let line = tokio::time::timeout(limit, next)
        .await
        .expect("message before timeout")
        .expect("read stdout")
        .expect("open stdout");
    json(&line)
}

pub async fn initialize(session: &mut Session) -> serde_json::Value {
    send(
        session,
        r#"{"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "test", "version": "0"}}}"#,
    )
    .await;
    let response = receive(session).await;
    send(
        session,
        r#"{"jsonrpc": "2.0", "method": "notifications/initialized"}"#,
    )
    .await;
    response
}

pub async fn call_tool(session: &mut Session, name: &str, arguments: &str) -> serde_json::Value {
    let message = format!(
        r#"{{"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {{"name": "{name}", "arguments": {arguments}}}}}"#
    );
    send(session, &message).await;
    receive(session).await
}
