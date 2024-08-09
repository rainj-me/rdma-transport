use log::{error, info};
use pyo3::prelude::*;
use rdma_transport::rdma::{self, Notification};
use std::net::SocketAddr;
use std::ops::DerefMut;
use std::sync::{Arc, RwLock};
use std::thread;
use std::time::Instant;
use tokio::runtime;
use tokio::sync::mpsc::{self, Sender};

use super::{CompletionReqs, TensorBlock, TensorBlocks};

pub enum Command {
    // Client send only works for push mode
    Send {
        local_tensor_block: TensorBlock,
        remote_tensor_block: TensorBlock,
    },
    // Client Recv only works for pull mode
    Recv {
        local_tensor_block: TensorBlock,
        remote_tensor_block: TensorBlock,
    },
    Complete {
        req_id: Vec<u8>,
    },
    Disconnect(),
}

#[pyclass]
pub struct VllmRdmaClient {
    sender: Option<Sender<Command>>,
    gpu_ordinal: i32,
    local_addr: SocketAddr,
    local_buffer: TensorBlocks,
    completion_reqs: Option<Arc<RwLock<CompletionReqs>>>,
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
            completion_reqs: None,
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

        let (tx, mut rx) = mpsc::channel(1024 * 1024 * 1024);
        self.sender = Some(tx);
        let completion_reqs = Arc::new(RwLock::new(CompletionReqs::new(1024)));
        self.completion_reqs = Some(completion_reqs.clone());
        let mut cm_id = rdma::client_init(server_addr, self.local_addr).unwrap();
        let gpu_ordinal = self.gpu_ordinal;
        let gpu_buffers = self.local_buffer.iter().map(Into::into).collect();
        let (cpu_conn, (mut cpu_mr, mut cpu_buffer), mut local_gpu_buffers, remote_gpu_buffers) =
            rdma::connect(&mut cm_id, gpu_ordinal, gpu_buffers).unwrap();
        // self.buffer = Some((gpu_buffer.get_base_ptr(), gpu_buffer.get_size()));
        // csy: We can associate a cuda event to this buffer, or each buffer.
        // info!("client gpu_buffer: {:?}", gpu_buffer);
        let tensor_blocks = remote_gpu_buffers
            .values()
            .map(Into::into)
            .collect::<Vec<TensorBlock>>()
            .into();

        let _ = thread::spawn(move || {
            let rt = runtime::Builder::new_current_thread().build().unwrap();
            rt.block_on(async {
                while let Some(cmd) = rx.recv().await {
                    match cmd {
                        Command::Complete { req_id } => {
                            let notification = Notification {
                                done: 0,
                                req_id: Some(req_id.clone()),
                            };

                            let metadata_size = bincode::serialized_size(&notification).unwrap();
                            bincode::serialize_into(cpu_buffer.deref_mut(), &notification).unwrap();

                            let mut reqs = completion_reqs.write().unwrap();
                            reqs.add_req(&req_id);
                            if reqs.is_full() {
                                reqs.remove_first();
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
                                error!("write complete message error {:?}", e);
                            }
                        }
                        Command::Send {
                            local_tensor_block,
                            remote_tensor_block,
                        } if local_tensor_block.get_size() > 0 => {
                            // csy: We can wait on this event here or use cuLaunchHostFunc to enqueue the write routine
                            let conn = remote_gpu_buffers
                                .get(&remote_tensor_block.get_base_ptr())
                                .unwrap();
                            let (gpu_mr, _) = local_gpu_buffers
                                .get_mut(&local_tensor_block.get_base_ptr())
                                .unwrap();
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
                        }
                        Command::Recv {
                            local_tensor_block,
                            remote_tensor_block,
                        } if local_tensor_block.get_size() > 0 => {
                            // csy: We can wait on this event here or use cuLaunchHostFunc to enqueue the write routine
                            let conn = remote_gpu_buffers
                                .get(&remote_tensor_block.get_base_ptr())
                                .unwrap();
                            let (gpu_mr, _) = local_gpu_buffers
                                .get_mut(&local_tensor_block.get_base_ptr())
                                .unwrap();
                            if let Err(e) = rdma::read(
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
                        }
                        Command::Disconnect() => {
                            info!("disconnect");
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

    fn send(&self, local_tensor_block: TensorBlock, remote_tensor_block: TensorBlock) {
        if let Some(sender) = &self.sender {
            if let Err(e) = sender.try_send(Command::Send {
                local_tensor_block,
                remote_tensor_block,
            }) {
                error!("send data msg error {:?}", e);
            }
        }
    }

    fn recv(&self, local_tensor_block: TensorBlock, remote_tensor_block: TensorBlock) {
        if let Some(sender) = &self.sender {
            if let Err(e) = sender.try_send(Command::Recv {
                local_tensor_block,
                remote_tensor_block,
            }) {
                error!("send data msg error {:?}", e);
            }
        }
    }

    fn complete(&self, req_id: Vec<u8>) {
        if let Some(sender) = &self.sender {
            if let Err(e) = sender.try_send(Command::Complete { req_id }) {
                error!("send complete msg error {:?}", e);
            }
        }
    }

    fn is_complete(&self, req_id: Vec<u8>) -> bool {
        if let Some(completion_reqs) = &self.completion_reqs {
            match completion_reqs.try_read() {
                Ok(reqs) => reqs.is_req_complete(&req_id),
                Err(_) => false,
            }
        } else {
            false
        }
    }

    async fn shutdown(&self) {
        if let Some(sender) = self.sender.as_ref() {
            if let Err(e) = sender.send(Command::Disconnect()).await {
                error!("shutdown error {:?}", e);
            }
        }
    }
}
