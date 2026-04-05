use thiserror::Error;

#[derive(Error, Debug)]
pub(crate) enum AppError {
    #[error("bind address missing for tcp mode")]
    NoBindAddress,
    #[error("failed to build simulation: {0}")]
    BuildError(String),
    #[error("max number of nodes is {max}")]
    MaxNodes { max: u8 },
    #[error("tracing subscriber error: {0}")]
    TracingInit(#[from] tracing::dispatcher::SetGlobalDefaultError),
    #[error("TUI init error: {0}")]
    TuiInit(String),
    #[error("failed to draw frame: {0}")]
    TerminalDraw(String),
    #[error("TUI error: {0}")]
    OtherTUI(String),
}
