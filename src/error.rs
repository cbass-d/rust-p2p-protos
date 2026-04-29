use thiserror::Error;

/// Top-level application error.
#[derive(Error, Debug)]
pub enum AppError {
    #[error("bind address missing for tcp mode")]
    NoBindAddress,
    #[error("failed to build simulation: {0}")]
    BuildError(String),
    #[error("all nodes exited")]
    AllNodesExited,
    #[error("max number of nodes is {max}")]
    MaxNodes { max: u8 },
    #[error("tracing subscriber error: {0}")]
    TracingInit(#[from] tracing::dispatcher::SetGlobalDefaultError),
    #[error("TUI init error: {0}")]
    TuiInit(#[source] std::io::Error),
    #[error("failed to draw frame: {0}")]
    TerminalDraw(#[source] std::io::Error),
    #[error("TUI error: {0}")]
    OtherTUI(#[source] std::io::Error),
}
