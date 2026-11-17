use crate::core::poll::Watch;
use crate::libs::woodpecker::types::WatchTarget;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid arguments")]
    Arguments(#[from] serde_json::Error),

    #[error("provide either branch or event, not both")]
    BothTargets,

    #[error("provide either branch or event")]
    NoTarget,

    #[error("unknown tool {name}")]
    UnknownTool { name: String },
}

#[derive(Debug, Clone, Copy)]
pub enum Tool {
    StopWatch,
    Watch,
}

#[derive(Debug, Clone)]
pub struct Call {
    pub tool: Tool,
    pub watch: Watch,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct Arguments {
    branch: Option<String>,
    event: Option<String>,
    repo: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct CallParams {
    arguments: serde_json::Value,
    name: String,
}

fn target_schema(action: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "repo": {"type": "string", "description": "Repository full name (e.g. owner/repo)"},
            "branch": {"type": "string", "description": format!("Branch name to {action}")},
            "event": {"type": "string", "description": format!("Event type to {action} (e.g. tag, push, pull_request)")},
        },
        "required": ["repo"],
    })
}

#[must_use]
pub fn definitions() -> serde_json::Value {
    serde_json::json!([
        {
            "name": "watch",
            "description": "Watch a Woodpecker CI repo for pipeline results. Polls every 5s and notifies on success or failure. Provide either branch or event, not both.",
            "inputSchema": target_schema("watch"),
        },
        {
            "name": "stop_watch",
            "description": "Stop watching a repo for CI results. Provide the same branch or event used in watch.",
            "inputSchema": target_schema("stop watching"),
        },
    ])
}

fn target(arguments: Arguments) -> Result<Watch, Error> {
    let target = match (arguments.branch, arguments.event) {
        (Some(_), Some(_)) => return Err(Error::BothTargets),
        (None, None) => return Err(Error::NoTarget),
        (Some(branch), None) => WatchTarget::Branch(branch),
        (None, Some(event)) => WatchTarget::Event(event),
    };
    Ok(Watch {
        repo: arguments.repo,
        target,
    })
}

#[must_use]
pub fn key(watch: &Watch) -> String {
    match &watch.target {
        WatchTarget::Branch(branch) => format!("{}/branch:{branch}", watch.repo),
        WatchTarget::Event(event) => format!("{}/event:{event}", watch.repo),
    }
}

fn watch(arguments: serde_json::Value) -> Result<Watch, Error> {
    let parsed: Arguments = serde_json::from_value(arguments)?;
    target(parsed)
}

fn tool(name: String) -> Result<Tool, Error> {
    match name.as_str() {
        "watch" => Ok(Tool::Watch),
        "stop_watch" => Ok(Tool::StopWatch),
        _ => Err(Error::UnknownTool { name }),
    }
}

pub fn parse_call(params: serde_json::Value) -> Result<Call, Error> {
    let call: CallParams = serde_json::from_value(params)?;
    let requested = tool(call.name)?;
    let parsed = watch(call.arguments)?;
    Ok(Call {
        tool: requested,
        watch: parsed,
    })
}
