mod errors;
#[macro_use]
mod macros;

pub mod ibverbs;
pub mod rdma;

pub use errors::{RdmaErrors, Result};
pub(crate) use macros::{rdma_call, rdma_type};
