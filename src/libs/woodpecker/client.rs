use base64::Engine as _;

use crate::libs::woodpecker::types::{
    Config, LogEntry, Pipeline, PipelineDetail, Repo, Step, WatchTarget,
};

const RECENT_PIPELINES: &str = "20";

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to build HTTP client: {reason}")]
    Build { reason: reqwest::Error },

    #[error("invalid response from {path}: {reason}")]
    Decode {
        path: String,
        reason: serde_json::Error,
    },

    #[error("invalid base64 in log line {line}: {reason}")]
    LogData {
        line: u64,
        reason: base64::DecodeError,
    },

    #[error("pipeline {pipeline} has no failed step")]
    NoFailedStep { pipeline: u64 },

    #[error("request to {path} failed: {reason}")]
    Request {
        path: String,
        reason: reqwest::Error,
    },

    #[error("{path} returned {status}")]
    Status {
        path: String,
        status: reqwest::StatusCode,
    },
}

fn ensure_crypto_provider() {
    if rustls::crypto::CryptoProvider::get_default().is_some() {
        return;
    }
    let provider = rustls::crypto::ring::default_provider();
    if let Err(installed) = provider.install_default() {
        tracing::debug!(?installed, "crypto provider installed concurrently");
    }
}

pub fn new() -> Result<reqwest::Client, Error> {
    ensure_crypto_provider();
    match reqwest::Client::builder().build() {
        Ok(http) => Ok(http),
        Err(reason) => Err(Error::Build { reason }),
    }
}

fn target_query(target: &WatchTarget) -> Vec<(&'static str, &str)> {
    match target {
        WatchTarget::Branch(branch) => vec![("branch", branch), ("perPage", RECENT_PIPELINES)],
        WatchTarget::Commit(_) => vec![("perPage", RECENT_PIPELINES)],
    }
}

fn matches_target(target: &WatchTarget, pipeline: &Pipeline) -> bool {
    match target {
        WatchTarget::Branch(_) => true,
        WatchTarget::Commit(commit) => &pipeline.commit == commit,
    }
}

async fn fetch_body(
    http: &reqwest::Client,
    config: &Config,
    path: &str,
    query: &[(&str, &str)],
) -> Result<String, Error> {
    let url = format!("{}/api{path}", config.server);
    let request = http.get(url).bearer_auth(&config.token).query(query);
    let response = match request.send().await {
        Ok(response) => response,
        Err(reason) => {
            return Err(Error::Request {
                path: path.to_owned(),
                reason,
            });
        }
    };
    let status = response.status();
    if !status.is_success() {
        return Err(Error::Status {
            path: path.to_owned(),
            status,
        });
    }
    match response.text().await {
        Ok(body) => Ok(body),
        Err(reason) => Err(Error::Request {
            path: path.to_owned(),
            reason,
        }),
    }
}

async fn fetch_json<T>(
    http: &reqwest::Client,
    config: &Config,
    path: &str,
    query: &[(&str, &str)],
) -> Result<T, Error>
where
    T: serde::de::DeserializeOwned,
{
    let body = fetch_body(http, config, path, query).await?;
    match serde_json::from_str(&body) {
        Ok(value) => Ok(value),
        Err(reason) => Err(Error::Decode {
            path: path.to_owned(),
            reason,
        }),
    }
}

pub async fn repo_id(http: &reqwest::Client, config: &Config, repo: &str) -> Result<u64, Error> {
    let path = format!("/repos/lookup/{repo}");
    let found: Repo = fetch_json(http, config, &path, &[]).await?;
    Ok(found.id)
}

pub async fn recent_pipelines(
    http: &reqwest::Client,
    config: &Config,
    repo_id: u64,
    target: &WatchTarget,
) -> Result<Vec<Pipeline>, Error> {
    let path = format!("/repos/{repo_id}/pipelines");
    let query = target_query(target);
    let pipelines: Vec<Pipeline> = fetch_json(http, config, &path, &query).await?;
    let matching = pipelines
        .into_iter()
        .filter(|pipeline| matches_target(target, pipeline))
        .collect();
    Ok(matching)
}

fn is_failed(step: &Step) -> bool {
    step.state == "failure" || step.state == "error"
}

pub async fn failed_step_id(
    http: &reqwest::Client,
    config: &Config,
    repo_id: u64,
    pipeline_number: u64,
) -> Result<u64, Error> {
    let path = format!("/repos/{repo_id}/pipelines/{pipeline_number}");
    let detail: PipelineDetail = fetch_json(http, config, &path, &[]).await?;
    let failed = detail
        .workflows
        .iter()
        .flat_map(|workflow| &workflow.children)
        .find(|step| is_failed(step));
    let Some(step) = failed else {
        return Err(Error::NoFailedStep {
            pipeline: pipeline_number,
        });
    };
    Ok(step.id)
}

fn decode_line(entry: &LogEntry) -> Result<String, Error> {
    let bytes = match base64::engine::general_purpose::STANDARD.decode(&entry.data) {
        Ok(bytes) => bytes,
        Err(reason) => {
            return Err(Error::LogData {
                line: entry.line,
                reason,
            });
        }
    };
    let text = String::from_utf8_lossy(&bytes);
    let line = text.into_owned();
    Ok(line)
}

pub async fn step_logs(
    http: &reqwest::Client,
    config: &Config,
    repo_id: u64,
    pipeline_number: u64,
    step_id: u64,
) -> Result<Vec<String>, Error> {
    let path = format!("/repos/{repo_id}/logs/{pipeline_number}/{step_id}");
    let entries: Vec<LogEntry> = fetch_json(http, config, &path, &[]).await?;
    entries.iter().map(decode_line).collect()
}

#[cfg(test)]
mod tests {
    use crate::libs::woodpecker::client;
    use crate::libs::woodpecker::types::{Config, Pipeline, WatchTarget};

    const PIPELINES: &str = r#"[{"author": "bot", "branch": "main", "commit": "def456", "event": "push", "message": "update flake", "number": 4, "status": "running"}, {"author": "jruz", "branch": "main", "commit": "abc123", "event": "push", "message": "fix things", "number": 3, "status": "failure"}]"#;

    const DETAIL: &str = r#"{"workflows": [{"children": [{"id": 1, "state": "success"}, {"id": 2, "state": "failure"}]}]}"#;

    const LOGS: &str =
        r#"[{"data": "Y2FyZ28gdGVzdA==", "line": 0}, {"data": "dGVzdCBmYWlsZWQ=", "line": 1}]"#;

    fn config(server: &wiremock::MockServer) -> Config {
        Config {
            server: server.uri(),
            token: "secret".to_owned(),
        }
    }

    fn json_response(body: &str) -> wiremock::ResponseTemplate {
        let template = wiremock::ResponseTemplate::new(200);
        template.set_body_raw(body, "application/json")
    }

    async fn mount(server: &wiremock::MockServer, mock: wiremock::MockBuilder, body: &str) {
        let response = json_response(body);
        mock.respond_with(response).mount(server).await;
    }

    async fn mount_json(server: &wiremock::MockServer, route: &str, body: &str) {
        let get = wiremock::matchers::method("GET");
        let path = wiremock::matchers::path(route);
        let auth = wiremock::matchers::header("authorization", "Bearer secret");
        let mock = wiremock::Mock::given(get).and(path).and(auth);
        mount(server, mock, body).await;
    }

    async fn mount_pipelines(server: &wiremock::MockServer, key: &str, value: &str, body: &str) {
        let path = wiremock::matchers::path("/api/repos/7/pipelines");
        let query = wiremock::matchers::query_param(key, value);
        let mock = wiremock::Mock::given(path).and(query);
        mount(server, mock, body).await;
    }

    #[tokio::test]
    async fn resolves_repo_id() {
        let server = wiremock::MockServer::start().await;
        mount_json(&server, "/api/repos/lookup/jruz/demo", r#"{"id": 7}"#).await;
        let http = client::new().expect("client");
        let settings = config(&server);
        let id = client::repo_id(&http, &settings, "jruz/demo")
            .await
            .expect("repo id");
        assert_eq!(id, 7);
    }

    fn numbers(pipelines: &[Pipeline]) -> Vec<u64> {
        pipelines.iter().map(|pipeline| pipeline.number).collect()
    }

    #[tokio::test]
    async fn fetches_branch_pipelines() {
        let server = wiremock::MockServer::start().await;
        mount_pipelines(&server, "branch", "main", PIPELINES).await;
        let http = client::new().expect("client");
        let settings = config(&server);
        let branch = "main".to_owned();
        let target = WatchTarget::Branch(branch);
        let pipelines = client::recent_pipelines(&http, &settings, 7, &target)
            .await
            .expect("pipelines");
        assert_eq!(numbers(&pipelines), vec![4, 3]);
    }

    #[tokio::test]
    async fn filters_pipelines_by_commit() {
        let server = wiremock::MockServer::start().await;
        mount_pipelines(&server, "perPage", "20", PIPELINES).await;
        let http = client::new().expect("client");
        let settings = config(&server);
        let commit = "abc123".to_owned();
        let target = WatchTarget::Commit(commit);
        let pipelines = client::recent_pipelines(&http, &settings, 7, &target)
            .await
            .expect("pipelines");
        assert_eq!(numbers(&pipelines), vec![3]);
    }

    #[tokio::test]
    async fn finds_failed_step() {
        let server = wiremock::MockServer::start().await;
        mount_json(&server, "/api/repos/7/pipelines/3", DETAIL).await;
        let http = client::new().expect("client");
        let settings = config(&server);
        let step = client::failed_step_id(&http, &settings, 7, 3)
            .await
            .expect("failed step");
        assert_eq!(step, 2);
    }

    #[tokio::test]
    async fn decodes_step_logs() {
        let server = wiremock::MockServer::start().await;
        mount_json(&server, "/api/repos/7/logs/3/2", LOGS).await;
        let http = client::new().expect("client");
        let settings = config(&server);
        let lines = client::step_logs(&http, &settings, 7, 3, 2)
            .await
            .expect("logs");
        assert_eq!(lines, vec!["cargo test", "test failed"]);
    }

    #[tokio::test]
    async fn reports_http_status() {
        let server = wiremock::MockServer::start().await;
        let http = client::new().expect("client");
        let settings = config(&server);
        let result = client::repo_id(&http, &settings, "jruz/missing").await;
        assert!(matches!(
            result,
            Err(client::Error::Status { status, .. }) if status == reqwest::StatusCode::NOT_FOUND
        ));
    }
}
