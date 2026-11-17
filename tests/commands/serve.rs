use crate::utils::stdio::{
    at, call_tool, failing_pipeline_mock, initialize, json, receive, send, spawn,
};

#[tokio::test]
async fn declares_channel_capability() {
    let server = failing_pipeline_mock().await;
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
async fn lists_watch_tools() {
    let server = failing_pipeline_mock().await;
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
    assert_eq!(names, vec!["watch", "stop_watch"]);
    session.child.kill().await.expect("kill");
}

#[tokio::test]
async fn rejects_branch_and_event_together() {
    let server = failing_pipeline_mock().await;
    let mut session = spawn(&server);
    initialize(&mut session).await;
    let arguments = r#"{"repo": "jruz/demo", "branch": "main", "event": "tag"}"#;
    let response = call_tool(&mut session, "watch", arguments).await;
    assert_eq!(at(&response, "/result/isError"), true);
    assert_eq!(
        at(&response, "/result/content/0/text"),
        "provide either branch or event, not both"
    );
    session.child.kill().await.expect("kill");
}

#[tokio::test]
async fn notifies_failed_pipeline_with_logs() {
    let server = failing_pipeline_mock().await;
    let mut session = spawn(&server);
    initialize(&mut session).await;
    let arguments = r#"{"repo": "jruz/demo", "branch": "main"}"#;
    let response = call_tool(&mut session, "watch", arguments).await;
    assert_eq!(
        at(&response, "/result/content/0/text"),
        "watching jruz/demo/branch:main \u{2014} polling every 5s"
    );
    let notification = receive(&mut session).await;
    assert_eq!(at(&notification, "/method"), "notifications/claude/channel");
    assert_eq!(at(&notification, "/params/meta/status"), "failure");
    assert_eq!(at(&notification, "/params/meta/pipeline"), "3");
    let content = at(&notification, "/params/content")
        .as_str()
        .expect("content");
    assert!(content.starts_with("Pipeline #3 FAILED"));
    assert!(content.ends_with("cargo test\ntest failed"));
    session.child.kill().await.expect("kill");
}

#[tokio::test]
async fn stops_watching() {
    let server = failing_pipeline_mock().await;
    let mut session = spawn(&server);
    initialize(&mut session).await;
    let arguments = r#"{"repo": "jruz/demo", "event": "tag"}"#;
    let response = call_tool(&mut session, "stop_watch", arguments).await;
    assert_eq!(
        at(&response, "/result/content/0/text"),
        "not watching jruz/demo/event:tag"
    );
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
