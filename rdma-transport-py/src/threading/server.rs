use log::{error, info};
use pyo3::prelude::*;
use rdma_transport::cuda::{cuda_device_to_host, CudaMemBuffer};
use rdma_transport::{cuda, rdma};
use std::net::SocketAddr;
use std::thread;
use std::time::Instant;
use tokio::runtime::Runtime;
use tokio::sync::oneshot::{self, Receiver, Sender};

#[pyclass]
pub struct RdmaServer {
    sender: Option<Sender<bool>>,
    sock_addr: SocketAddr,
    gpu_ordinal: i32,
}

pub async fn stop(rx: &mut Receiver<bool>) -> bool {
    match rx.await {
        Ok(res) => {
            info!("try to stop at {:?}", Instant::now());
            res
        }
        Err(_) => true,
    }
}

pub fn print_buffer(buffer: &CudaMemBuffer, size: usize) {
    let mut buffer_cpu: Vec<u8> = vec![0; size];

    info!("before {:?}", &buffer_cpu);

    cuda_device_to_host(buffer, &mut buffer_cpu).unwrap();

    info!("after {:?}", String::from_utf8_lossy(&buffer_cpu));
}

#[pymethods]
impl RdmaServer {
    #[new]
    fn new(sock_addr: String, gpu_ordinal: i32) -> Self {
        let sock_addr = match sock_addr.parse::<SocketAddr>() {
            Ok(sock_addr) => sock_addr,
            Err(e) => {
                error!("parse socket address failed: {:?}", e);
                panic!();
            }
        };

        RdmaServer {
            sender: None,
            sock_addr,
            gpu_ordinal,
        }
    }

    fn listen(&mut self) {
        let (tx, mut rx) = oneshot::channel::<bool>();
        self.sender = Some(tx);
        let mut listen_id = rdma::server_init(&self.sock_addr).unwrap();
        let gpu_ordinal = self.gpu_ordinal;
        let _ = thread::spawn(move || {
            let rt = Runtime::new().unwrap();
            rt.block_on(async {
                loop {
                    tokio::select! {
                        _ = stop(&mut rx) => {
                            cuda::cuda_device_primary_ctx_release(gpu_ordinal).unwrap();
                            break;
                        }
                        Ok(mut cm_id) = rdma::accept(&mut listen_id) => {
                            info!("start qp handshake");
                            rt.spawn( async move {
                                match rdma::handshake(&mut cm_id, gpu_ordinal).await {
                                    Ok((mut mr, mut buffer, _conn)) =>loop {
                                        let notification = rdma::handle_notification(&mut cm_id, &mut mr).await.unwrap();
                                        if notification.done > 0 {
                                            info!("notifcation: {:?}" , notification);
                                            rdma::deregister_mr(&mut mr, &mut buffer).unwrap();
                                            break;
                                        } else {
                                            print_buffer(&buffer, notification.size);
                                        }
                                    }
                                    Err(e) => {
                                        error!("exchange qp failed: {:?}", e);
                                    }
                                };
                                rdma::disconnect(&mut cm_id).unwrap();
                            });
                        }
                    }
                }
            });

            info!("runtime end at {:?}", Instant::now());
        });
    }

    fn shutdown(&mut self) {
        if let Some(tx) = self.sender.take() {
            let _ = tx.send(true);
        }
    }
}
