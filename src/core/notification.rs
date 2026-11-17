use crate::libs::woodpecker::types::Pipeline;

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
        ("pipeline".to_owned(), pipeline.number.to_string()),
        ("status".to_owned(), pipeline.status.clone()),
        ("commit".to_owned(), pipeline.commit.clone()),
    ]
}

fn summary(headline: &str, pipeline: &Pipeline) -> String {
    format!(
        "Pipeline #{} {headline}\nBranch: {}\nCommit: {}\nAuthor: {}\nMessage: {}",
        pipeline.number, pipeline.branch, pipeline.commit, pipeline.author, pipeline.message,
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
pub fn success(repo: &str, pipeline: &Pipeline) -> Notification {
    Notification {
        content: summary("PASSED", pipeline),
        meta: meta(repo, pipeline),
    }
}

#[must_use]
pub fn failure(repo: &str, pipeline: &Pipeline, logs: &[String]) -> Notification {
    Notification {
        content: format!(
            "{}\n\n--- Logs (last lines) ---\n{}",
            summary("FAILED", pipeline),
            tail(logs)
        ),
        meta: meta(repo, pipeline),
    }
}
