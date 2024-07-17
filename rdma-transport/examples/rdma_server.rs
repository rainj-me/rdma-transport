use core::slice;
use std::net::SocketAddr;
use std::ptr::null_mut;

use anyhow::{anyhow, Result};
use cuda::cuda_call;
use cuda_sys::{
    cuCtxCreate_v2, cuCtxSetCurrent, cuDeviceGet, cuInit, cuMemAlloc_v2, cuMemcpyDtoH_v2,
    CU_CTX_MAP_HOST,
};

use cuda_sys;
use libc::AI_PASSIVE;

use rdma_core::{
    ibverbs::{ibv_modify_qp, ibv_poll_cq, ibv_query_qp, ibv_reg_mr},
    rdma::{
        rdma_accept, rdma_create_ep, rdma_get_request, rdma_getaddrinfo, rdma_listen,
        rdma_post_recv, rdma_post_send,
    },
};
use rdma_core_sys::{
    ibv_qp_attr, ibv_qp_init_attr, ibv_wc, rdma_addrinfo, IBV_ACCESS_LOCAL_WRITE,
    IBV_ACCESS_REMOTE_READ, IBV_ACCESS_REMOTE_WRITE, IBV_QP_ACCESS_FLAGS, IBV_QP_CAP,
    IBV_SEND_INLINE, IBV_WC_SUCCESS, RDMA_PS_TCP,
};
use rdma_transport::rdma::{Connection, Notification, RdmaDev};

const BUFFER_SIZE: usize = 16 * 1024 * 1024;

pub fn main() -> Result<()> {
    let mut cu_dev = 0;
    let mut cu_ctx = null_mut();
    let gpu_ordinal = 0;
    let mut cu_mem_ptr: u64 = 0;
    let bind_addr  = "127.0.0.1:23456".parse::<SocketAddr>()?;

    cuda_call!(cuInit, cuInit(0))?;
    cuda_call!(cuDeviceGet, cuDeviceGet(&mut cu_dev, gpu_ordinal))?;
    cuda_call!(cuCtxCreate_v2, cuCtxCreate_v2(&mut cu_ctx, CU_CTX_MAP_HOST, cu_dev))?;
    cuda_call!(cuCtxSetCurrent, cuCtxSetCurrent(cu_ctx))?;
    cuda_call!(cuMemAlloc_v2, cuMemAlloc_v2(&mut cu_mem_ptr, BUFFER_SIZE))?;

    let mut buffer = unsafe { slice::from_raw_parts_mut(cu_mem_ptr as *mut u8, BUFFER_SIZE) };
    let mut rdma_dev = RdmaDev::default();
    let mut hints = rdma_addrinfo::default();
    hints.ai_flags = AI_PASSIVE;
    hints.ai_port_space = RDMA_PS_TCP as i32;

    let addr_info = rdma_getaddrinfo(&bind_addr.ip().to_string(), &bind_addr.port().to_string(), &hints)?;
    rdma_dev.addr_info = Some(addr_info);

    let mut qp_init_attr = ibv_qp_init_attr::default();
    qp_init_attr.cap.max_send_wr = 1;
    qp_init_attr.cap.max_recv_wr = 1;
    qp_init_attr.cap.max_send_sge = 1;
    qp_init_attr.cap.max_recv_sge = 1;
    qp_init_attr.sq_sig_all = 1;

    let listen_id = rdma_create_ep(addr_info, None, Some(&mut qp_init_attr))?;
    rdma_dev.listen_id = Some(listen_id);

    rdma_listen(listen_id, 0)?;

    let cm_id = rdma_get_request(listen_id)?;
    rdma_dev.cm_id = Some(cm_id);

    let qp = unsafe { (*cm_id).qp };
    ibv_query_qp(qp, &mut ibv_qp_attr::default(), IBV_QP_CAP as i32, None)?;

    let mut mod_attr = ibv_qp_attr::default();
    mod_attr.qp_access_flags = IBV_ACCESS_REMOTE_READ | IBV_ACCESS_REMOTE_WRITE;
    ibv_modify_qp(qp, &mut mod_attr, IBV_QP_ACCESS_FLAGS as i32)?;

    rdma_dev.send_flags = IBV_SEND_INLINE;

    let access = IBV_ACCESS_LOCAL_WRITE | IBV_ACCESS_REMOTE_WRITE | IBV_ACCESS_REMOTE_READ;
    let pd = unsafe { (*cm_id).pd };
    let mr = ibv_reg_mr(pd, &mut buffer, access as i32)?;
    rdma_dev.recv_mr = Some(mr);

    let mut client_conn = Connection {
        addr: 0 as u64,
        rkey: 0,
    };

    rdma_post_recv(
        cm_id,
        std::ptr::null_mut::<u32>(),
        &mut client_conn,
        std::mem::size_of::<Connection>(),
        rdma_dev.recv_mr.unwrap(),
    )?;

    rdma_accept(cm_id, None)?;

    let mut wc = ibv_wc::default();
    let recv_cq = unsafe { (*cm_id).recv_cq };
    ibv_poll_cq(recv_cq, 1, &mut wc)?;
    if wc.status != IBV_WC_SUCCESS {
        return Err(anyhow!("poll_send_comp failed with status: {:?}", wc.status));
    }

    let mut conn = Connection {
        addr: buffer.as_ptr() as u64,
        rkey: unsafe { (*mr).rkey },
    };

    rdma_post_send(
        cm_id,
        std::ptr::null_mut::<u32>(),
        &mut conn,
        std::mem::size_of::<Connection>(),
        rdma_dev.send_mr,
        rdma_dev.send_flags,
    )?;

    let mut wc = ibv_wc::default();
    let send_cq = unsafe { (*cm_id).send_cq };
    ibv_poll_cq(send_cq, 1, &mut wc)?;
    if wc.status != IBV_WC_SUCCESS {
        return Err(anyhow!("poll_send_comp failed with errorno: {:?}", wc.status));
    }

    let mut notification = Notification { size: 0, done: 0 };

    loop {
        rdma_post_recv(
            cm_id,
            std::ptr::null_mut::<u32>(),
            &mut notification,
            std::mem::size_of::<Notification>(),
            rdma_dev.recv_mr.unwrap(),
        )?;

        let mut wc = ibv_wc::default();
        let recv_cq = unsafe { (*cm_id).recv_cq };
        ibv_poll_cq(recv_cq, 1, &mut wc)?;
        if wc.status != IBV_WC_SUCCESS {
            return Err(anyhow!("poll_recv_comp failed with errorno: {:?}", wc.status));
        }
        if notification.done > 0 {
            break;
        }
    }

    let mut buffer_cpu: Vec<u8> = vec![0; BUFFER_SIZE];

    println!("before {:?}", &buffer_cpu[0..100]);

    cuda_call!(cuMemcpyDtoH_v2, cuMemcpyDtoH_v2(
        buffer_cpu.as_mut_ptr() as *mut std::ffi::c_void,
        buffer.as_mut_ptr() as u64,
        BUFFER_SIZE,
    ))?;

    println!("after {:?}", String::from_utf8_lossy(&buffer_cpu[0..100]));

    Ok(())
}
