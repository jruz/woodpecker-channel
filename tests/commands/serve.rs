use crate::utils::stdio::{
    BOT_COMMIT, RELEASE_COMMIT, at, call_tool, commit_arguments, initialize, json, pipeline,
    pipelines, receive, send, spawn, text, woodpecker_mock,
};

async fn idle_mock() -> wiremock::MockServer {
    let release = pipeline(3, "success", RELEASE_COMMIT);
    let body = pipelines(&[release]);
    woodpecker_mock(&body, &body).await
}

#[tokio::test]
async fn declares_channel_capability() {
    let server = idle_mock().await;
    let mut session = spawn(&server);
    let response = initialize(&mut session).await;
    assert_eq!(at(&response, "/result/protocolVersion"), "2025-06-18");
    assert_eq!(
        at(
            &response,
            "/result/capabilities/experimental/claude~1channel"
        ),
        &json("{}")
    );
    session.child.kill().await.expect("kill");
}

#[tokio::test]
async fn lists_pi_style_tools() {
    let server = idle_mock().await;
    let mut session = spawn(&server);
    initialize(&mut session).await;
    send(
        &mut session,
        r#"{"jsonrpc": "2.0", "id": 2, "method": "tools/list"}"#,
    )
    .await;
    let response = receive(&mut session).await;
    let names: Vec<&serde_json::Value> = at(&response, "/result/tools")
        .as_array()
        .expect("tools")
        .iter()
        .map(|tool| at(tool, "/name"))
        .collect();
    assert_eq!(names, vec!["watch", "unwatch", "status"]);
    session.child.kill().await.expect("kill");
}

#[tokio::test]
async fn watch_returns_commit_baseline() {
    let release = pipeline(3, "success", RELEASE_COMMIT);
    let bot = pipeline(4, "running", BOT_COMMIT);
    let body = pipelines(&[bot, release]);
    let server = woodpecker_mock(&body, &body).await;
    let mut session = spawn(&server);
    initialize(&mut session).await;
    let arguments = commit_arguments(RELEASE_COMMIT);
    let response = call_tool(&mut session, "watch", &arguments).await;
    let expected = format!(
        "watching jruz/demo/commit:{RELEASE_COMMIT} \u{2014} polling every 5s\n#3 push main success aaaaaaa"
    );
    assert_eq!(text(&response), expected);
    session.child.kill().await.expect("kill");
}

#[tokio::test]
async fn notifies_pipeline_finished_behind_newer_one() {
    let running = pipeline(6, "running", RELEASE_COMMIT);
    let first = pipelines(&[running]);
    let bot = pipeline(7, "running", BOT_COMMIT);
    let release = pipeline(6, "success", RELEASE_COMMIT);
    let later = pipelines(&[bot, release]);
    let server = woodpecker_mock(&first, &later).await;
    let mut session = spawn(&server);
    initialize(&mut session).await;
    let arguments = r#"{"repo": "jruz/demo", "branch": "main"}"#;
    let response = call_tool(&mut session, "watch", arguments).await;
    assert!(text(&response).ends_with("#6 push main running aaaaaaa"));
    let notification = receive(&mut session).await;
    assert_eq!(at(&notification, "/method"), "notifications/claude/channel");
    assert_eq!(at(&notification, "/params/meta/pipeline"), "6");
    assert_eq!(at(&notification, "/params/meta/status"), "success");
    assert_eq!(at(&notification, "/params/meta/event"), "push");
    session.child.kill().await.expect("kill");
}

#[tokio::test]
async fn notifies_failure_with_logs() {
    let running = pipeline(3, "running", RELEASE_COMMIT);
    let first = pipelines(&[running]);
    let failed = pipeline(3, "failure", RELEASE_COMMIT);
    let later = pipelines(&[failed]);
    let server = woodpecker_mock(&first, &later).await;
    let mut session = spawn(&server);
    initialize(&mut session).await;
    let arguments = commit_arguments(RELEASE_COMMIT);
    call_tool(&mut session, "watch", &arguments).await;
    let notification = receive(&mut session).await;
    assert_eq!(at(&notification, "/params/meta/status"), "failure");
    let content = at(&notification, "/params/content")
        .as_str()
        .expect("content");
    assert!(content.starts_with("Pipeline #3 FAILED"));
    assert!(content.ends_with("cargo test\ntest failed"));
    session.child.kill().await.expect("kill");
}

#[tokio::test]
async fn status_lists_watches() {
    let server = idle_mock().await;
    let mut session = spawn(&server);
    initialize(&mut session).await;
    let arguments = commit_arguments(RELEASE_COMMIT);
    call_tool(&mut session, "watch", &arguments).await;
    let response = call_tool(&mut session, "status", "{}").await;
    let expected = format!("jruz/demo/commit:{RELEASE_COMMIT}\n#3 push main success aaaaaaa");
    assert_eq!(text(&response), expected);
    session.child.kill().await.expect("kill");
}

#[tokio::test]
async fn unwatch_stops_watch() {
    let server = idle_mock().await;
    let mut session = spawn(&server);
    initialize(&mut session).await;
    let arguments = commit_arguments(RELEASE_COMMIT);
    call_tool(&mut session, "watch", &arguments).await;
    let stopped = call_tool(&mut session, "unwatch", &arguments).await;
    let expected = format!("stopped watching jruz/demo/commit:{RELEASE_COMMIT}");
    assert_eq!(text(&stopped), expected);
    let status = call_tool(&mut session, "status", "{}").await;
    assert_eq!(text(&status), "no active watches");
    session.child.kill().await.expect("kill");
}

#[tokio::test]
async fn rejects_short_commit() {
    let server = idle_mock().await;
    let mut session = spawn(&server);
    initialize(&mut session).await;
    let arguments = commit_arguments("abc123");
    let response = call_tool(&mut session, "watch", &arguments).await;
    assert_eq!(at(&response, "/result/isError"), true);
    assert_eq!(
        text(&response),
        "commit must be a full lowercase 40-character SHA, got abc123"
    );
    session.child.kill().await.expect("kill");
}

#[tokio::test]
async fn rejects_branch_and_commit_together() {
    let server = idle_mock().await;
    let mut session = spawn(&server);
    initialize(&mut session).await;
    let arguments =
        format!(r#"{{"repo": "jruz/demo", "branch": "main", "commit": "{RELEASE_COMMIT}"}}"#);
    let response = call_tool(&mut session, "watch", &arguments).await;
    assert_eq!(text(&response), "provide either branch or commit, not both");
    session.child.kill().await.expect("kill");
}

#[test]
fn fails_without_server_url() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_woodpecker-channel"))
        .arg("serve")
        .env_remove("WOODPECKER_SERVER")
        .output()
        .expect("run CLI");
    assert!(!output.status.success());
    assert_eq!(
        String::from_utf8(output.stderr).expect("UTF-8 output"),
        "error: WOODPECKER_SERVER is not set: environment variable not found\n"
    );
}
