pub struct Output {
    pub success: bool,
    pub text: String,
}

fn causes(error: &dyn std::error::Error) -> Vec<String> {
    let first = Some(error);
    std::iter::successors(first, |current| current.source())
        .map(ToString::to_string)
        .collect()
}

#[must_use]
pub fn failure(error: &dyn std::error::Error) -> Output {
    Output {
        success: false,
        text: format!("error: {}\n", causes(error).join(": ")),
    }
}
