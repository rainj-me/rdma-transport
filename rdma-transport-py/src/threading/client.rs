use log::{error, info};
use pyo3::prelude::*;
use rdma_core::ibverbs::IbvMr;
use rdma_core::rdma::RdmaCmId;
use rdma_transport::cuda::{cuda_host_to_device, CudaMemBuffer};
use rdma_transport::rdma::{self, Notification};
// use rdma_transport::{cuda, rdma};
use std::net::SocketAddr;
use std::thread;
use std::time::Instant;
use tokio::runtime::Runtime;
use tokio::sync::mpsc::{channel, Sender};

#[pyclass]
#[derive(Debug, Clone, Copy)]
pub enum Command {
    Send { size: usize },
    Complete(),
}

#[pyclass]
pub struct RdmaClient {
    sender: Option<Sender<Command>>,
    buffer: Option<(u64, usize)>,
    local_addr: SocketAddr,
    gpu_ordinal: i32,
}

pub async fn stop(cm_id: &mut RdmaCmId, mr: &mut IbvMr, buffer: &CudaMemBuffer) {
    let mut notification = Notification { size: 0, done: 1 };
    rdma::send(cm_id, &mut notification).await.unwrap();
    rdma::disconnect(cm_id).unwrap();
    rdma::deregister_mr(mr, buffer).unwrap();
}

#[pymethods]
impl RdmaClient {
    #[new]
    fn new(local_addr: String, gpu_ordinal: i32) -> Self {
        let local_addr = match local_addr.parse::<SocketAddr>() {
            Ok(sock_addr) => sock_addr,
            Err(e) => {
                error!("parse socket address failed: {:?}", e);
                panic!();
            }
        };

        RdmaClient {
            sender: None,
            buffer: None,
            local_addr,
            gpu_ordinal,
        }
    }

    fn connect(&mut self, server_addr: String) {
        let server_addr = match server_addr.parse::<SocketAddr>() {
            Ok(sock_addr) => sock_addr,
            Err(e) => {
                error!("parse socket address failed: {:?}", e);
                panic!();
            }
        };

        let (tx, mut rx) = channel(1);
        self.sender = Some(tx);
        let mut cm_id = rdma::client_init(server_addr, self.local_addr).unwrap();
        let gpu_ordinal = self.gpu_ordinal;
        let (mut mr, mut buffer, conn) = rdma::connect(&mut cm_id, gpu_ordinal).unwrap();
        self.buffer = Some((buffer.get_ptr(), buffer.get_size()));

        let _ = thread::spawn(move || {
            let rt = Runtime::new().unwrap();
            rt.block_on(async {
                while let Some(cmd) = (&mut rx).recv().await {
                    match cmd {
                        Command::Send { size } => {
                            rdma::write(&mut cm_id, &mut mr, &conn, &mut buffer, size)
                                .await
                                .unwrap();
                        }
                        Command::Complete() => {
                            info!("complete");
                            stop(&mut cm_id, &mut mr, &mut buffer).await;
                            break;
                        }
                    }
                }
            });

            info!("runtime end at {:?}", Instant::now());
        });
    }

    fn get_buffer(&self) -> Option<(u64, usize)> {
        self.buffer
    }

    fn fill_data(&self, data: String) {
        if let Some((ptr, size)) = self.buffer {
            let buffer = CudaMemBuffer::new(ptr, size);
            cuda_host_to_device(data.as_bytes(), &buffer).unwrap();
        }
    }

    fn send(&self, size: usize) {
        if let Some(sender) = self.sender.clone() {
            let rt = Runtime::new().unwrap();
            rt.block_on(async move {
                sender.send(Command::Send { size }).await.unwrap();
            });
        }
    }

    fn shutdown(&mut self) {
        if let Some(sender) = self.sender.take() {
            let rt = Runtime::new().unwrap();
            rt.block_on(async move {
                let _ = sender.send(Command::Complete());
            });
        }
    }
}
