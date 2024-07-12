use rdma_core_sys::rdma_addrinfo;
use std::ffi::CString;

use crate::{RdmaErrors, Result};

pub fn rdma_getaddrinfo(
    node: &str,
    service: &str,
    hints: &rdma_addrinfo,
) -> Result<*mut rdma_addrinfo> {
    let mut addr_info: *mut rdma_addrinfo = &mut rdma_addrinfo::default();
    let node = CString::new(node).map_err(|_| RdmaErrors::InvalidAddress(node.to_string()))?;
    let service =
        CString::new(service).map_err(|_| RdmaErrors::InvalidAddress(service.to_string()))?;

    let ret = unsafe {
        rdma_core_sys::rdma_getaddrinfo(node.as_ptr(), service.as_ptr(), hints, &mut addr_info)
    };
    return if ret == 0 {
        Ok(addr_info)
    } else {
        Err(RdmaErrors::OpsFailed("rdma_getaddrinfo".to_string(), ret))
    };
}
