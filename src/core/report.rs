use crate::core::event;
use crate::core::notification;
use crate::libs::woodpecker::client;
use crate::libs::woodpecker::types::{Config, Pipeline};

async fn failure_logs(
    http: &reqwest::Client,
    config: &Config,
    repo: &str,
    pipeline_number: u64,
) -> Result<Vec<String>, client::Error> {
    let repo_id = client::repo_id(http, config, repo).await?;
    let step_id = client::failed_step_id(http, config, repo_id, pipeline_number).await?;
    client::step_logs(http, config, repo_id, pipeline_number, step_id).await
}

async fn describe_failure(
    http: reqwest::Client,
    config: Config,
    repo: String,
    pipeline: Pipeline,
    events: event::Sender,
) {
    let logs = match failure_logs(&http, &config, &repo, pipeline.number).await {
        Ok(logs) => logs,
        Err(error) => {
            tracing::error!(%repo, pipeline = pipeline.number, %error, "logs unavailable");
            let reason = format!("(logs unavailable: {error})");
            vec![reason]
        }
    };
    let finished = notification::finished(&repo, &pipeline, &logs);
    let next = event::Event::Notification(finished);
    if let Err(closed) = events.send(next) {
        tracing::info!(%closed, "server stopped");
    }
}

pub fn spawn_failure(
    http: reqwest::Client,
    config: Config,
    repo: String,
    pipeline: Pipeline,
    events: event::Sender,
) {
    let describing = describe_failure(http, config, repo, pipeline, events);
    tokio::spawn(describing);
}
