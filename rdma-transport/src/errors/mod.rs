use cuda::CudaErrors;
use rdma_core::RdmaErrors;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum TransportErrors {
    #[error("Rdma error: {0}")]
    RdmaErrors(RdmaErrors),
    #[error("Cuda error: {0}")]
    CudaErrors(CudaErrors),
    #[error("ops {0} failed with msg {1} ")]
    OpsFailed(String, String),
}

impl From<RdmaErrors> for TransportErrors {
    fn from(value: RdmaErrors) -> Self {
        TransportErrors::RdmaErrors(value)
    }
}

impl From<CudaErrors> for TransportErrors {
    fn from(value: CudaErrors) -> Self {
        TransportErrors::CudaErrors(value)
    }
}

pub type Result<T> = std::result::Result<T, TransportErrors>;
