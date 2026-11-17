use std::collections::btree_map;

use crate::core::event;
use crate::core::notification;
use crate::core::report;
use crate::core::snapshot;
use crate::core::tools;
use crate::core::watcher;
use crate::libs::mcp::jsonrpc;
use crate::libs::woodpecker::client;
use crate::libs::woodpecker::types::{Config, Pipeline};

pub struct Context {
    pub config: Config,
    pub events: event::Sender,
    pub http: reqwest::Client,
}

enum Phase {
    Pending(serde_json::Value),
    Watching(Vec<Pipeline>),
}

pub struct Entry {
    handle: tokio::task::JoinHandle<()>,
    phase: Phase,
    repo: String,
}

pub type Watches = btree_map::BTreeMap<String, Entry>;

#[must_use]
pub fn reply(id: &serde_json::Value, text: &str, is_error: bool) -> serde_json::Value {
    let content = jsonrpc::tool_text(text, is_error);
    jsonrpc::result(id, &content)
}

fn describe(phase: &Phase) -> String {
    match phase {
        Phase::Pending(_) => "awaiting first poll".to_owned(),
        Phase::Watching(pipelines) => snapshot::summary(pipelines),
    }
}

pub fn start(
    context: &Context,
    watches: &mut Watches,
    id: &serde_json::Value,
    watch: tools::Watch,
) -> Option<serde_json::Value> {
    let key = tools::key(&watch);
    match watches.entry(key) {
        btree_map::Entry::Occupied(entry) => {
            let current = describe(&entry.get().phase);
            let text = format!("already watching {}\n{current}", entry.key());
            let response = reply(id, &text, false);
            Some(response)
        }
        btree_map::Entry::Vacant(entry) => {
            tracing::info!(key = %entry.key(), "watch start");
            let repo = watch.repo.clone();
            let http = context.http.clone();
            let config = context.config.clone();
            let events = context.events.clone();
            let watch_key = entry.key().clone();
            let handle = watcher::spawn(http, config, watch, watch_key, events);
            let request = id.clone();
            let pending = Phase::Pending(request);
            entry.insert(Entry {
                handle,
                phase: pending,
                repo,
            });
            None
        }
    }
}

#[must_use]
pub fn stop(watches: &mut Watches, watch: &tools::Watch) -> String {
    let key = tools::key(watch);
    let Some(entry) = watches.remove(&key) else {
        return format!("not watching {key}");
    };
    entry.handle.abort();
    tracing::info!(%key, "watch stop");
    format!("stopped watching {key}")
}

#[must_use]
pub fn status(watches: &Watches) -> String {
    if watches.is_empty() {
        return "no active watches".to_owned();
    }
    watches
        .iter()
        .map(|(key, entry)| format!("{key}\n{}", describe(&entry.phase)))
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn report_finished(
    context: &Context,
    repo: &str,
    finished: Vec<Pipeline>,
) -> Vec<serde_json::Value> {
    let mut messages = Vec::new();
    for pipeline in finished {
        if snapshot::is_failure(&pipeline) {
            let http = context.http.clone();
            let config = context.config.clone();
            let events = context.events.clone();
            let owner = repo.to_owned();
            report::spawn_failure(http, config, owner, pipeline, events);
            continue;
        }
        let done = notification::finished(repo, &pipeline, &[]);
        let message = notification::message(done);
        messages.push(message);
    }
    messages
}

fn transition(
    context: &Context,
    key: &str,
    entry: Entry,
    result: Result<Vec<Pipeline>, client::Error>,
) -> (Option<Entry>, Vec<serde_json::Value>) {
    match (entry.phase, result) {
        (Phase::Pending(id), Ok(pipelines)) => {
            let current = snapshot::summary(&pipelines);
            let text = format!("watching {key} \u{2014} polling every 5s\n{current}");
            let response = reply(&id, &text, false);
            let watching = Entry {
                handle: entry.handle,
                phase: Phase::Watching(pipelines),
                repo: entry.repo,
            };
            (Some(watching), vec![response])
        }
        (Phase::Pending(id), Err(error)) => {
            entry.handle.abort();
            tracing::error!(%key, %error, "watch failed");
            let text = format!("failed to watch {key}: {error}");
            let response = reply(&id, &text, true);
            (None, vec![response])
        }
        (Phase::Watching(previous), Ok(pipelines)) => {
            let finished = snapshot::finished_since(&previous, &pipelines);
            let messages = report_finished(context, &entry.repo, finished);
            let watching = Entry {
                handle: entry.handle,
                phase: Phase::Watching(pipelines),
                repo: entry.repo,
            };
            (Some(watching), messages)
        }
        (Phase::Watching(previous), Err(error)) => {
            tracing::error!(%key, %error, "poll failed");
            let unchanged = Entry {
                handle: entry.handle,
                phase: Phase::Watching(previous),
                repo: entry.repo,
            };
            (Some(unchanged), Vec::new())
        }
    }
}

pub fn polled(
    context: &Context,
    watches: &mut Watches,
    key: String,
    result: Result<Vec<Pipeline>, client::Error>,
) -> Vec<serde_json::Value> {
    let Some(entry) = watches.remove(&key) else {
        return Vec::new();
    };
    let (next, messages) = transition(context, &key, entry, result);
    if let Some(kept) = next {
        watches.insert(key, kept);
    }
    messages
}
