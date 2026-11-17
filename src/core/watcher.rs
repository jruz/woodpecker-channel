use crate::core::event;
use crate::core::tools::Watch;
use crate::libs::woodpecker::client;
use crate::libs::woodpecker::types::{Config, Pipeline};

pub const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);

async fn fetch(
    http: &reqwest::Client,
    config: &Config,
    watch: &Watch,
) -> Result<Vec<Pipeline>, client::Error> {
    let repo_id = client::repo_id(http, config, &watch.repo).await?;
    client::recent_pipelines(http, config, repo_id, &watch.target).await
}

async fn run(
    http: reqwest::Client,
    config: Config,
    watch: Watch,
    key: String,
    events: event::Sender,
) {
    let mut interval = tokio::time::interval(POLL_INTERVAL);
    loop {
        interval.tick().await;
        let result = fetch(&http, &config, &watch).await;
        let next = event::Event::Polled {
            key: key.clone(),
            result,
        };
        if let Err(closed) = events.send(next) {
            tracing::info!(%key, %closed, "server stopped");
            return;
        }
    }
}

#[must_use]
pub fn spawn(
    http: reqwest::Client,
    config: Config,
    watch: Watch,
    key: String,
    events: event::Sender,
) -> tokio::task::JoinHandle<()> {
    let watching = run(http, config, watch, key, events);
    tokio::spawn(watching)
}
