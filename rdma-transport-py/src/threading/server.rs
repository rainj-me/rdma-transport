use log::{error, info};
use pyo3::prelude::*;
use rdma_transport::cuda::cuda_device_to_host;
use rdma_transport::rdma::Notification;
use rdma_transport::{cuda, rdma, GPUMemBuffer};
use std::net::SocketAddr;
use std::thread;
use std::time::Instant;
use tokio::runtime::Runtime;
use tokio::sync::{
    mpsc::{self, UnboundedReceiver},
    oneshot::{self, Sender},
};

use super::Message;

#[pyclass]
pub enum Command {
    Complete(),
}

#[pyclass]
pub struct RdmaServer {
    cmd_sender: Option<Sender<Command>>,
    data_reciever: Option<UnboundedReceiver<Message>>,
    sock_addr: SocketAddr,
    gpu_ordinal: i32,
}

pub fn print_buffer(gpu_buffer: &GPUMemBuffer, notification: &Notification) {
    println!("notification: {:?}", notification);
    let (_, offset, size) = notification.buffer;
    let mut data = Box::new([0; 1024 * 1024]);
    let device_buffer = GPUMemBuffer::new(gpu_buffer.get_ptr() + offset as u64, size as usize);
    cuda_device_to_host(&device_buffer, data.as_mut(), Some(size as usize)).unwrap();
    println!("data: {}", String::from_utf8_lossy(&data[..]));
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
            cmd_sender: None,
            data_reciever: None,
            sock_addr,
            gpu_ordinal,
        }
    }

    fn listen(&mut self) {
        let (cmd_tx, mut cmd_rx) = oneshot::channel::<Command>();
        let (data_tx, data_rx) = mpsc::unbounded_channel::<Message>();
        self.cmd_sender = Some(cmd_tx);
        self.data_reciever = Some(data_rx);
        let mut listen_id = rdma::server_init(&self.sock_addr).unwrap();
        let gpu_ordinal = self.gpu_ordinal;
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
                        Ok(mut cm_id) = rdma::accept(&mut listen_id) => {
                            info!("start qp handshake");
                            rt.spawn( async move {
                                match rdma::handshake(&mut cm_id, gpu_ordinal).await {
                                    Ok((_conn, (_gpu_mr, gpu_buffer), (mut cpu_mr, mut cpu_buffer))) =>loop {
                                        let notification = rdma::handle_notification(&mut cm_id, &mut cpu_mr, &mut cpu_buffer).await.unwrap();
                                        if notification.done > 0 {
                                            info!("notifcation: {:?}" , notification);
                                            rdma::free_gpu_membuffer(&gpu_buffer).unwrap();
                                            break;
                                        } else {
                                            // info!("notification: {:?}", notification);
                                            data_tx.send(notification.into()).unwrap();
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

    async fn recv_message(&mut self) -> Option<Message> {
        if let Some(rx) = &mut self.data_reciever {
            rx.recv().await
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
