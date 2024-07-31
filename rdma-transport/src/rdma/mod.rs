mod client;
mod server;

use std::{collections::HashMap, ops::Deref, slice::Iter};

use rdma_core::{
    ibverbs::{ibv_poll_cq, IbvMr},
    rdma::{rdma_post_write, rdma_post_write_with_opcode, RdmaCmId},
};
use serde::{Deserialize, Serialize};

use rdma_core_sys::{ibv_wc, IBV_SEND_SIGNALED, IBV_WC_SUCCESS};
pub use server::{
    listen, disconnect as server_disconnect, handle_notification, accept, init as server_init,
};

pub use client::{connect, disconnect as client_disconnect, init as client_init};

use crate::{
    buffer::CPU_BUFFER_BASE_SIZE, cuda::cuda_mem_free, GPUMemBuffer, MemBuffer, Result,
    TransportErrors, GPU_BUFFER_BASE_SIZE,
};

pub fn free_gpu_membuffer(buffer: &GPUMemBuffer) -> Result<()> {
    cuda_mem_free(&buffer).map_err(|e| e.into())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Connection {
    base_ptr: u64,
    mr_rkey: u32,
}

impl Connection {
    pub fn new(base_ptr: u64, mr_rkey: u32) -> Connection {
        Connection {
            base_ptr,
            mr_rkey
        }
    }

    pub fn get_base_ptr(&self) -> u64 {
        self.base_ptr
    }

    pub fn get_mr_rkey(&self) -> u32 {
        self.mr_rkey
    }
}



#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Connections {
    conns: Vec<Connection>,
}

impl Connections {
    pub fn add(&mut self, conn: Connection) {
        self.conns.push(conn);
    }
}

impl Deref for Connections {
    type Target = [Connection];
    fn deref(&self) -> &Self::Target {
        &self.conns
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Notification {
    pub buffer: (u64, u64, u32),
    pub done: u32, // 1 is done 0 is data
    pub req_id: Vec<u8>,
    pub remaining: u32,
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
        conn.get_base_ptr() + (offset as u64 * CPU_BUFFER_BASE_SIZE as u64),
        conn.get_mr_rkey(),
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
        buffer.get_base_ptr() + (offset as u64 * GPU_BUFFER_BASE_SIZE as u64),
        size as usize,
        Some(mr),
        IBV_SEND_SIGNALED,
        conn.get_base_ptr() + (offset as u64 * GPU_BUFFER_BASE_SIZE as u64),
        conn.get_mr_rkey(),
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
