use tokio::io::AsyncBufReadExt as _;
use tokio::io::AsyncWriteExt as _;

use crate::core::event;
use crate::core::notification;
use crate::core::tools;
use crate::core::watches;
use crate::libs::mcp::jsonrpc;
use crate::libs::woodpecker::client;
use crate::libs::woodpecker::types::Config;

const INSTRUCTIONS: &str = "CI pipeline results arrive as <channel source=\"woodpecker-channel\" repo=\"...\" branch=\"...\" event=\"...\" pipeline=\"...\" status=\"...\" commit=\"...\">.
After pushing, call watch with the repo and the full commit SHA from `git rev-parse HEAD`; this covers push and tag pipelines for that commit. Use branch only to follow every pipeline on a branch.
The watch response lists current pipelines. Each pipeline that later finishes arrives as one channel message; failures include the failed step's logs.
Push success is not release success: keep the commit watch until the release pipeline you expect has finished, then call unwatch. Use status to list active watches.";

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

enum Step {
    Continue(Vec<serde_json::Value>),
    Stop,
}

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

fn call_tool(
    context: &watches::Context,
    active: &mut watches::Watches,
    id: &serde_json::Value,
    params: Option<serde_json::Value>,
) -> Option<serde_json::Value> {
    let Some(params) = params else {
        let missing = jsonrpc::error(id, jsonrpc::INVALID_PARAMS, "missing params");
        return Some(missing);
    };
    let text = match tools::parse_call(params) {
        Ok(tools::Call::Watch(watch)) => return watches::start(context, active, id, watch),
        Ok(tools::Call::Unwatch(watch)) => watches::stop(active, &watch),
        Ok(tools::Call::Status) => watches::status(active),
        Err(error) => {
            let message = error.to_string();
            let response = watches::reply(id, &message, true);
            return Some(response);
        }
    };
    let response = watches::reply(id, &text, false);
    Some(response)
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
    context: &watches::Context,
    active: &mut watches::Watches,
    id: &serde_json::Value,
    method: &str,
    params: Option<serde_json::Value>,
) -> Option<serde_json::Value> {
    let empty = serde_json::json!({});
    let borrowed = params.as_ref();
    let response = match method {
        "initialize" => initialize(id, borrowed),
        "ping" => jsonrpc::result(id, &empty),
        "tools/list" => list_tools(id),
        "tools/call" => return call_tool(context, active, id, params),
        _ => unknown_method(id, method),
    };
    Some(response)
}

fn parse_failure(error: &jsonrpc::Error) -> serde_json::Value {
    tracing::error!(%error, "invalid message");
    let message = error.to_string();
    jsonrpc::error(&serde_json::Value::Null, jsonrpc::PARSE_ERROR, &message)
}

fn handle_line(
    context: &watches::Context,
    active: &mut watches::Watches,
    line: &str,
) -> Option<serde_json::Value> {
    match jsonrpc::parse(line) {
        Ok(jsonrpc::Incoming::Request { id, method, params }) => {
            handle_request(context, active, &id, &method, params)
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

fn handle_event(
    context: &watches::Context,
    active: &mut watches::Watches,
    next: event::Event,
) -> Result<Step, Error> {
    let outgoing = match next {
        event::Event::Line(line) => {
            let response = handle_line(context, active, &line);
            response.into_iter().collect()
        }
        event::Event::Notification(finished) => {
            let message = notification::message(finished);
            vec![message]
        }
        event::Event::Polled { key, result } => watches::polled(context, active, key, result),
        event::Event::InputFailed(reason) => return Err(Error::Input { reason }),
        event::Event::InputClosed => return Ok(Step::Stop),
    };
    let step = Step::Continue(outgoing);
    Ok(step)
}

pub async fn run(server: String, token: String) -> Result<(), Error> {
    let (events, mut incoming) = tokio::sync::mpsc::unbounded_channel();
    let context = watches::Context {
        config: Config { server, token },
        events: events.clone(),
        http: client::new()?,
    };
    let reading = read_input(events);
    tokio::spawn(reading);
    let mut active = watches::Watches::new();
    let mut stdout = tokio::io::stdout();
    tracing::info!("server connected");
    while let Some(next) = incoming.recv().await {
        let step = handle_event(&context, &mut active, next)?;
        let Step::Continue(outgoing) = step else {
            break;
        };
        for message in outgoing {
            write(&mut stdout, &message).await?;
        }
    }
    tracing::info!("input closed");
    Ok(())
}
