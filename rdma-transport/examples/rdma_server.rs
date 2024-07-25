use std::net::SocketAddr;

use anyhow::Result;
use rdma_transport::cuda::cuda_init_ctx;
use rdma_transport::rdma;

#[tokio::main]
pub async fn main() -> Result<()> {
    let bind_addr = "192.168.14.224:23457".parse::<SocketAddr>()?;
    let gpu_ordinal = 4;

    let _ = cuda_init_ctx(gpu_ordinal)?;
    let mut listen_id = rdma::server_init(&bind_addr)?;

    while let Ok(mut cm_id) = rdma::accept(&mut listen_id).await {
        tokio::spawn(async move {
            match rdma::handshake(&mut cm_id, gpu_ordinal).await {
                Ok((mut mr, mut buffer, _conn)) => loop {
                    let notification = rdma::handle_notification(&mut cm_id, &mut mr)
                        .await
                        .unwrap();
                    println!("notifcation: {:?}", notification);
                    if notification.done > 0 {
                        rdma::deregister_mr(&mut mr, &mut buffer).unwrap();
                        break;
                    }
                },
                Err(e) => {
                    println!("exchange qp failed: {:?}", e);
                }
            };
            rdma::disconnect(&mut cm_id).unwrap();
        });
    }

    Ok(())
}
