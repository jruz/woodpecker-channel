#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{name} is not set: {reason}")]
    Missing {
        name: &'static str,
        reason: std::env::VarError,
    },
}

pub struct Settings {
    pub home: String,
    pub server: String,
    pub token: String,
}

fn var(name: &'static str) -> Result<String, Error> {
    match std::env::var(name) {
        Ok(value) => Ok(value),
        Err(reason) => Err(Error::Missing { name, reason }),
    }
}

pub fn settings() -> Result<Settings, Error> {
    Ok(Settings {
        server: var("WOODPECKER_SERVER")?,
        token: var("WOODPECKER_TOKEN")?,
        home: var("HOME")?,
    })
}
