use std::net::SocketAddr;
use std::ops::DerefMut;

use libc::AI_PASSIVE;

use rdma_core::ibverbs::{IbvMr, IbvQpInitAttr};
use rdma_core::rdma::{rdma_disconnect, RdmaAddrInfo, RdmaCmId};
use rdma_core::{
    ibverbs::{ibv_modify_qp, ibv_poll_cq, ibv_query_qp, ibv_reg_mr},
    rdma::{
        rdma_accept, rdma_create_ep, rdma_get_request, rdma_getaddrinfo, rdma_listen,
        rdma_post_recv, rdma_post_send,
    },
};
use rdma_core_sys::{
    ibv_qp_attr, ibv_wc, ntohl, IBV_ACCESS_LOCAL_WRITE, IBV_ACCESS_REMOTE_READ,
    IBV_ACCESS_REMOTE_WRITE, IBV_QP_ACCESS_FLAGS, IBV_QP_CAP, IBV_SEND_INLINE, IBV_WC_SUCCESS,
    RDMA_PS_TCP,
};

use crate::buffer::CPU_BUFFER_BASE_SIZE;
use crate::cuda::{cuda_device_primary_ctx_retain, cuda_mem_alloc, cuda_set_current_ctx};
use crate::{GPUMemBuffer, MemBuffer, Result, TransportErrors, GPU_BUFFER_SIZE};

use super::{Connection, Notification};

pub fn init(bind_addr: &SocketAddr) -> Result<RdmaCmId> {
    let mut hints = RdmaAddrInfo::default();
    hints.ai_flags = AI_PASSIVE;
    hints.ai_port_space = RDMA_PS_TCP as i32;

    let mut addr_info = rdma_getaddrinfo(
        &bind_addr.ip().to_string(),
        &bind_addr.port().to_string(),
        &hints,
    )?;

    let mut qp_init_attr = IbvQpInitAttr::default();
    qp_init_attr.cap.max_send_wr = 1;
    qp_init_attr.cap.max_recv_wr = 1;
    qp_init_attr.cap.max_send_sge = 1;
    qp_init_attr.cap.max_recv_sge = 1;
    qp_init_attr.sq_sig_all = 1;

    let mut listen_id = rdma_create_ep(&mut addr_info, None, Some(&mut qp_init_attr))?;

    rdma_listen(&mut listen_id, 0)?;
    Ok(listen_id)
}

pub async fn accept(listen_id: &mut RdmaCmId) -> Result<RdmaCmId> {
    rdma_get_request(listen_id).map_err(|e| e.into())
}

pub async fn handshake(
    cm_id: &mut RdmaCmId,
    gpu_ordinal: i32,
) -> Result<(Connection, (IbvMr, GPUMemBuffer), (IbvMr, MemBuffer))> {
    let qp = cm_id.qp;
    ibv_query_qp(qp, &mut ibv_qp_attr::default(), IBV_QP_CAP as i32, None)?;

    let mut mod_attr = ibv_qp_attr::default();
    mod_attr.qp_access_flags = IBV_ACCESS_REMOTE_READ | IBV_ACCESS_REMOTE_WRITE;
    ibv_modify_qp(qp, &mut mod_attr, IBV_QP_ACCESS_FLAGS as i32)?;

    let access = IBV_ACCESS_LOCAL_WRITE | IBV_ACCESS_REMOTE_WRITE | IBV_ACCESS_REMOTE_READ;
    let pd = cm_id.pd;

    let mut cpu_buffer = MemBuffer::default();
    let mut cpu_mr = ibv_reg_mr(pd, cpu_buffer.deref_mut(), access as i32)?;

    let mut cu_ctx = cuda_device_primary_ctx_retain(gpu_ordinal)?;
    cuda_set_current_ctx(&mut cu_ctx)?;
    let mut gpu_buffer: GPUMemBuffer = cuda_mem_alloc(GPU_BUFFER_SIZE)?;
    let gpu_mr = ibv_reg_mr(pd, &mut gpu_buffer, access as i32)?;

    let mut conn_client_info = Connection::default();

    rdma_post_recv(
        cm_id,
        None::<&mut u32>,
        &mut conn_client_info as *mut _ as u64,
        std::mem::size_of::<Connection>(),
        &mut cpu_mr,
    )?;

    rdma_accept(cm_id, None)?;

    let mut wc = ibv_wc::default();
    let recv_cq = cm_id.recv_cq;
    ibv_poll_cq(recv_cq, 1, &mut wc)?;
    if wc.status != IBV_WC_SUCCESS {
        return Err(TransportErrors::OpsFailed(
            "accept".to_string(),
            format!("poll_recv_comp failed with status: {:?}", wc.status),
        ));
    }

    let mut conn_server_info = Connection {
        gpu_buffer_addr: gpu_buffer.get_ptr(),
        gpu_mr_rkey: gpu_mr.rkey,
        cpu_buffer_addr: cpu_buffer.get_ptr(),
        cpu_mr_rkey: cpu_mr.rkey,
    };

    rdma_post_send(
        cm_id,
        None::<&mut u32>,
        &mut conn_server_info,
        std::mem::size_of::<Connection>(),
        Some(&mut cpu_mr),
        IBV_SEND_INLINE,
    )?;

    let mut wc = ibv_wc::default();
    let send_cq = cm_id.send_cq;
    ibv_poll_cq(send_cq, 1, &mut wc)?;
    if wc.status != IBV_WC_SUCCESS {
        return Err(TransportErrors::OpsFailed(
            "accept".to_string(),
            format!("poll_send_comp failed with status: {:?}", wc.status),
        ));
    }

    Ok((conn_client_info, (gpu_mr, gpu_buffer), (cpu_mr, cpu_buffer)))
}

pub async fn handle_notification(
    cm_id: &mut RdmaCmId,
    cpu_mr: &mut IbvMr,
    cpu_buffer: &mut MemBuffer,
) -> Result<Notification> {
    // let mut notification = Notification::default();
    rdma_post_recv(
        cm_id,
        None::<&mut u32>,
        cpu_buffer.get_ptr(),
        cpu_buffer.get_capacity(),
        cpu_mr,
    )?;

    let mut wc = ibv_wc::default();
    let recv_cq = cm_id.recv_cq;
    ibv_poll_cq(recv_cq, 1, &mut wc)?;
    if wc.status != IBV_WC_SUCCESS {
        return Err(TransportErrors::OpsFailed(
            "handle_request".to_string(),
            format!("poll_recv_comp failed with status: {:?}", wc.status),
        ));
    }

    if wc.opcode == rdma_core_sys::IBV_WC_RECV_RDMA_WITH_IMM {
        let imm_data = unsafe { ntohl(wc.__bindgen_anon_1.imm_data) };
        let offset = (imm_data & 0xFFFF0000) >> 16;
        let size = (imm_data & 0x0000FFFF) as usize;
        let start = (offset as usize) * CPU_BUFFER_BASE_SIZE;
        let data = &cpu_buffer[start .. (start + size)];
        // println!("offset: {}, size: {}, start:{}, end: {}, data: {:?}", offset, size, start, start + size, &data[0..10] );
        let notification = bincode::deserialize::<Notification>(data).unwrap();
        return Ok(notification);
    }

    Ok(Notification::default())
}

pub fn disconnect(cm_id: &mut RdmaCmId) -> Result<()> {
    rdma_disconnect(cm_id).map_err(|e| e.into())
}
