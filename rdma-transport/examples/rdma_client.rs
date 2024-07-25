use std::net::SocketAddr;
use std::time::Instant;

use anyhow::Result;

use rdma_core::rdma::rdma_disconnect;

use rdma_transport::cuda::{cuda_host_to_device, cuda_init_ctx, cuda_mem_free};
use rdma_transport::rdma::{self, Notification};

#[tokio::main]
pub async fn main() -> Result<()> {
    let server_addr = "192.168.14.224:23457".parse::<SocketAddr>()?;
    let local_addr = "192.168.14.224:23458".parse::<SocketAddr>()?;
    let gpu_ordinal = 4;

    let msg_size = 1024 * 1024;
    let loops = 100;

    let _ = cuda_init_ctx(gpu_ordinal)?;

    let mut cm_id = rdma::client_init(server_addr, local_addr)?;

    let (mut mr, mut buffer, conn) = rdma::connect(&mut cm_id, gpu_ordinal)?;

    let msg = "Hello, RDMA! The voice echoed through the dimly lit control room. The array of monitors flickered to life, displaying a mesmerizing array of data streams, holographic charts, and real-time simulations. Sitting at the central console was Dr. Elara Hinton, a leading expert in quantum computing and neural networks.".as_bytes();

    cuda_host_to_device(msg, &buffer)?;

    let start = Instant::now();
    for _ in 0..loops {
        rdma::write(&mut cm_id, &mut mr, &conn, &mut buffer, msg_size).await?;
    }
    let elapse = start.elapsed().as_millis();
    let bw = (msg_size as f32 * loops as f32 * 1000.0) / (elapse as f32 * 1024.0 * 1024.0);
    println!(
        "message size: {}, loops: {}, duration: {}, bw: {:.2} MB/s",
        msg_size, loops, elapse, bw
    );

    let mut notification = Notification { size: 0, done: 1 };
    rdma::send(&mut cm_id, &mut notification).await?;
    rdma_disconnect(&mut cm_id)?;
    cuda_mem_free(&buffer)?;

    Ok(())
}
