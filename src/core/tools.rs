use crate::libs::woodpecker::types::WatchTarget;

const COMMIT_LENGTH: usize = 40;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("invalid arguments: {reason}")]
    Arguments { reason: serde_json::Error },

    #[error("provide either branch or commit, not both")]
    BothTargets,

    #[error("commit must be a full lowercase 40-character SHA, got {commit}")]
    InvalidCommit { commit: String },

    #[error("missing arguments")]
    MissingArguments,

    #[error("provide either branch or commit")]
    NoTarget,

    #[error("unknown tool {name}")]
    UnknownTool { name: String },
}

#[derive(Debug, Clone)]
pub struct Watch {
    pub repo: String,
    pub target: WatchTarget,
}

#[derive(Debug, Clone)]
pub enum Call {
    Status,
    Unwatch(Watch),
    Watch(Watch),
}

#[derive(Debug, Clone, serde::Deserialize)]
struct Arguments {
    branch: Option<String>,
    commit: Option<String>,
    repo: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
struct CallParams {
    arguments: Option<serde_json::Value>,
    name: String,
}

fn target_schema(action: &str) -> serde_json::Value {
    serde_json::json!({
        "type": "object",
        "properties": {
            "repo": {"type": "string", "description": "Repository full name (e.g. owner/repo)"},
            "commit": {"type": "string", "description": format!("Full lowercase 40-character commit SHA to {action}")},
            "branch": {"type": "string", "description": format!("Exact branch name to {action}")},
        },
        "required": ["repo"],
    })
}

#[must_use]
pub fn definitions() -> serde_json::Value {
    serde_json::json!([
        {
            "name": "watch",
            "description": "Watch Woodpecker CI pipelines for a commit or a branch. Returns the current pipelines, then polls every 5s and notifies when a pipeline finishes. Provide exactly one of commit or branch; prefer commit after pushing.",
            "inputSchema": target_schema("watch"),
        },
        {
            "name": "unwatch",
            "description": "Stop a watch. Provide the same repo and commit or branch used in watch.",
            "inputSchema": target_schema("stop watching"),
        },
        {
            "name": "status",
            "description": "List active watches with their latest pipelines.",
            "inputSchema": {"type": "object", "properties": {}},
        },
    ])
}

fn is_full_sha(commit: &str) -> bool {
    commit.len() == COMMIT_LENGTH
        && commit
            .chars()
            .all(|character| matches!(character, '0'..='9' | 'a'..='f'))
}

fn commit_target(commit: String) -> Result<WatchTarget, Error> {
    if !is_full_sha(&commit) {
        return Err(Error::InvalidCommit { commit });
    }
    let target = WatchTarget::Commit(commit);
    Ok(target)
}

fn target(arguments: Arguments) -> Result<Watch, Error> {
    let target = match (arguments.branch, arguments.commit) {
        (Some(_), Some(_)) => return Err(Error::BothTargets),
        (None, None) => return Err(Error::NoTarget),
        (Some(branch), None) => WatchTarget::Branch(branch),
        (None, Some(commit)) => commit_target(commit)?,
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
        WatchTarget::Commit(commit) => format!("{}/commit:{commit}", watch.repo),
    }
}

fn watch(arguments: Option<serde_json::Value>) -> Result<Watch, Error> {
    let Some(value) = arguments else {
        return Err(Error::MissingArguments);
    };
    let parsed: Arguments = match serde_json::from_value(value) {
        Ok(parsed) => parsed,
        Err(reason) => return Err(Error::Arguments { reason }),
    };
    target(parsed)
}

pub fn parse_call(params: serde_json::Value) -> Result<Call, Error> {
    let call: CallParams = match serde_json::from_value(params) {
        Ok(call) => call,
        Err(reason) => return Err(Error::Arguments { reason }),
    };
    match call.name.as_str() {
        "status" => Ok(Call::Status),
        "watch" => {
            let parsed = watch(call.arguments)?;
            let requested = Call::Watch(parsed);
            Ok(requested)
        }
        "unwatch" => {
            let parsed = watch(call.arguments)?;
            let requested = Call::Unwatch(parsed);
            Ok(requested)
        }
        _ => Err(Error::UnknownTool { name: call.name }),
    }
}
