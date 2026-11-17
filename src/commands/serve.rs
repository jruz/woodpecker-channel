use crate::core::logging;
use crate::core::server;
use crate::utils::env;
use crate::utils::output;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Env(#[from] env::Error),

    #[error(transparent)]
    Logging(#[from] logging::Error),

    #[error("failed to start async runtime: {reason}")]
    Runtime { reason: std::io::Error },

    #[error(transparent)]
    Server(#[from] server::Error),
}

fn serve() -> Result<(), Error> {
    let settings = env::settings()?;
    logging::init(&settings.home)?;
    let runtime = match tokio::runtime::Runtime::new() {
        Ok(runtime) => runtime,
        Err(reason) => return Err(Error::Runtime { reason }),
    };
    let serving = server::run(settings.server, settings.token);
    runtime.block_on(serving)?;
    Ok(())
}

#[must_use]
pub fn run() -> output::Output {
    match serve() {
        Ok(()) => output::Output {
            success: true,
            text: String::new(),
        },
        Err(error) => output::failure(&error),
    }
}
