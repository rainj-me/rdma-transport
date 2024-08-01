use log::{error, info};
use pyo3::prelude::*;
use rdma_transport::{cuda, rdma, GPUMemBuffer};
use std::net::SocketAddr;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Instant;
use tokio::runtime::Runtime;
use tokio::sync::oneshot::{self, Sender};

use super::{CompletionReqs,  TensorBlocks};

#[pyclass]
pub enum Command {
    Complete(),
}

#[pyclass]
pub struct VllmRdmaServer {
    cmd_sender: Option<Sender<Command>>,
    sock_addr: SocketAddr,
    gpu_ordinal: i32,
    local_buffer: TensorBlocks,
    completion_reqs: Option<Arc<RwLock<CompletionReqs>>>,
}

#[pymethods]
impl VllmRdmaServer {
    #[new]
    fn new(sock_addr: String, gpu_ordinal: i32, local_buffer: TensorBlocks) -> Self {
        let sock_addr = match sock_addr.parse::<SocketAddr>() {
            Ok(sock_addr) => sock_addr,
            Err(e) => {
                error!("parse socket address failed: {:?}", e);
                panic!();
            }
        };

        VllmRdmaServer {
            cmd_sender: None,
            sock_addr,
            gpu_ordinal,
            local_buffer,
            completion_reqs: None,
        }
    }

    fn listen(&mut self) {
        let (cmd_tx, mut cmd_rx) = oneshot::channel::<Command>();
        self.cmd_sender = Some(cmd_tx);
        let completion_reqs = Arc::new(RwLock::new(CompletionReqs::new(1024)));
        self.completion_reqs = Some(completion_reqs.clone());
        let mut listen_id = rdma::server_init(&self.sock_addr).unwrap();
        let gpu_ordinal = self.gpu_ordinal;
        let gpu_buffers = self.local_buffer.iter().map(Into::into).collect::<Vec<GPUMemBuffer>>();
        let _ = thread::spawn(move || {
            let rt = Runtime::new().unwrap();
            rt.block_on(async {
                loop {
                    let completion_reqs = completion_reqs.clone();
                    tokio::select! {
                        Ok(Command::Complete()) = (&mut cmd_rx) => {
                            cuda::cuda_device_primary_ctx_release(gpu_ordinal).unwrap();
                            break;
                        }
                        Ok(mut cm_id) = rdma::listen(&mut listen_id) => {
                            info!("start qp handshake");
                            let gpu_buffers = gpu_buffers.clone();
                            rt.spawn( async move {
                                match rdma::accept(&mut cm_id, gpu_ordinal, gpu_buffers).await {
                                    Ok((_conn, (mut cpu_mr, mut cpu_buffer), _)) =>loop {
                                        let notification = rdma::handle_notification(&mut cm_id, &mut cpu_mr, &mut cpu_buffer).await.unwrap();
                                        if notification.done > 0 {
                                            info!("notifcation: {:?}" , notification);
                                            break;
                                        }
                                        
                                        if notification.remaining == 0 {
                                            let mut reqs = completion_reqs.write().unwrap();
                                            reqs.add_req(&notification.req_id);
                                            if reqs.is_full() {
                                                reqs.remove_first();
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        error!("exchange qp failed: {:?}", e);
                                    }
                                };
                                rdma::server_disconnect(&mut cm_id).unwrap();
                            });
                        }
                    }
                }
            });

            info!("runtime end at {:?}", Instant::now());
        });
    }

    fn is_complete(&mut self, req_id: Vec<u8>) -> bool {
        if let Some(completion_reqs) = &self.completion_reqs {
            match completion_reqs.try_read() {
                Ok(reqs) => reqs.is_req_complete(&req_id),
                Err(_) => false,
            }
        } else {
            false
        }
    }

    fn shutdown(&mut self) {
        if let Some(sender) = self.cmd_sender.take() {
            let _ = sender.send(Command::Complete());
        }
    }
}
