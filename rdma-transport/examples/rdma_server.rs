use std::net::SocketAddr;

use anyhow::Result;
use rdma_transport::cuda::{cuda_device_to_host, cuda_init_ctx, cuda_mem_alloc, cuda_mem_free};
use rdma_transport::{rdma, GPUMemBuffer, GPU_BUFFER_BASE_SIZE};

#[tokio::main]
pub async fn main() -> Result<()> {
    let bind_addr = "192.168.14.224:23460".parse::<SocketAddr>()?;
    let gpu_ordinal = 4;
    let gpu_buffer_count = 4;

    let _ = cuda_init_ctx(gpu_ordinal)?;
    let mut local_gpu_buffers = Vec::with_capacity(gpu_buffer_count);
    for _ in 0..gpu_buffer_count {
        local_gpu_buffers.push(cuda_mem_alloc(GPU_BUFFER_BASE_SIZE)?);
    }
    let mut listen_id = rdma::server_init(&bind_addr)?;

    while let Ok(mut cm_id) = rdma::listen(&mut listen_id).await {
        let local_gpu_buffers = local_gpu_buffers.clone();
        tokio::spawn(async move {
            match rdma::accept(&mut cm_id, gpu_ordinal, local_gpu_buffers).await {
                Ok((_conn, (mut cpu_mr, mut cpu_buffer), _)) => loop {
                    let notification =
                        rdma::handle_notification(&mut cm_id, &mut cpu_mr, &mut cpu_buffer)
                            .await
                            .unwrap();
                    if notification.done == 1 {
                        println!("notifcation: {:?}", notification);
                        rdma::server_disconnect(&mut cm_id).unwrap();
                        break;
                    } else {
                        // println!("notification: {:?}", notification);
                        if let Some(req_id) = &notification.req_id {
                            println!("request {} complete", hex::encode(req_id));
                        }
                        // let (_, offset, size) = notification.buffer;
                        // let mut data = Box::new([0; GPU_BUFFER_BASE_SIZE]);
                        // let device_buffer =
                        //     GPUMemBuffer::new(gpu_buffer.get_ptr() + offset as u64, size as usize);
                        // cuda_device_to_host(&device_buffer, data.as_mut(), Some(32)).unwrap();
                        // println!("data: {}", String::from_utf8_lossy(&data[0..32]));
                    }
                },
                Err(e) => {
                    println!("exchange qp failed: {:?}", e);
                }
            };
        });
    }

    for gpu_buffer in local_gpu_buffers {
        cuda_mem_free(&gpu_buffer)?;
    }

    Ok(())
}
