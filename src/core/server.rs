use std::collections::hash_map;

use tokio::io::AsyncBufReadExt as _;
use tokio::io::AsyncWriteExt as _;

use crate::core::event;
use crate::core::notification::Notification;
use crate::core::poll;
use crate::core::tools;
use crate::core::watcher;
use crate::libs::mcp::jsonrpc;
use crate::libs::woodpecker::client;
use crate::libs::woodpecker::types::Config;

const CHANNEL_METHOD: &str = "notifications/claude/channel";

const INSTRUCTIONS: &str = "CI pipeline events arrive as <channel source=\"woodpecker-channel\" repo=\"...\" branch=\"...\" pipeline=\"...\" status=\"failure|success\">.
Failure notifications contain pipeline details and logs. Success notifications confirm the pipeline passed.
Use the watch tool after pushing to CI. Use stop_watch to cancel.
watch accepts either branch (for push pipelines) or event (e.g. 'tag') \u{2014} not both.";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Client(#[from] client::Error),

    #[error("failed to encode message")]
    Encode(#[from] serde_json::Error),

    #[error("failed to read stdin: {reason}")]
    Input { reason: std::io::Error },

    #[error("failed to write stdout")]
    Output(#[from] std::io::Error),
}

struct Context {
    config: Config,
    events: event::Sender,
    http: reqwest::Client,
}

type Watchers = hash_map::HashMap<String, tokio::task::JoinHandle<()>>;

fn server_info(version: &str) -> serde_json::Value {
    serde_json::json!({
        "protocolVersion": version,
        "capabilities": {
            "tools": {},
            "experimental": {"claude/channel": {}},
        },
        "serverInfo": {
            "name": env!("CARGO_PKG_NAME"),
            "version": env!("CARGO_PKG_VERSION"),
        },
        "instructions": INSTRUCTIONS,
    })
}

fn initialize(id: &serde_json::Value, params: Option<&serde_json::Value>) -> serde_json::Value {
    let requested = params
        .and_then(|value| value.get("protocolVersion"))
        .and_then(serde_json::Value::as_str);
    let Some(version) = requested else {
        return jsonrpc::error(id, jsonrpc::INVALID_PARAMS, "missing protocolVersion");
    };
    let info = server_info(version);
    jsonrpc::result(id, &info)
}

fn start_watch(context: &Context, watchers: &mut Watchers, watch: poll::Watch) -> String {
    let key = tools::key(&watch);
    match watchers.entry(key) {
        hash_map::Entry::Occupied(entry) => format!("already watching {}", entry.key()),
        hash_map::Entry::Vacant(entry) => {
            let message = format!("watching {} \u{2014} polling every 5s", entry.key());
            tracing::info!(key = %entry.key(), "watch start");
            let http = context.http.clone();
            let config = context.config.clone();
            let events = context.events.clone();
            let handle = watcher::spawn(http, config, watch, events);
            entry.insert(handle);
            message
        }
    }
}

fn stop_watch(watchers: &mut Watchers, watch: &poll::Watch) -> String {
    let key = tools::key(watch);
    let Some(handle) = watchers.remove(&key) else {
        return format!("not watching {key}");
    };
    handle.abort();
    tracing::info!(%key, "watch stop");
    format!("stopped watching {key}")
}

fn run_tool(context: &Context, watchers: &mut Watchers, call: tools::Call) -> String {
    match call.tool {
        tools::Tool::Watch => start_watch(context, watchers, call.watch),
        tools::Tool::StopWatch => stop_watch(watchers, &call.watch),
    }
}

fn call_tool(
    context: &Context,
    watchers: &mut Watchers,
    id: &serde_json::Value,
    params: Option<serde_json::Value>,
) -> serde_json::Value {
    let Some(params) = params else {
        return jsonrpc::error(id, jsonrpc::INVALID_PARAMS, "missing params");
    };
    let tool_result = match tools::parse_call(params) {
        Ok(call) => {
            let text = run_tool(context, watchers, call);
            jsonrpc::tool_text(&text, false)
        }
        Err(error) => {
            let text = error.to_string();
            jsonrpc::tool_text(&text, true)
        }
    };
    jsonrpc::result(id, &tool_result)
}

fn list_tools(id: &serde_json::Value) -> serde_json::Value {
    let tools = tools::definitions();
    let listing = serde_json::json!({"tools": tools});
    jsonrpc::result(id, &listing)
}

fn unknown_method(id: &serde_json::Value, method: &str) -> serde_json::Value {
    let message = format!("unknown method {method}");
    jsonrpc::error(id, jsonrpc::METHOD_NOT_FOUND, &message)
}

fn handle_request(
    context: &Context,
    watchers: &mut Watchers,
    id: &serde_json::Value,
    method: &str,
    params: Option<serde_json::Value>,
) -> serde_json::Value {
    let empty = serde_json::json!({});
    let borrowed = params.as_ref();
    match method {
        "initialize" => initialize(id, borrowed),
        "ping" => jsonrpc::result(id, &empty),
        "tools/list" => list_tools(id),
        "tools/call" => call_tool(context, watchers, id, params),
        _ => unknown_method(id, method),
    }
}

fn parse_failure(error: &jsonrpc::Error) -> serde_json::Value {
    tracing::error!(%error, "invalid message");
    let message = error.to_string();
    jsonrpc::error(&serde_json::Value::Null, jsonrpc::PARSE_ERROR, &message)
}

fn handle_line(
    context: &Context,
    watchers: &mut Watchers,
    line: &str,
) -> Option<serde_json::Value> {
    match jsonrpc::parse(line) {
        Ok(jsonrpc::Incoming::Request { id, method, params }) => {
            let response = handle_request(context, watchers, &id, &method, params);
            Some(response)
        }
        Ok(jsonrpc::Incoming::Notification { method }) => {
            tracing::debug!(%method, "client notification");
            None
        }
        Ok(jsonrpc::Incoming::Response) => None,
        Err(error) => {
            let response = parse_failure(&error);
            Some(response)
        }
    }
}

fn channel_message(notification: Notification) -> serde_json::Value {
    tracing::info!(meta = ?notification.meta, "notify");
    let meta: serde_json::Map<String, serde_json::Value> = notification
        .meta
        .into_iter()
        .map(|(key, value)| (key, serde_json::Value::String(value)))
        .collect();
    let params = serde_json::json!({"content": notification.content, "meta": meta});
    jsonrpc::notification(CHANNEL_METHOD, &params)
}

async fn read_input(events: event::Sender) {
    let stdin = tokio::io::stdin();
    let mut lines = tokio::io::BufReader::new(stdin).lines();
    loop {
        let next = match lines.next_line().await {
            Ok(Some(line)) => event::Event::Line(line),
            Ok(None) => event::Event::InputClosed,
            Err(error) => event::Event::InputFailed(error),
        };
        let last = !matches!(next, event::Event::Line(_));
        if let Err(closed) = events.send(next) {
            tracing::info!(%closed, "server stopped");
            return;
        }
        if last {
            return;
        }
    }
}

async fn write(stdout: &mut tokio::io::Stdout, message: &serde_json::Value) -> Result<(), Error> {
    let mut encoded = serde_json::to_vec(message)?;
    encoded.push(b'\n');
    stdout.write_all(&encoded).await?;
    stdout.flush().await?;
    Ok(())
}

pub async fn run(server: String, token: String) -> Result<(), Error> {
    let (events, mut incoming) = tokio::sync::mpsc::unbounded_channel();
    let context = Context {
        config: Config { server, token },
        events: events.clone(),
        http: client::new()?,
    };
    let reading = read_input(events);
    tokio::spawn(reading);
    let mut watchers = Watchers::new();
    let mut stdout = tokio::io::stdout();
    tracing::info!("server connected");
    while let Some(next) = incoming.recv().await {
        let outgoing = match next {
            event::Event::Line(line) => handle_line(&context, &mut watchers, &line),
            event::Event::Notification(notification) => {
                let message = channel_message(notification);
                Some(message)
            }
            event::Event::InputFailed(reason) => return Err(Error::Input { reason }),
            event::Event::InputClosed => break,
        };
        if let Some(message) = outgoing {
            write(&mut stdout, &message).await?;
        }
    }
    tracing::info!("input closed");
    Ok(())
}
