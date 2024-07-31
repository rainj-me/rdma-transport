use log::{error, info};
use pyo3::prelude::*;
use rdma_transport::{rdma::Notification, CPU_BUFFER_BASE_SIZE};
use rdma_transport::{cuda::cuda_host_to_device, rdma, GPUMemBuffer};
use std::net::SocketAddr;
use std::thread;
use std::time::Instant;
use tokio::runtime;
use tokio::sync::mpsc::{self, Sender};


#[pyclass]
pub enum Command {
    Send { offset: u32, size: u32, data: Vec<u8> },
    Complete(),
}

#[pyclass]
pub struct RdmaClient {
    sender: Option<Sender<Command>>,
    buffer: Option<(u64, usize)>,
    local_addr: SocketAddr,
    gpu_ordinal: i32,
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

        let (tx, mut rx) = mpsc::channel(1024);
        self.sender = Some(tx);
        let mut cm_id = rdma::client_init(server_addr, self.local_addr).unwrap();
        let gpu_ordinal = self.gpu_ordinal;
        let (conn, (mut gpu_mr, mut gpu_buffer), (mut cpu_mr, mut cpu_buffer)) =
            rdma::connect(&mut cm_id, gpu_ordinal).unwrap();
        self.buffer = Some((gpu_buffer.get_base_ptr(), gpu_buffer.get_size()));
        // csy: We can associate a cuda event to this buffer, or each buffer.
        info!("client gpu_buffer: {:?}", gpu_buffer);

        let _ = thread::spawn(move || {
            let rt = runtime::Builder::new_current_thread().build().unwrap();
            rt.block_on(async {
                while let Some(cmd) = rx.recv().await {
                    match cmd {
                        Command::Send { size, offset , data } if size > 0 => {
                            let notification = Notification {
                                done: 0,
                                buffer: (&mut gpu_buffer as *mut _ as u64, offset, size),
                                data,
                            };

                            let metadata_size = bincode::serialized_size(&notification).unwrap();
                            let start = offset as usize * CPU_BUFFER_BASE_SIZE;
                            let end = start + CPU_BUFFER_BASE_SIZE;
                            let buffer = &mut cpu_buffer[start..end];
                            bincode::serialize_into(buffer, &notification).unwrap();
                            // csy: We can wait on this event here or use cuLaunchHostFunc to enqueue the write routine
                            if let Err(e) = rdma::write(
                                &mut cm_id,
                                &conn,
                                &mut gpu_mr,
                                &mut gpu_buffer,
                                offset as u32,
                                size,
                            )
                            .await
                            {
                                error!("write data error {:?}", e);
                            }
                            if let  Err(e) = rdma::write_metadata(
                                &mut cm_id,
                                &conn,
                                &mut cpu_mr,
                                &mut cpu_buffer,
                                offset as u16,
                                metadata_size as u16,
                            )
                            .await{
                                error!("write metadata error {:?}", e);
                            }
                        }
                        Command::Complete() => {
                            info!("complete");
                            rdma::client_disconnect(
                                &mut cm_id,
                                &conn,
                                &mut cpu_mr,
                                &mut cpu_buffer,
                            )
                            .await
                            .unwrap();
                            rdma::free_gpu_membuffer(&gpu_buffer).unwrap();
                            break;
                        }
                        _ => {}
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
            let buffer = GPUMemBuffer::new(ptr, size);
            cuda_host_to_device(data.as_bytes(), &buffer).unwrap();
        }
    }

    async fn send(&self, offset: u32, size: u32, data: Vec<u8>) {
        if let Some(sender) = &self.sender {
            if let Err(e) = sender.send(Command::Send { offset, size, data }).await {
                error!("send error {:?}", e);
            }
        }
    }

    async fn shutdown(&self) {
        if let Some(sender) = self.sender.as_ref() {
            if let Err(e) = sender.send(Command::Complete()).await {
                error!("shutdown error {:?}", e);
            }
        }
    }
}
