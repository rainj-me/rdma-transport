use log::{error, info};
use pyo3::prelude::*;
use rdma_transport::cuda::cuda_device_to_host;
use rdma_transport::rdma::Notification;
use rdma_transport::{cuda, rdma, GPUMemBuffer};
use std::net::SocketAddr;
use std::thread;
use std::time::Instant;
use tokio::runtime::Runtime;
use tokio::sync::oneshot::{self, Sender};

#[pyclass]
pub enum Command {
    Complete(),
}

#[pyclass]
pub struct Message {
    buffer: (u32, u32),
    req_id: String,
    block_ids: Vec<u32>,
}

#[pyclass]
pub struct RdmaServer {
    sender: Option<Sender<Command>>,
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
            sender: None,
            sock_addr,
            gpu_ordinal,
        }
    }

    fn listen(&mut self) {
        let (tx, mut rx) = oneshot::channel::<Command>();
        self.sender = Some(tx);
        let mut listen_id = rdma::server_init(&self.sock_addr).unwrap();
        let gpu_ordinal = self.gpu_ordinal;
        let _ = thread::spawn(move || {
            let rt = Runtime::new().unwrap();
            rt.block_on(async {
                loop {
                    tokio::select! {
                        Ok(Command::Complete()) = (&mut rx) => {
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
                                            println!("notification: {:?}", notification);
                                            // print_buffer(&gpu_buffer, &notification);
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

    fn shutdown(&mut self) {
        if let Some(sender) = self.sender.take() {
            let _ = sender.send(Command::Complete());
        }
    }
}
