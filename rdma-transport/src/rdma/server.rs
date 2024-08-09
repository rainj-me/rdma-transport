use std::collections::HashMap;
use std::net::SocketAddr;
use std::ops::DerefMut;

use libc::AI_PASSIVE;

use rdma_core::ibverbs::{IbvMr, IbvQpInitAttr};
use rdma_core::rdma::{rdma_disconnect, rdma_post_write_with_opcode, RdmaAddrInfo, RdmaCmId};
use rdma_core::{
    ibverbs::{ibv_modify_qp, ibv_poll_cq, ibv_query_qp, ibv_reg_mr},
    rdma::{
        rdma_accept, rdma_create_ep, rdma_get_request, rdma_getaddrinfo, rdma_listen,
        rdma_post_recv, rdma_post_send,
    },
};
use rdma_core_sys::{
    ibv_qp_attr, ibv_wc, ntohl, IBV_ACCESS_LOCAL_WRITE, IBV_ACCESS_REMOTE_READ,
    IBV_ACCESS_REMOTE_WRITE, IBV_QP_ACCESS_FLAGS, IBV_QP_CAP, IBV_SEND_INLINE, IBV_SEND_SIGNALED,
    IBV_WC_SUCCESS, RDMA_PS_TCP,
};

use crate::buffer::CPU_BUFFER_BASE_SIZE;
use crate::cuda::{cuda_device_primary_ctx_retain, cuda_set_current_ctx};
use crate::{GPUMemBuffer, MemBuffer, Result, TransportErrors};

use super::{Connection, Connections, Notification};

// const BUFFER_SIZE: usize = 16 * 1024 * 1024;

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

pub async fn listen(listen_id: &mut RdmaCmId) -> Result<RdmaCmId> {
    rdma_get_request(listen_id).map_err(Into::into)
}

pub async fn accept(
    cm_id: &mut RdmaCmId,
    gpu_ordinal: i32,
    gpu_buffers: Vec<GPUMemBuffer>,
) -> Result<(
    Connection,
    (IbvMr, MemBuffer),
    HashMap<u64, (IbvMr, GPUMemBuffer)>,
)> {
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
    let mut local_gpu_buffer_map: HashMap<u64, (IbvMr, GPUMemBuffer)> = HashMap::new();
    let mut conns = Connections::default();
    for mut buffer in gpu_buffers.into_iter() {
        let gpu_mr = ibv_reg_mr(pd, &mut buffer, access as i32)?;
        conns.add(Connection::new(buffer.get_base_ptr(), gpu_mr.rkey));
        local_gpu_buffer_map.insert(buffer.get_base_ptr(), (gpu_mr, buffer));
    }

    let client_conn = establish_conn(cm_id, &mut cpu_mr, &mut cpu_buffer)?;

    let size = bincode::serialized_size(&conns)
        .map_err(|e| TransportErrors::OpsFailed("accept".to_string(), e.to_string()))?;
    bincode::serialize_into(cpu_buffer.deref_mut(), &conns)
        .map_err(|e| TransportErrors::OpsFailed("accept".to_string(), e.to_string()))?;

    rdma_post_write_with_opcode(
        cm_id,
        Some(&mut 1),
        cpu_buffer.get_ptr(),
        CPU_BUFFER_BASE_SIZE,
        Some(&mut cpu_mr),
        IBV_SEND_SIGNALED,
        client_conn.get_base_ptr(),
        client_conn.get_mr_rkey(),
        rdma_core_sys::IBV_WR_RDMA_WRITE_WITH_IMM,
        size as u32,
    )?;

    let mut wc = ibv_wc::default();
    let send_cq = cm_id.send_cq;
    ibv_poll_cq(send_cq, 1, &mut wc)?;

    if wc.status != IBV_WC_SUCCESS {
        return Err(TransportErrors::OpsFailed(
            "accept".to_string(),
            format!("poll_write_comp failed with status: {:?}", wc.status),
        ));
    }

    Ok((client_conn, (cpu_mr, cpu_buffer), local_gpu_buffer_map))
}

fn establish_conn(
    cm_id: &mut RdmaCmId,
    cpu_mr: &mut IbvMr,
    cpu_buffer: &mut MemBuffer,
) -> Result<Connection> {
    let mut client_conn = Connection::default();
    rdma_post_recv(
        cm_id,
        None::<&mut u32>,
        &mut client_conn as *mut _ as u64,
        std::mem::size_of::<Connection>(),
        cpu_mr,
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
    let mut server_conn = Connection {
        base_ptr: cpu_buffer.get_ptr(),
        mr_rkey: cpu_mr.rkey,
    };
    rdma_post_send(
        cm_id,
        None::<&mut u32>,
        &mut server_conn,
        std::mem::size_of::<Connection>(),
        Some(cpu_mr),
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
    Ok(client_conn)
}

pub async fn handle_notification(
    cm_id: &mut RdmaCmId,
    cpu_mr: &mut IbvMr,
    cpu_buffer: &mut MemBuffer,
) -> Result<Notification> {
    rdma_post_recv(
        cm_id,
        None::<&mut u32>,
        cpu_buffer.get_ptr(),
        cpu_buffer.get_size(),
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
        let size = imm_data as usize;
        // println!("offset: {}, size: {}", offset, size);
        let notification =
            bincode::deserialize::<Notification>(&cpu_buffer[0..size]).map_err(|e| {
                TransportErrors::OpsFailed("handle_notification".to_string(), e.to_string())
            })?;
        return Ok(notification);
    }

    Ok(Notification::default())
}

pub fn disconnect(cm_id: &mut RdmaCmId) -> Result<()> {
    rdma_disconnect(cm_id).map_err(|e| e.into())
}
