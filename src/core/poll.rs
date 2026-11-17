use crate::core::notification;
use crate::libs::woodpecker::client;
use crate::libs::woodpecker::types::{Config, Pipeline, WatchTarget};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Woodpecker(#[from] client::Error),
}

#[derive(Debug, Clone)]
pub struct Watch {
    pub repo: String,
    pub target: WatchTarget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outcome {
    pub last_seen: Option<u64>,
    pub notification: Option<notification::Notification>,
}

fn is_failure(pipeline: &Pipeline) -> bool {
    pipeline.status == "failure" || pipeline.status == "error"
}

fn is_finished(pipeline: &Pipeline) -> bool {
    pipeline.status == "success" || is_failure(pipeline)
}

fn is_new(last_seen: Option<u64>, pipeline: &Pipeline) -> bool {
    match last_seen {
        Some(seen) => pipeline.number > seen,
        None => true,
    }
}

#[must_use]
pub fn finished_unseen(last_seen: Option<u64>, latest: Option<Pipeline>) -> Option<Pipeline> {
    latest.filter(|pipeline| is_new(last_seen, pipeline) && is_finished(pipeline))
}

async fn failed_logs(
    http: &reqwest::Client,
    config: &Config,
    repo_id: u64,
    pipeline_number: u64,
) -> Result<Vec<String>, client::Error> {
    let step_id = client::failed_step_id(http, config, repo_id, pipeline_number).await?;
    client::step_logs(http, config, repo_id, pipeline_number, step_id).await
}

async fn describe(
    http: &reqwest::Client,
    config: &Config,
    watch: &Watch,
    repo_id: u64,
    pipeline: &Pipeline,
) -> notification::Notification {
    if !is_failure(pipeline) {
        return notification::success(&watch.repo, pipeline);
    }
    match failed_logs(http, config, repo_id, pipeline.number).await {
        Ok(logs) => notification::failure(&watch.repo, pipeline, &logs),
        Err(error) => {
            tracing::error!(repo = %watch.repo, pipeline = pipeline.number, %error, "logs unavailable");
            let reason = vec![format!("(logs unavailable: {error})")];
            notification::failure(&watch.repo, pipeline, &reason)
        }
    }
}

pub async fn poll(
    http: &reqwest::Client,
    config: &Config,
    watch: &Watch,
    last_seen: Option<u64>,
) -> Result<Outcome, Error> {
    let repo_id = client::repo_id(http, config, &watch.repo).await?;
    let latest = client::latest_pipeline(http, config, repo_id, &watch.target).await?;
    let Some(pipeline) = finished_unseen(last_seen, latest) else {
        return Ok(Outcome {
            last_seen,
            notification: None,
        });
    };
    let notification = describe(http, config, watch, repo_id, &pipeline).await;
    Ok(Outcome {
        last_seen: Some(pipeline.number),
        notification: Some(notification),
    })
}
