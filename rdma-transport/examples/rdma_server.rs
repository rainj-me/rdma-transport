use std::net::SocketAddr;

use anyhow::Result;
use rdma_transport::cuda::{cuda_init_ctx, cuda_mem_alloc, cuda_mem_free, cuda_set_current_ctx};
use rdma_transport::rdma::serve;

const BUFFER_SIZE: usize = 16 * 1024 * 1024;

#[tokio::main]
pub async fn main() -> Result<()> {
    let bind_addr = "192.168.14.224:23456".parse::<SocketAddr>()?;
    let gpu_ordinal = 4;

    let cu_ctx = cuda_init_ctx(gpu_ordinal)?;
    // cuda_set_current_ctx(&mut cu_ctx)?;
    // let buffer = cuda_mem_alloc(BUFFER_SIZE)?;
    
    let _ = serve(bind_addr, cu_ctx).await;

    // cuda_mem_free(&buffer)?;

    Ok(())
}
