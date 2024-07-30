use std::net::SocketAddr;
use std::ops::DerefMut;
use std::time::Instant;

use anyhow::Result;

use rdma_transport::cuda::{cuda_host_to_device, cuda_init_ctx};
use rdma_transport::rdma::{self, Notification};
use rdma_transport::GPU_BUFFER_BASE_SIZE;

#[tokio::main]
pub async fn main() -> Result<()> {
    let server_addr = "192.168.14.224:23460".parse::<SocketAddr>()?;
    let local_addr = "192.168.14.224:23461".parse::<SocketAddr>()?;
    let gpu_ordinal = 4;

    let msg_size = GPU_BUFFER_BASE_SIZE as u32;
    let loops = 100;

    let _ = cuda_init_ctx(gpu_ordinal)?;

    let mut cm_id = rdma::client_init(server_addr, local_addr)?;

    let (conn, (mut gpu_mr, mut gpu_buffer), (mut cpu_mr, mut cpu_buffer)) =
        rdma::connect(&mut cm_id, gpu_ordinal)?;

    let origin_msg = "Hello, RDMA! The voice echoed through the dimly lit control room. The array of monitors flickered to life, displaying a mesmerizing array of data streams, holographic charts, and real-time simulations. Sitting at the central console was Dr. Elara Hinton, a leading expert in quantum computing and neural networks.".as_bytes();

    let mut msg: Vec<u8> = Vec::with_capacity(gpu_buffer.get_capacity());
    while msg.len() < gpu_buffer.get_capacity() {
        msg.extend_from_slice(origin_msg);
    }

    cuda_host_to_device(&msg[0..gpu_buffer.get_capacity()], &gpu_buffer)?;

    let start = Instant::now();
    for i in 0..loops {
        let total_offsets = gpu_buffer.get_capacity() as u32 / msg_size;
        let offset = i % total_offsets;

        let notification = Notification {
            done: 0,
            buffer: (&mut gpu_buffer as *mut _ as u64, offset, msg_size),
            data: Vec::new(),
        };

        let (start, end) = (offset as usize, offset as usize + 32);
        println!("data: {}", String::from_utf8_lossy(&msg[start..end]));

        let size = bincode::serialized_size(&notification).unwrap();
        bincode::serialize_into(cpu_buffer.deref_mut(), &notification)?;
        
        rdma::write(
            &mut cm_id,
            &conn,
            &mut gpu_mr,
            &mut gpu_buffer,
            offset,
            msg_size,
        )
        .await?;
        rdma::write_metadata(&mut cm_id, &conn, &mut cpu_mr, &mut cpu_buffer, offset as u16, size as u16).await?;
    }

    let elapse = start.elapsed().as_millis();
    let bw = (msg_size as f32 * loops as f32 * 1000.0) / (elapse as f32 * 1024.0 * 1024.0);
    println!(
        "message size: {}, loops: {}, duration: {}, bw: {:.2} MB/s",
        msg_size, loops, elapse, bw
    );

    rdma::client_disconnect(&mut cm_id, &conn, &mut cpu_mr, &mut cpu_buffer).await?;

    Ok(())
}
