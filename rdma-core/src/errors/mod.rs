use thiserror::Error;

#[derive(Error, Debug)]
pub enum RdmaErrors {
    #[error("ops {0} failed with errorno: {1}")]
    OpsFailed(String, i32),
    #[error("operation not found: {0}")]
    OpsNotFound(String),
    #[error("invalid address: {0}")]
    InvalidAddress(String)
}

pub type Result<T> = std::result::Result<T, RdmaErrors>;
