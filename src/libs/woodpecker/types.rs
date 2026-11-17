#[derive(Debug, Clone)]
pub struct Config {
    pub server: String,
    pub token: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum WatchTarget {
    Branch(String),
    Event(String),
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Repo {
    pub id: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Deserialize)]
pub struct Pipeline {
    pub author: String,
    pub branch: String,
    pub commit: String,
    pub event: String,
    pub message: String,
    pub number: u64,
    pub status: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Step {
    pub id: u64,
    pub state: String,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct Workflow {
    pub children: Vec<Step>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct PipelineDetail {
    pub workflows: Vec<Workflow>,
}

#[derive(Debug, Clone, serde::Deserialize)]
pub struct LogEntry {
    pub data: String,
    pub line: u64,
}
