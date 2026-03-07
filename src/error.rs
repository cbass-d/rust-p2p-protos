use thiserror::Error;

#[derive(Error, Debug)]
pub(crate) enum AppError {
    #[error("max number of nodes is {max}")]
    MaxNodes { max: u8 },
    #[error("tracing subcriber error: {0}")]
    TracingInit(#[from] tracing::dispatcher::SetGlobalDefaultError),
}
