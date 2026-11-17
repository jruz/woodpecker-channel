use tokio::io::AsyncBufReadExt as _;
use tokio::io::AsyncWriteExt as _;

pub const RELEASE_COMMIT: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
pub const BOT_COMMIT: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

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

pub fn text(response: &serde_json::Value) -> &str {
    let content = at(response, "/result/content/0/text");
    content.as_str().expect("text content")
}

pub fn pipeline(number: u64, status: &str, commit: &str) -> String {
    format!(
        r#"{{"author": "jruz", "branch": "main", "commit": "{commit}", "event": "push", "message": "change", "number": {number}, "status": "{status}"}}"#
    )
}

pub fn pipelines(entries: &[String]) -> String {
    let joined = entries.join(", ");
    format!("[{joined}]")
}

fn json_response(body: &str) -> wiremock::ResponseTemplate {
    let template = wiremock::ResponseTemplate::new(200);
    template.set_body_raw(body, "application/json")
}

async fn mount_json(server: &wiremock::MockServer, route: &str, body: &str) {
    let get = wiremock::matchers::method("GET");
    let path = wiremock::matchers::path(route);
    let response = json_response(body);
    let mock = wiremock::Mock::given(get).and(path).respond_with(response);
    mock.mount(server).await;
}

async fn mount_first_poll(server: &wiremock::MockServer, body: &str) {
    let get = wiremock::matchers::method("GET");
    let path = wiremock::matchers::path("/api/repos/7/pipelines");
    let response = json_response(body);
    let mock = wiremock::Mock::given(get)
        .and(path)
        .respond_with(response)
        .up_to_n_times(1)
        .with_priority(1);
    mock.mount(server).await;
}

pub async fn woodpecker_mock(first_poll: &str, later_polls: &str) -> wiremock::MockServer {
    let server = wiremock::MockServer::start().await;
    mount_json(&server, "/api/repos/lookup/jruz/demo", r#"{"id": 7}"#).await;
    mount_first_poll(&server, first_poll).await;
    mount_json(&server, "/api/repos/7/pipelines", later_polls).await;
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
    let limit = std::time::Duration::from_secs(15);
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

pub fn commit_arguments(commit: &str) -> String {
    format!(r#"{{"repo": "jruz/demo", "commit": "{commit}"}}"#)
}
