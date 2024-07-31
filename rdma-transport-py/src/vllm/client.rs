use log::{error, info};
use pyo3::prelude::*;
use rdma_transport::rdma::{self, Notification};
use std::net::SocketAddr;
use std::ops::DerefMut;
use std::thread;
use std::time::Instant;
use tokio::runtime;
use tokio::sync::mpsc::{self, Sender};

use super::{TensorBlock, TensorBlocks};

pub enum Command {
    Send {
        local_tensor_block: TensorBlock,
        remote_tensor_block: TensorBlock,
        req_id: Vec<u8>,
        remaining: u32,
    },
    Complete(),
}

#[pyclass]
pub struct VllmRdmaClient {
    sender: Option<Sender<Command>>,
    gpu_ordinal: i32,
    local_addr: SocketAddr,
    local_buffer: TensorBlocks,
}

#[pymethods]
impl VllmRdmaClient {
    #[new]
    fn new(local_addr: String, gpu_ordinal: i32, local_buffer: TensorBlocks) -> Self {
        let local_addr = match local_addr.parse::<SocketAddr>() {
            Ok(sock_addr) => sock_addr,
            Err(e) => {
                error!("parse socket address failed: {:?}", e);
                panic!();
            }
        };

        VllmRdmaClient {
            sender: None,
            local_buffer,
            local_addr,
            gpu_ordinal,
        }
    }

    fn connect(&mut self, server_addr: String) -> TensorBlocks {
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
        let gpu_buffers = self.local_buffer.iter().map(Into::into).collect();
        let (cpu_conn, (mut cpu_mr, mut cpu_buffer), mut local_gpu_buffers, remote_gpu_buffers) =
            rdma::connect(&mut cm_id, gpu_ordinal, gpu_buffers).unwrap();
        // self.buffer = Some((gpu_buffer.get_base_ptr(), gpu_buffer.get_size()));
        // csy: We can associate a cuda event to this buffer, or each buffer.
        // info!("client gpu_buffer: {:?}", gpu_buffer);
        let tensor_blocks = remote_gpu_buffers.values().map(Into::into).collect::<Vec<TensorBlock>>().into();

        let _ = thread::spawn(move || {
            let rt = runtime::Builder::new_current_thread().build().unwrap();
            rt.block_on(async {
                while let Some(cmd) = rx.recv().await {
                    match cmd {
                        Command::Send { local_tensor_block, remote_tensor_block,  req_id, remaining } if local_tensor_block.get_size() > 0 => {
                            let notification = Notification {
                                done: 0,
                                buffer: (remote_tensor_block.get_base_ptr(), remote_tensor_block.get_offset(), local_tensor_block.get_size()),
                                req_id,
                                remaining,
                            };

                            let metadata_size = bincode::serialized_size(&notification).unwrap();
                            bincode::serialize_into(cpu_buffer.deref_mut(), &notification).unwrap();
                            // csy: We can wait on this event here or use cuLaunchHostFunc to enqueue the write routine
                            let conn = remote_gpu_buffers.get(&remote_tensor_block.get_base_ptr()).unwrap();
                            let (gpu_mr, gpu_buffer) = local_gpu_buffers.get_mut(&local_tensor_block.get_base_ptr()).unwrap();
                            if let Err(e) = rdma::write(
                                &mut cm_id,
                                conn,
                                gpu_mr,
                                local_tensor_block.get_base_ptr() + local_tensor_block.get_offset(),
                                conn.get_base_ptr() + remote_tensor_block.get_offset(),
                                local_tensor_block.get_size(),
                            )
                            .await
                            {
                                error!("write data error {:?}", e);
                            }
                            if let Err(e) = rdma::write_metadata(
                                &mut cm_id,
                                &cpu_conn,
                                &mut cpu_mr,
                                &mut cpu_buffer,
                                0,
                                metadata_size as u16,
                            )
                            .await
                            {
                                error!("write metadata error {:?}", e);
                            }
                        }
                        Command::Complete() => {
                            info!("complete");
                            rdma::client_disconnect(
                                &mut cm_id,
                                &cpu_conn,
                                &mut cpu_mr,
                                &mut cpu_buffer,
                            )
                            .await
                            .unwrap();
                            break;
                        }
                        _ => {}
                    }
                }
            });
            info!("runtime end at {:?}", Instant::now());
        });

        return tensor_blocks;
    }

    async fn send(&self, local_tensor_block: TensorBlock, remote_tensor_block:TensorBlock, req_id: Vec<u8>, remaining: u32) {
        if let Some(sender) = &self.sender {
            if let Err(e) = sender.send(Command::Send { local_tensor_block, remote_tensor_block, req_id, remaining }).await {
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
