use tracing_subscriber::layer::SubscriberExt as _;
use tracing_subscriber::util::SubscriberInitExt as _;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to open log file in {directory}: {reason}")]
    File {
        directory: String,
        reason: tracing_appender::rolling::InitError,
    },

    #[error("failed to install logger")]
    Install(#[from] tracing_subscriber::util::TryInitError),
}

fn log_directory(home: &str) -> String {
    format!("{home}/.local/state/woodpecker-channel")
}

fn file_appender(
    directory: String,
) -> Result<tracing_appender::rolling::RollingFileAppender, Error> {
    let builder = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::NEVER)
        .filename_prefix("server.log");
    match builder.build(&directory) {
        Ok(appender) => Ok(appender),
        Err(reason) => Err(Error::File { directory, reason }),
    }
}

pub fn init(home: &str) -> Result<(), Error> {
    let directory = log_directory(home);
    let appender = file_appender(directory)?;
    let stderr_layer = tracing_subscriber::fmt::layer().with_writer(std::io::stderr);
    let file_layer = tracing_subscriber::fmt::layer()
        .with_ansi(false)
        .with_writer(appender);
    tracing_subscriber::registry()
        .with(tracing_subscriber::filter::LevelFilter::INFO)
        .with(stderr_layer)
        .with(file_layer)
        .try_init()?;
    Ok(())
}
