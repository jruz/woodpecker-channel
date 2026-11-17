use crate::libs::mcp::jsonrpc;
use crate::libs::woodpecker::types::Pipeline;

const CHANNEL_METHOD: &str = "notifications/claude/channel";
const MAX_LOG_LINES: usize = 80;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notification {
    pub content: String,
    pub meta: Vec<(String, String)>,
}

fn meta(repo: &str, pipeline: &Pipeline) -> Vec<(String, String)> {
    vec![
        ("repo".to_owned(), repo.to_owned()),
        ("branch".to_owned(), pipeline.branch.clone()),
        ("event".to_owned(), pipeline.event.clone()),
        ("pipeline".to_owned(), pipeline.number.to_string()),
        ("status".to_owned(), pipeline.status.clone()),
        ("commit".to_owned(), pipeline.commit.clone()),
    ]
}

fn headline(pipeline: &Pipeline) -> String {
    match pipeline.status.as_str() {
        "success" => "PASSED".to_owned(),
        "failure" | "error" => "FAILED".to_owned(),
        other => other.to_uppercase(),
    }
}

fn summary(pipeline: &Pipeline) -> String {
    format!(
        "Pipeline #{} {}\nEvent: {}\nBranch: {}\nCommit: {}\nAuthor: {}\nMessage: {}",
        pipeline.number,
        headline(pipeline),
        pipeline.event,
        pipeline.branch,
        pipeline.commit,
        pipeline.author,
        pipeline.message,
    )
}

fn tail(lines: &[String]) -> String {
    let skip = lines.len().saturating_sub(MAX_LOG_LINES);
    lines
        .iter()
        .skip(skip)
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join("\n")
}

#[must_use]
pub fn finished(repo: &str, pipeline: &Pipeline, logs: &[String]) -> Notification {
    let overview = summary(pipeline);
    let content = if logs.is_empty() {
        overview
    } else {
        format!("{overview}\n\n--- Logs (last lines) ---\n{}", tail(logs))
    };
    Notification {
        content,
        meta: meta(repo, pipeline),
    }
}

#[must_use]
pub fn message(notification: Notification) -> serde_json::Value {
    tracing::info!(meta = ?notification.meta, "notify");
    let meta: serde_json::Map<String, serde_json::Value> = notification
        .meta
        .into_iter()
        .map(|(key, value)| (key, serde_json::Value::String(value)))
        .collect();
    let params = serde_json::json!({"content": notification.content, "meta": meta});
    jsonrpc::notification(CHANNEL_METHOD, &params)
}
