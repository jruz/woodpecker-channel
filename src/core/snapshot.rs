use crate::libs::woodpecker::types::Pipeline;

const COMMIT_PREFIX: usize = 7;

#[must_use]
pub fn is_failure(pipeline: &Pipeline) -> bool {
    let status = pipeline.status.as_str();
    matches!(status, "failure" | "error")
}

#[must_use]
pub fn is_finished(pipeline: &Pipeline) -> bool {
    let status = pipeline.status.as_str();
    matches!(
        status,
        "success" | "failure" | "error" | "killed" | "declined" | "skipped"
    )
}

fn was_reported(previous: &[Pipeline], pipeline: &Pipeline) -> bool {
    previous
        .iter()
        .any(|seen| seen.number == pipeline.number && seen.status == pipeline.status)
}

#[must_use]
pub fn finished_since(previous: &[Pipeline], current: &[Pipeline]) -> Vec<Pipeline> {
    let mut finished: Vec<Pipeline> = current
        .iter()
        .filter(|pipeline| is_finished(pipeline) && !was_reported(previous, pipeline))
        .cloned()
        .collect();
    finished.sort_by_key(|pipeline| pipeline.number);
    finished
}

fn line(pipeline: &Pipeline) -> String {
    let short: String = pipeline.commit.chars().take(COMMIT_PREFIX).collect();
    format!(
        "#{} {} {} {} {short}",
        pipeline.number, pipeline.event, pipeline.branch, pipeline.status
    )
}

#[must_use]
pub fn summary(pipelines: &[Pipeline]) -> String {
    if pipelines.is_empty() {
        return "awaiting a matching pipeline".to_owned();
    }
    let mut ordered: Vec<&Pipeline> = pipelines.iter().collect();
    ordered.sort_by_key(|pipeline| pipeline.number);
    ordered.into_iter().map(line).collect::<Vec<_>>().join("\n")
}
