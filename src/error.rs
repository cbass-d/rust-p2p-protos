use thiserror::Error;

#[derive(Error, Debug)]
pub(crate) enum AppError {
    #[error("max number of nodes is {max}")]
    MaxNodes { max: u8 },
    #[error("tracing subcriber error: {0}")]
    TracingInit(#[from] tracing::dispatcher::SetGlobalDefaultError),
    #[error("TUI init error: {0}")]
    TuiInit(String),
    #[error("failed to draw frame: {0}")]
    TerminalDraw(String),
    #[error("TUI error: {0}")]
    OtherTUI(String),
}
