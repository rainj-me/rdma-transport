mod client;
mod server;

use rdma_core::{
    ibverbs::{ibv_poll_cq, IbvMr},
    rdma::{rdma_post_write, rdma_post_write_with_opcode, RdmaCmId},
};
use serde::{Deserialize, Serialize};

use rdma_core_sys::{ibv_wc, IBV_SEND_SIGNALED, IBV_WC_SUCCESS};
pub use server::{
    accept, disconnect as server_disconnect, handle_notification, handshake, init as server_init,
};

pub use client::{connect, disconnect as client_disconnect, init as client_init};

use crate::{
    buffer::CPU_BUFFER_BASE_SIZE, cuda::cuda_mem_free, GPUMemBuffer, MemBuffer, Result,
    TransportErrors, GPU_BUFFER_BASE_SIZE,
};

pub fn free_gpu_membuffer(buffer: &GPUMemBuffer) -> Result<()> {
    cuda_mem_free(&buffer).map_err(|e| e.into())
}

#[derive(Debug, Clone, Copy)]
pub struct Connection {
    pub gpu_buffer_addr: u64,
    pub gpu_mr_rkey: u32,
    pub cpu_buffer_addr: u64,
    pub cpu_mr_rkey: u32,
}

impl Default for Connection {
    fn default() -> Self {
        Connection {
            gpu_buffer_addr: 0,
            gpu_mr_rkey: 0,
            cpu_buffer_addr: 0,
            cpu_mr_rkey: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub buffer: (u64, u32, u32),
    pub done: u32, // 1 is done 0 is data
    pub data: Vec<u8>,
}

impl Default for Notification {
    fn default() -> Self {
        Notification {
            done: 0,
            buffer: (0, 0, 0),
            data: Vec::new(),
        }
    }
}

impl Notification {
    pub fn complete() -> Self {
        let mut notification = Notification::default();
        notification.done = 1;
        notification
    }
}

pub async fn write_metadata(
    cm_id: &mut RdmaCmId,
    conn: &Connection,
    cpu_mr: &mut IbvMr,
    cpu_buffer: &mut MemBuffer,
    offset: u16,
    size: u16,
) -> Result<()> {
    let imm_data = ((offset as u32) << 16) + size as u32;
    rdma_post_write_with_opcode(
        cm_id,
        Some(&mut 1),
        cpu_buffer.get_ptr() + (offset as u64 * CPU_BUFFER_BASE_SIZE as u64),
        CPU_BUFFER_BASE_SIZE,
        Some(cpu_mr),
        IBV_SEND_SIGNALED,
        conn.cpu_buffer_addr + (offset as u64 * CPU_BUFFER_BASE_SIZE as u64),
        conn.cpu_mr_rkey,
        rdma_core_sys::IBV_WR_RDMA_WRITE_WITH_IMM,
        imm_data,
    )?;

    let mut wc = ibv_wc::default();
    let send_cq = cm_id.send_cq;
    ibv_poll_cq(send_cq, 1, &mut wc)?;

    if wc.status != IBV_WC_SUCCESS {
        return Err(TransportErrors::OpsFailed(
            "write_metadata".to_string(),
            format!("poll_write_comp failed with status: {:?}", wc.status),
        ));
    }

    Ok(())
}

pub async fn write(
    cm_id: &mut RdmaCmId,
    conn: &Connection,
    mr: &mut IbvMr,
    buffer: &mut GPUMemBuffer,
    offset: u32,
    size: u32,
) -> Result<()> {
    rdma_post_write(
        cm_id,
        Some(&mut 1),
        buffer.get_ptr() + (offset as u64 * GPU_BUFFER_BASE_SIZE as u64),
        size as usize,
        Some(mr),
        IBV_SEND_SIGNALED,
        conn.gpu_buffer_addr + (offset as u64 * GPU_BUFFER_BASE_SIZE as u64),
        conn.gpu_mr_rkey,
    )?;

    let mut wc = ibv_wc::default();
    let send_cq = cm_id.send_cq;
    ibv_poll_cq(send_cq, 1, &mut wc)?;

    if wc.status != IBV_WC_SUCCESS {
        return Err(TransportErrors::OpsFailed(
            "write".to_string(),
            format!("poll_write_comp failed with status: {:?}", wc.status),
        ));
    }

    Ok(())
}
