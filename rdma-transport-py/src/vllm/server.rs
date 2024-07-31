use log::{error, info};
use pyo3::prelude::*;
use rdma_transport::cuda::cuda_device_to_host;
use rdma_transport::rdma::Notification;
use rdma_transport::{cuda, rdma, GPUMemBuffer, GPU_BUFFER_BASE_SIZE};
use tokio::sync::mpsc::Receiver;
use std::net::SocketAddr;
use std::task::Poll;
use std::thread;
use std::time::Instant;
use tokio::runtime::Runtime;
use tokio::sync::{
    mpsc::{self, UnboundedReceiver},
    oneshot::{self, Sender},
};

use super::{TensorBlock, TensorBlocks};

#[pyclass]
pub enum Command {
    Complete(),
}

#[pyclass]
pub struct VllmRdmaServer {
    cmd_sender: Option<Sender<Command>>,
    data_reciever: Option<Receiver<Vec<u8>>>,
    sock_addr: SocketAddr,
    gpu_ordinal: i32,
    local_buffer: TensorBlocks,
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
            data_reciever: None,
            sock_addr,
            gpu_ordinal,
            local_buffer,
        }
    }

    fn listen(&mut self) {
        let (cmd_tx, mut cmd_rx) = oneshot::channel::<Command>();
        let (data_tx, data_rx) = mpsc::channel::<Vec<u8>>(1024);
        self.cmd_sender = Some(cmd_tx);
        self.data_reciever = Some(data_rx);
        let mut listen_id = rdma::server_init(&self.sock_addr).unwrap();
        let gpu_ordinal = self.gpu_ordinal;
        let gpu_buffers = self.local_buffer.iter().map(Into::into).collect::<Vec<GPUMemBuffer>>();
        let _ = thread::spawn(move || {
            let rt = Runtime::new().unwrap();
            rt.block_on(async {
                loop {
                    let data_tx = data_tx.clone();
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
                                            let req_id = notification.req_id.to_owned();
                                            info!("request: {} complete!", hex::encode(&notification.req_id));
                                            if let  Err(e) = data_tx.send(req_id).await {
                                                error!("request: {} completion notification error: {}", hex::encode(&notification.req_id), e.to_string())
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

    fn is_complete(&mut self) -> Option<Vec<u8>> {
        if let Some(rx) = &mut self.data_reciever {
            match rx.try_recv() {
                Ok(data) => Some(data),
                _ => None,
            }
        } else {
            None
        }
    }

    fn shutdown(&mut self) {
        if let Some(sender) = self.cmd_sender.take() {
            let _ = sender.send(Command::Complete());
        }
    }
}
