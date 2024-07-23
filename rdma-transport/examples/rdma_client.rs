use std::net::SocketAddr;
use std::time::Instant;

use anyhow::{anyhow, Result};

use rdma_core::rdma::rdma_disconnect;
use rdma_core::{
    ibverbs::ibv_poll_cq,
    rdma::{rdma_post_send, rdma_post_write},
};
use rdma_core_sys::{ibv_wc, IBV_SEND_INLINE, IBV_SEND_SIGNALED, IBV_WC_SUCCESS};
use rdma_transport::cuda::{cuda_host_to_device, cuda_init_ctx, cuda_mem_alloc, cuda_mem_free, cuda_set_current_ctx};
use rdma_transport::rdma::{connect, Notification};

const BUFFER_SIZE: usize = 16 * 1024 * 1024;

pub fn main() -> Result<()> {
    let server_addr = "192.168.14.224:23456".parse::<SocketAddr>()?;
    let local_addr = "192.168.14.224:23457".parse::<SocketAddr>()?;
    let msg_size = 1024 * 1024;
    let loops = 10000;

    let gpu_ordinal = 4;

    let mut cu_ctx = cuda_init_ctx(gpu_ordinal)?;
    cuda_set_current_ctx(&mut cu_ctx)?;
    let mut buffer = cuda_mem_alloc(BUFFER_SIZE)?;

    let (mut cm_id, mut mr, conn) = connect(server_addr, local_addr, &mut buffer)?;

    let msg = "Hello, RDMA! The voice echoed through the dimly lit control room. The array of monitors flickered to life, displaying a mesmerizing array of data streams, holographic charts, and real-time simulations. Sitting at the central console was Dr. Elara Hinton, a leading expert in quantum computing and neural networks.".as_bytes();

    cuda_host_to_device(msg, &buffer)?;

    let start = Instant::now();
    for _ in 0..loops {
        rdma_post_write(
            &mut cm_id,
            Some(&mut 1),
            buffer.as_mut_ptr() as *mut std::ffi::c_void,
            msg_size,
            Some(&mut mr),
            IBV_SEND_SIGNALED,
            conn.addr,
            conn.rkey,
        )?;

        let mut wc = ibv_wc::default();
        let send_cq = cm_id.send_cq;
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
            &mut cm_id,
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
            return Err(anyhow!(
                "ibv_poll_cq on send_cq failed with status: {:?}",
                wc.status
            ));
        }
    }
    let elapse = start.elapsed().as_millis();
    let bw = (msg_size as f32 * loops as f32 * 1000.0) / (elapse as f32 * 1024.0 * 1024.0);
    println!(
        "message size: {}, loops: {}, duration: {}, bw: {:.2} MB/s",
        msg_size, loops, elapse, bw
    );

    let mut notification = Notification { size: 0, done: 1 };

    rdma_post_send(
        &mut cm_id,
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
        return Err(anyhow!(
            "ibv_poll_cq on send_cq failed with status: {:?}",
            wc.status
        ));
    }

    rdma_disconnect(&mut cm_id)?;
    cuda_mem_free(&buffer)?;

    Ok(())
}
