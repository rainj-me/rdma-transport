use std::time::Instant;
use std::{net::SocketAddr, ptr::null_mut, slice};

use anyhow::{anyhow, Result};
use cuda::cuda_call;
use os_socketaddr::OsSocketAddr;

use cuda_sys::{
    cuCtxCreate_v2, cuCtxSetCurrent, cuDeviceGet, cuInit, cuMemAlloc_v2, cuMemcpyHtoD_v2,
    CU_CTX_MAP_HOST,
};

use rdma_core::{
    ibverbs::{ibv_modify_qp, ibv_poll_cq, ibv_reg_mr},
    rdma::{
        rdma_connect, rdma_create_ep, rdma_getaddrinfo, rdma_post_recv, rdma_post_send,
        rdma_post_write,
    },
};
use rdma_core_sys::{
    ibv_qp_attr, ibv_qp_init_attr, ibv_wc, rdma_addrinfo, IBV_ACCESS_LOCAL_WRITE,
    IBV_ACCESS_REMOTE_READ, IBV_ACCESS_REMOTE_WRITE, IBV_QP_ACCESS_FLAGS, IBV_SEND_INLINE,
    IBV_SEND_SIGNALED, IBV_WC_SUCCESS, RDMA_PS_TCP,
};
use rdma_transport::rdma::{Connection, Notification, RdmaDev};

const BUFFER_SIZE: usize = 16 * 1024 * 1024;

pub fn main() -> Result<()> {
    let mut cu_dev = 0;
    let mut cu_ctx = null_mut();
    let gpu_ordinal = 0;
    let mut cu_mem_ptr: u64 = 0;
    let remote_addr  = "127.0.0.1:23456".parse::<SocketAddr>()?;
    let local_addr = "127.0.0.2:23457".parse::<SocketAddr>()?;
    let msg_size = 1024 * 1024;
    let loops = 10000;

    cuda_call!(cuInit, cuInit(0))?;
    cuda_call!(cuDeviceGet, cuDeviceGet(&mut cu_dev, gpu_ordinal))?;
    cuda_call!(cuCtxCreate_v2, cuCtxCreate_v2(&mut cu_ctx, CU_CTX_MAP_HOST, cu_dev))?;
    cuda_call!(cuCtxSetCurrent, cuCtxSetCurrent(cu_ctx))?;
    cuda_call!(cuMemAlloc_v2, cuMemAlloc_v2(&mut cu_mem_ptr, BUFFER_SIZE))?;

    let mut buffer = unsafe { slice::from_raw_parts_mut(cu_mem_ptr as *mut u8, BUFFER_SIZE) };

    let mut src_addr: OsSocketAddr = local_addr.into();

    // let mut buffer: Vec<u8> = vec![0; BUFFER_SIZE];

    let mut rdma_dev = RdmaDev::default();
    let mut hints = rdma_addrinfo::default();
    hints.ai_port_space = RDMA_PS_TCP as i32;
    hints.ai_src_addr = src_addr.as_mut_ptr();
    hints.ai_src_len = src_addr.len();

    let addr_info = rdma_getaddrinfo(&remote_addr.ip().to_string(), &remote_addr.port().to_string(), &hints)?;
    rdma_dev.addr_info = Some(addr_info);

    let mut qp_init_attr = ibv_qp_init_attr::default();
    qp_init_attr.cap.max_send_wr = 1;
    qp_init_attr.cap.max_recv_wr = 1;
    qp_init_attr.cap.max_send_sge = 1;
    qp_init_attr.cap.max_recv_sge = 1;
    qp_init_attr.cap.max_inline_data = 16;
    qp_init_attr.sq_sig_all = 1;
    let cm_id = rdma_create_ep(addr_info, None, Some(&mut qp_init_attr))?;

    rdma_dev.cm_id = Some(cm_id);
    rdma_dev.send_flags = IBV_SEND_INLINE;
    let qp = unsafe { (*cm_id).qp };
    let pd = unsafe { (*cm_id).pd };

    let mut mod_attr = ibv_qp_attr::default();
    mod_attr.qp_access_flags = IBV_ACCESS_REMOTE_READ | IBV_ACCESS_REMOTE_WRITE;
    ibv_modify_qp(qp, &mut mod_attr, IBV_QP_ACCESS_FLAGS as i32)?;

    let mr = ibv_reg_mr(pd, &mut buffer, IBV_ACCESS_LOCAL_WRITE as i32)?;
    rdma_dev.recv_mr = Some(mr);

    let mut server_conn = Connection { addr: 0, rkey: 0 };

    rdma_post_recv(
        cm_id,
        std::ptr::null_mut::<u32>(),
        &mut server_conn,
        std::mem::size_of::<Connection>(),
        rdma_dev.recv_mr.unwrap(),
    )?;

    rdma_connect(cm_id, None)?;

    let mut conn = Connection {
        addr: buffer.as_mut_ptr() as u64,
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
        return Err(anyhow!(
            "ibv_poll_cq on send_cq failed with status: {:?}",
            wc.status
        ));
    }

    let mut wc = ibv_wc::default();
    let recv_cq = unsafe { (*cm_id).recv_cq };
    ibv_poll_cq(recv_cq, 1, &mut wc)?;

    if wc.status != IBV_WC_SUCCESS {
        return Err(anyhow!(
            "ibv_poll_cq on recv_cq failed with status: {:?}",
            wc.status
        ));
    }

    let msg = "Hello, RDMA! The voice echoed through the dimly lit control room. The array of monitors flickered to life, displaying a mesmerizing array of data streams, holographic charts, and real-time simulations. Sitting at the central console was Dr. Elara Hinton, a leading expert in quantum computing and neural networks.".as_bytes();

    cuda_call!(cuMemcpyHtoD_v2, cuMemcpyHtoD_v2(
        buffer.as_mut_ptr() as u64,
        msg.as_ptr() as *const std::ffi::c_void,
        msg.len(),
    ))?;

    let start = Instant::now();
    for _ in 0..loops {
        rdma_post_write(
            cm_id,
            &mut 1,
            buffer.as_mut_ptr() as *mut std::ffi::c_void,
            msg_size,
            rdma_dev.recv_mr,
            IBV_SEND_SIGNALED,
            server_conn.addr,
            server_conn.rkey,
        )?;

        let mut wc = ibv_wc::default();
        let send_cq = unsafe { (*cm_id).send_cq };
        ibv_poll_cq(send_cq, 1, &mut wc)?;

        if wc.status != IBV_WC_SUCCESS {
            return Err(anyhow!(
                "ibv_poll_cq on recv_cq failed with status: {:?}",
                wc.status
            ));
        }

        let mut notification = Notification {
            size: msg_size,
            done: 0,
        };

        rdma_post_send(
            cm_id,
            std::ptr::null_mut::<u32>(),
            &mut notification,
            std::mem::size_of::<Notification>(),
            rdma_dev.send_mr,
            rdma_dev.send_flags,
        )?;

        let mut wc = ibv_wc::default();
        let send_cq = unsafe { (*cm_id).send_cq };
        ibv_poll_cq(send_cq, 1, &mut wc)?;

        if wc.status != IBV_WC_SUCCESS {
            return Err(anyhow!(
                "ibv_poll_cq on send_cq failed with status: {:?}",
                wc.status
            ));
        }
    }
    let elapse = start.elapsed().as_millis();
    let bw = (msg_size as f32 * loops as f32 * 1000.0) / (elapse as f32 * 1024.0 * 1024.0 );
    print!(
        "message size: {}, loops: {}, duration: {}, bw: {:.2} MB/s",
        msg_size, loops, elapse, bw
    );

    let mut notification = Notification { size: 0, done: 1 };

    rdma_post_send(
        cm_id,
        std::ptr::null_mut::<u32>(),
        &mut notification,
        std::mem::size_of::<Notification>(),
        rdma_dev.send_mr,
        rdma_dev.send_flags,
    )?;

    let mut wc = ibv_wc::default();
    let send_cq = unsafe { (*cm_id).send_cq };
    ibv_poll_cq(send_cq, 1, &mut wc)?;

    if wc.status != IBV_WC_SUCCESS {
        return Err(anyhow!(
            "ibv_poll_cq on send_cq failed with status: {:?}",
            wc.status
        ));
    }

    Ok(())
}
