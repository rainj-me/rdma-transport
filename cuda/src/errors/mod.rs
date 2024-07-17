use thiserror::Error;

#[derive(Error, Debug)]
pub enum CudaErrors {
    #[error("ops {0} failed with errorno: {1}")]
    OpsFailed(String, u32),
    #[error("operation not found: {0}")]
    OpsNotFound(String),
}

pub type Result<T> = std::result::Result<T, CudaErrors>;
