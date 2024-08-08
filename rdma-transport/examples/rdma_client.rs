use std::net::SocketAddr;
use std::ops::DerefMut;
use std::time::Instant;

use anyhow::Result;

use rdma_transport::cuda::{cuda_host_to_device, cuda_init_ctx, cuda_mem_alloc, cuda_mem_free};
use rdma_transport::rdma::{self, Notification};
use rdma_transport::GPU_BUFFER_BASE_SIZE;

#[tokio::main]
pub async fn main() -> Result<()> {
    let server_addr = "192.168.14.224:23460".parse::<SocketAddr>()?;
    let local_addr = "192.168.14.224:23461".parse::<SocketAddr>()?;
    let gpu_ordinal = 4;
    let gpu_buffer_count = 4;

    let msg_size = GPU_BUFFER_BASE_SIZE as u32;
    let loops = 10000;

    let _ = cuda_init_ctx(gpu_ordinal)?;
    let mut local_gpu_buffers = Vec::with_capacity(gpu_buffer_count);
    for _ in 0..gpu_buffer_count {
        local_gpu_buffers.push(cuda_mem_alloc(GPU_BUFFER_BASE_SIZE)?);
    }

    let mut cm_id = rdma::client_init(server_addr, local_addr)?;

    let (cpu_conn, (mut cpu_mr, mut cpu_buffer), mut local_gpu_buffer_map, remote_gpu_conn_map) =
        rdma::connect(&mut cm_id, gpu_ordinal, local_gpu_buffers.clone())?;

    let origin_msg = "Hello, RDMA! The voice echoed through the dimly lit control room. The array of monitors flickered to life, displaying a mesmerizing array of data streams, holographic charts, and real-time simulations. Sitting at the central console was Dr. Elara Hinton, a leading expert in quantum computing and neural networks.".as_bytes();

    let mut msg: Vec<u8> = Vec::with_capacity(GPU_BUFFER_BASE_SIZE);
    while msg.len() < GPU_BUFFER_BASE_SIZE {
        msg.extend_from_slice(origin_msg);
    }
    for i in 0..gpu_buffer_count {
        cuda_host_to_device(&msg[0..GPU_BUFFER_BASE_SIZE], &local_gpu_buffers[i])?;
    }

    let remote_base_ptrs = remote_gpu_conn_map.keys().collect::<Vec<&u64>>();

    let start = Instant::now();
    for i in 0..loops {
        let gpu_buffer_index = i % gpu_buffer_count;
        let base_ptr = local_gpu_buffers[gpu_buffer_index].get_base_ptr();
        let (gpu_mr, gpu_buffer) = local_gpu_buffer_map.get_mut(&base_ptr).unwrap();
        let remote_base_ptr = remote_base_ptrs[gpu_buffer_index];
        let remote_gpu_conn = remote_gpu_conn_map.get(remote_base_ptr).unwrap();

        let notification = Notification {
            done: 0,
            req_id: Some(format!("request: {}", i).into_bytes()),
        };

        // println!("sample data: {}", String::from_utf8_lossy(&msg[0..50]));

        let size = bincode::serialized_size(&notification).unwrap();
        bincode::serialize_into(cpu_buffer.deref_mut(), &notification)?;

        rdma::write(
            &mut cm_id,
            remote_gpu_conn,
            gpu_mr,
            base_ptr,
            remote_gpu_conn.get_base_ptr(),
            msg_size,
        )
        .await?;
        rdma::write_metadata(
            &mut cm_id,
            &cpu_conn,
            &mut cpu_mr,
            &mut cpu_buffer,
            0,
            size as u16,
        )
        .await?;
    }

    let elapse = start.elapsed().as_millis();
    let bw = (msg_size as f32 * loops as f32 * 1000.0) / (elapse as f32 * 1024.0 * 1024.0);
    println!(
        "message size: {}, loops: {}, duration: {}, bw: {:.2} MB/s",
        msg_size, loops, elapse, bw
    );

    rdma::client_disconnect(&mut cm_id, &cpu_conn, &mut cpu_mr, &mut cpu_buffer).await?;

    for gpu_buffer in local_gpu_buffers {
        cuda_mem_free(&gpu_buffer)?;
    }

    Ok(())
}
