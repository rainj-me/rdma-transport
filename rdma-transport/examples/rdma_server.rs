use std::net::SocketAddr;

use anyhow::Result;
use rdma_transport::cuda::{cuda_device_to_host, cuda_init_ctx};
use rdma_transport::{rdma, GPUMemBuffer, GPU_BUFFER_BASE_SIZE};

#[tokio::main]
pub async fn main() -> Result<()> {
    let bind_addr = "192.168.14.224:23460".parse::<SocketAddr>()?;
    let gpu_ordinal = 4;

    let _ = cuda_init_ctx(gpu_ordinal)?;
    let mut listen_id = rdma::server_init(&bind_addr)?;

    while let Ok(mut cm_id) = rdma::accept(&mut listen_id).await {
        tokio::spawn(async move {
            match rdma::handshake(&mut cm_id, gpu_ordinal).await {
                Ok((_conn, (_gpu_mr, mut gpu_buffer), (mut cpu_mr, mut cpu_buffer))) => loop {
                    let notification =
                        rdma::handle_notification(&mut cm_id, &mut cpu_mr, &mut cpu_buffer)
                            .await
                            .unwrap();
                    if notification.done == 1 {
                        println!("notifcation: {:?}", notification);
                        rdma::send_ack(&mut cm_id, &mut cpu_mr, &notification).await.unwrap();
                        rdma::server_disconnect(&mut cm_id).unwrap();
                        rdma::free_gpu_membuffer(&mut gpu_buffer).unwrap();
                        break;
                    } else {
                        println!("notification: {:?}", notification);
                        let (_, offset, size) = notification.buffer;
                        let mut data = Box::new([0; GPU_BUFFER_BASE_SIZE]);
                        let device_buffer =
                            GPUMemBuffer::new(gpu_buffer.get_ptr() + offset as u64, size as usize);
                        cuda_device_to_host(&device_buffer, data.as_mut(), Some(32)).unwrap();
                        rdma::send_ack(&mut cm_id, &mut cpu_mr, &notification).await.unwrap();
                        println!("data: {}", String::from_utf8_lossy(&data[0..32]));
                    }
                },
                Err(e) => {
                    println!("exchange qp failed: {:?}", e);
                }
            };
        });
    }

    Ok(())
}
