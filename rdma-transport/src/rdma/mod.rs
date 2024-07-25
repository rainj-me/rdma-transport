mod client;
mod server;

use rdma_core::{
    ibverbs::{ibv_poll_cq, IbvMr},
    rdma::{rdma_disconnect, rdma_post_send, rdma_post_write, RdmaCmId},
};

use rdma_core_sys::{ibv_wc, IBV_SEND_INLINE, IBV_SEND_SIGNALED, IBV_WC_SUCCESS};
pub use server::{accept, handle_notification, handshake, init as server_init};

pub use client::{connect, init as client_init};

use crate::{
    cuda::{cuda_mem_free, CudaMemBuffer},
    Result, TransportErrors,
};

pub fn deregister_mr(_mr: &mut IbvMr, buffer: &CudaMemBuffer) -> Result<()> {
    cuda_mem_free(&buffer).map_err(|e| e.into())
}

pub fn disconnect(cm_id: &mut RdmaCmId) -> Result<()> {
    rdma_disconnect(cm_id).map_err(|e| e.into())
}

#[derive(Debug, Clone, Copy)]
pub struct Connection {
    pub addr: u64,
    pub rkey: u32,
}

#[derive(Debug, Clone, Copy)]
pub struct Notification {
    pub size: usize,
    pub done: usize,
}

pub async fn send<T>(cm_id: &mut RdmaCmId, msg: &mut T) -> Result<()> {
    rdma_post_send(
        cm_id,
        None::<&mut u32>,
        msg,
        std::mem::size_of::<T>(),
        None,
        IBV_SEND_INLINE,
    )?;

    let mut wc = ibv_wc::default();
    let send_cq = cm_id.send_cq;
    ibv_poll_cq(send_cq, 1, &mut wc)?;

    if wc.status != IBV_WC_SUCCESS {
        return Err(TransportErrors::OpsFailed(
            "send".to_string(),
            format!("poll_send_comp failed with status: {:?}", wc.status),
        ));
    }
    Ok(())
}

pub async fn write(
    cm_id: &mut RdmaCmId,
    mr: &mut IbvMr,
    conn: &Connection,
    buffer: &mut CudaMemBuffer,
    size: usize,
) -> Result<()> {
    rdma_post_write(
        cm_id,
        Some(&mut 1),
        buffer.as_mut_ptr() as *mut std::ffi::c_void,
        size,
        Some(mr),
        IBV_SEND_SIGNALED,
        conn.addr,
        conn.rkey,
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

    let mut notification = Notification {
        size: size,
        done: 0,
    };

    rdma_post_send(
        cm_id,
        None::<&mut u32>,
        &mut notification,
        std::mem::size_of::<Notification>(),
        None,
        IBV_SEND_INLINE,
    )?;

    let mut wc = ibv_wc::default();
    let send_cq = cm_id.send_cq;
    ibv_poll_cq(send_cq, 1, &mut wc)?;

    if wc.status != IBV_WC_SUCCESS {
        return Err(TransportErrors::OpsFailed(
            "write".to_string(),
            format!("poll_send_comp failed with status: {:?}", wc.status),
        ));
    }

    Ok(())
}
