use std::net::SocketAddr;

use os_socketaddr::OsSocketAddr;

use rdma_core::{
    ibverbs::{ibv_modify_qp, ibv_poll_cq, ibv_reg_mr, IbvMr, IbvQpInitAttr},
    rdma::{
        rdma_connect, rdma_create_ep, rdma_getaddrinfo, rdma_post_recv, rdma_post_send,
        RdmaAddrInfo, RdmaCmId,
    },
};
use rdma_core_sys::{
    ibv_qp_attr, ibv_wc, IBV_ACCESS_LOCAL_WRITE, IBV_ACCESS_REMOTE_READ, IBV_ACCESS_REMOTE_WRITE,
    IBV_QP_ACCESS_FLAGS, IBV_SEND_INLINE, IBV_WC_SUCCESS, RDMA_PS_TCP,
};

use crate::{
    cuda::{cuda_device_primary_ctx_retain, cuda_mem_alloc, cuda_set_current_ctx, CudaMemBuffer},
    Result, TransportErrors,
};

use super::Connection;

const BUFFER_SIZE: usize = 16 * 1024 * 1024;

pub fn init(server_addr: SocketAddr, local_addr: SocketAddr) -> Result<RdmaCmId> {
    let mut src_addr: OsSocketAddr = local_addr.into();

    let mut hints = RdmaAddrInfo::default();
    hints.ai_port_space = RDMA_PS_TCP as i32;
    hints.ai_src_addr = src_addr.as_mut_ptr();
    hints.ai_src_len = src_addr.len();

    let mut addr_info = rdma_getaddrinfo(
        &server_addr.ip().to_string(),
        &server_addr.port().to_string(),
        &hints,
    )?;

    let mut qp_init_attr = IbvQpInitAttr::default();
    qp_init_attr.cap.max_send_wr = 1;
    qp_init_attr.cap.max_recv_wr = 1;
    qp_init_attr.cap.max_send_sge = 1;
    qp_init_attr.cap.max_recv_sge = 1;
    qp_init_attr.cap.max_inline_data = 16;
    qp_init_attr.sq_sig_all = 1;
    let cm_id = rdma_create_ep(&mut addr_info, None, Some(&mut qp_init_attr))?;
    Ok(cm_id)
}

pub fn connect(
    cm_id: &mut RdmaCmId,
    gpu_ordinal: i32,
) -> Result<(IbvMr, CudaMemBuffer, Connection)> {
    let qp = cm_id.qp;
    let pd = cm_id.pd;

    let mut mod_attr = ibv_qp_attr::default();
    mod_attr.qp_access_flags = IBV_ACCESS_REMOTE_READ | IBV_ACCESS_REMOTE_WRITE;
    ibv_modify_qp(qp, &mut mod_attr, IBV_QP_ACCESS_FLAGS as i32)?;

    let mut cu_ctx = cuda_device_primary_ctx_retain(gpu_ordinal)?;
    cuda_set_current_ctx(&mut cu_ctx)?;
    let mut buffer: CudaMemBuffer = cuda_mem_alloc(BUFFER_SIZE)?;
    let mut mr = ibv_reg_mr(pd, &mut buffer, IBV_ACCESS_LOCAL_WRITE as i32)?;

    let mut conn_server_info = Connection { addr: 0, rkey: 0 };

    rdma_post_recv(
        cm_id,
        None::<&mut u32>,
        &mut conn_server_info,
        std::mem::size_of::<Connection>(),
        &mut mr,
    )?;

    rdma_connect(cm_id, None)?;

    let mut conn_client_info = Connection {
        addr: buffer.as_mut_ptr() as u64,
        rkey: mr.rkey,
    };

    rdma_post_send(
        cm_id,
        None::<&mut u32>,
        &mut conn_client_info,
        std::mem::size_of::<Connection>(),
        None,
        IBV_SEND_INLINE,
    )?;

    let mut wc = ibv_wc::default();
    let send_cq = cm_id.send_cq;
    ibv_poll_cq(send_cq, 1, &mut wc)?;

    if wc.status != IBV_WC_SUCCESS {
        return Err(TransportErrors::OpsFailed(
            "listen".to_string(),
            format!("poll_send_comp failed with status: {:?}", wc.status),
        ));
    }

    let mut wc = ibv_wc::default();
    let recv_cq = cm_id.recv_cq;
    ibv_poll_cq(recv_cq, 1, &mut wc)?;

    if wc.status != IBV_WC_SUCCESS {
        return Err(TransportErrors::OpsFailed(
            "listen".to_string(),
            format!("poll_recv_comp failed with status: {:?}", wc.status),
        ));
    }

    Ok((mr, buffer, conn_server_info))
}
