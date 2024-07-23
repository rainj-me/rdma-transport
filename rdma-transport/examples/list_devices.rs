use anyhow::{anyhow, Result};
use os_socketaddr::OsSocketAddr;
use rdma_core_sys::{
    ibv_context, ibv_device, ibv_get_device_list, rdma_bind_addr, rdma_cm_id,
    rdma_create_event_channel, rdma_create_id, rdma_destroy_event_channel, rdma_destroy_id,
    RDMA_PS_UDP,
};
use rdma_core_sys::{ibv_get_device_guid, ibv_get_device_name};
use std::{net::SocketAddr, ptr};

pub fn open_device_by_addr(addr: SocketAddr) -> Result<ibv_context> {
    let rdma_cm_channel = unsafe {
        let cm_channel = rdma_create_event_channel();
        if cm_channel == ptr::null_mut() {
            Err(anyhow!("rdma create event channle failed"))
        } else {
            Ok(cm_channel)
        }
    }?;

    let cm_id = unsafe {
        let context: *mut std::ffi::c_void = ptr::null_mut();
        let mut cm_id: *mut rdma_cm_id = &mut rdma_cm_id::default();
        let res = rdma_create_id(rdma_cm_channel, &mut cm_id, context, RDMA_PS_UDP);
        println!("channel fd: {:?}", (*(*cm_id).channel).fd);

        if res != 0 {
            rdma_destroy_event_channel(rdma_cm_channel);
            Err(anyhow!("rdma create id failed"))
        } else {
            Ok(cm_id)
        }
    }?;

    println!("channel fd: {:?}", unsafe { *(*cm_id).channel }.fd);

    let _ = unsafe {
        let mut sock_addr: OsSocketAddr = addr.into();
        println!("sock addr: {:?}", sock_addr);
        let res = rdma_bind_addr(cm_id, sock_addr.as_mut_ptr());
        if res != 0 {
            rdma_destroy_id(cm_id);
            rdma_destroy_event_channel(rdma_cm_channel);
            Err(anyhow!("rdma create id failed"))
        } else {
            Ok(())
        }
    }?;

    let context = unsafe { *(*cm_id).verbs };

    println!(
        "bind to rdma device name {:?} on {:?}",
        unsafe { *context.device }.name,
        addr
    );

    Ok(context)
}

pub fn list_devices() -> Vec<*mut ibv_device> {
    let mut res = Vec::new();
    let null_ptr: *mut i32 = ptr::null_mut();
    unsafe {
        let devices = ibv_get_device_list(null_ptr);
        let mut p = devices;
        while *p != ptr::null_mut() {
            res.push(*p);
            p = p.offset(1);
        }
    };
    return res;
}

pub fn main() -> Result<()> {
    let devices = list_devices();
    for device in devices {
        let name = unsafe { std::ffi::CStr::from_ptr(ibv_get_device_name(device)) };

        let guid: u64 = unsafe { ibv_get_device_guid(device) };

        println!(
            "device name: {:?}, device guid: {:x}",
            name.to_string_lossy(),
            guid.to_be()
        );
    }

    Ok(())
}
