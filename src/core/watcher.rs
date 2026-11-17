use crate::core::event;
use crate::core::poll;
use crate::libs::woodpecker::types::Config;

pub const POLL_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5);

async fn run(http: reqwest::Client, config: Config, watch: poll::Watch, events: event::Sender) {
    let mut interval = tokio::time::interval(POLL_INTERVAL);
    let mut last_seen = None;
    loop {
        interval.tick().await;
        match poll::poll(&http, &config, &watch, last_seen).await {
            Ok(outcome) => {
                last_seen = outcome.last_seen;
                let Some(notification) = outcome.notification else {
                    continue;
                };
                let next = event::Event::Notification(notification);
                if let Err(closed) = events.send(next) {
                    tracing::info!(repo = %watch.repo, %closed, "server stopped");
                    return;
                }
            }
            Err(error) => tracing::error!(repo = %watch.repo, %error, "poll failed"),
        }
    }
}

#[must_use]
pub fn spawn(
    http: reqwest::Client,
    config: Config,
    watch: poll::Watch,
    events: event::Sender,
) -> tokio::task::JoinHandle<()> {
    let watching = run(http, config, watch, events);
    tokio::spawn(watching)
}
