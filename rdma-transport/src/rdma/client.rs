use std::{collections::HashMap, net::SocketAddr, ops::DerefMut};

use os_socketaddr::OsSocketAddr;

use rdma_core::{
    ibverbs::{ibv_modify_qp, ibv_poll_cq, ibv_reg_mr, IbvMr, IbvQpInitAttr},
    rdma::{
        rdma_connect, rdma_create_ep, rdma_disconnect, rdma_getaddrinfo, rdma_post_recv,
        rdma_post_send, RdmaAddrInfo, RdmaCmId,
    },
};
use rdma_core_sys::{
    ibv_qp_attr, ibv_wc, ntohl, IBV_ACCESS_LOCAL_WRITE, IBV_ACCESS_REMOTE_READ,
    IBV_ACCESS_REMOTE_WRITE, IBV_QP_ACCESS_FLAGS, IBV_SEND_INLINE, IBV_WC_SUCCESS, RDMA_PS_TCP,
};

use crate::{
    buffer::GPU_BUFFER_SIZE,
    cuda::{cuda_device_primary_ctx_retain, cuda_mem_alloc, cuda_set_current_ctx},
    GPUMemBuffer, MemBuffer, Result, TransportErrors,
};

use super::{write_metadata, Connection, Connections, Notification};

// const BUFFER_SIZE: usize = 16 * 1024 * 1024;

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
    gpu_buffers: Vec<GPUMemBuffer>,
) -> Result<(
    Connection,
    (IbvMr, MemBuffer),
    HashMap<u64, (IbvMr, GPUMemBuffer)>,
    HashMap<u64, Connection>,
)> {
    let qp = cm_id.qp;
    let pd = cm_id.pd;

    let mut mod_attr = ibv_qp_attr::default();
    mod_attr.qp_access_flags = IBV_ACCESS_REMOTE_READ | IBV_ACCESS_REMOTE_WRITE;
    ibv_modify_qp(qp, &mut mod_attr, IBV_QP_ACCESS_FLAGS as i32)?;

    let access = IBV_ACCESS_LOCAL_WRITE | IBV_ACCESS_REMOTE_WRITE | IBV_ACCESS_REMOTE_READ;
    let mut cpu_buffer: MemBuffer = MemBuffer::default();
    let mut cpu_mr = ibv_reg_mr(pd, cpu_buffer.deref_mut(), access as i32)?;

    let mut cu_ctx = cuda_device_primary_ctx_retain(gpu_ordinal)?;
    cuda_set_current_ctx(&mut cu_ctx)?;

    let mut local_gpu_buffer_map: HashMap<u64, (IbvMr, GPUMemBuffer)> = HashMap::new();
    for mut buffer in gpu_buffers.into_iter() {
        let gpu_mr = ibv_reg_mr(pd, &mut buffer, IBV_ACCESS_LOCAL_WRITE as i32)?;
        local_gpu_buffer_map.insert(buffer.get_base_ptr(), (gpu_mr, buffer));
    }

    let server_conn = establish_conn(cm_id, &mut cpu_mr, &mut cpu_buffer)?;

    // the following blocks try to recv the GPU mem conn information from server side
    rdma_post_recv(
        cm_id,
        None::<&mut u32>,
        cpu_buffer.get_ptr(),
        cpu_buffer.get_size(),
        &mut cpu_mr,
    )?;

    let mut wc = ibv_wc::default();
    let recv_cq = cm_id.recv_cq;
    ibv_poll_cq(recv_cq, 1, &mut wc)?;
    if wc.status != IBV_WC_SUCCESS {
        return Err(TransportErrors::OpsFailed(
            "connect".to_string(),
            format!("poll_recv_comp failed with status: {:?}", wc.status),
        ));
    }

    if wc.opcode != rdma_core_sys::IBV_WC_RECV_RDMA_WITH_IMM {
        return Err(TransportErrors::OpsFailed(
            "connect".to_string(),
            "expect IBV_WR_RDMA_WRITE_WITH_IMM opcode".to_string(),
        ));
    }

    let imm_data = unsafe { ntohl(wc.__bindgen_anon_1.imm_data) };
    let size = imm_data as usize;
    let data = &cpu_buffer[0..size];
    let server_gpu_conns = bincode::deserialize::<Connections>(data)
        .map_err(|e| TransportErrors::OpsFailed("connect".to_string(), e.to_string()))?;
    let mut remote_gpu_conns_map: HashMap<u64, Connection> = HashMap::new();
    for conn in server_gpu_conns.into_iter() {
        remote_gpu_conns_map.insert(conn.base_ptr, conn.to_owned());
    }

    Ok((
        server_conn,
        (cpu_mr, cpu_buffer),
        local_gpu_buffer_map,
        remote_gpu_conns_map,
    ))
}

fn establish_conn(
    cm_id: &mut RdmaCmId,
    cpu_mr: &mut IbvMr,
    cpu_buffer: &mut MemBuffer,
) -> Result<Connection> {
    let mut server_conn = Connection::default();
    rdma_post_recv(
        cm_id,
        None::<&mut u32>,
        &mut server_conn as *mut _ as u64,
        std::mem::size_of::<Connection>(),
        cpu_mr,
    )?;

    rdma_connect(cm_id, None)?;

    let mut client_conn = Connection::new(cpu_buffer.get_ptr(), cpu_mr.rkey);
    rdma_post_send(
        cm_id,
        None::<&mut u32>,
        &mut client_conn,
        std::mem::size_of::<Connection>(),
        None,
        IBV_SEND_INLINE,
    )?;
    let mut wc = ibv_wc::default();
    let send_cq = cm_id.send_cq;
    ibv_poll_cq(send_cq, 1, &mut wc)?;
    if wc.status != IBV_WC_SUCCESS {
        return Err(TransportErrors::OpsFailed(
            "establish_conn".to_string(),
            format!("poll_send_comp failed with status: {:?}", wc.status),
        ));
    }
    let mut wc = ibv_wc::default();
    let recv_cq = cm_id.recv_cq;
    ibv_poll_cq(recv_cq, 1, &mut wc)?;
    if wc.status != IBV_WC_SUCCESS {
        return Err(TransportErrors::OpsFailed(
            "establish_conn".to_string(),
            format!("poll_recv_comp failed with status: {:?}", wc.status),
        ));
    }

    Ok(server_conn)
}

pub async fn disconnect(
    cm_id: &mut RdmaCmId,
    conn: &Connection,
    cpu_mr: &mut IbvMr,
    cpu_buffer: &mut MemBuffer,
) -> Result<()> {
    let notification = Notification::complete();
    let size = bincode::serialized_size(&notification).unwrap();
    bincode::serialize_into(cpu_buffer.deref_mut(), &notification)
        .map_err(|e| TransportErrors::OpsFailed("disconnect".to_string(), e.to_string()))?;

    write_metadata(cm_id, conn, cpu_mr, cpu_buffer, 0, size as u16).await?;
    rdma_disconnect(cm_id).map_err(|e| e.into())
}
