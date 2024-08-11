macro_rules! rdma_call {
    ($name: ident, $call:expr) => {{
        let ret = unsafe { $call };
        if ret == 0 {
            Ok(())
        } else {
            let ret_val = unsafe {* libc::__errno_location() };
            Err($crate::RdmaErrors::OpsFailed(
                stringify!($name).to_string(),
                ret_val,
            ))
        }
    }};
    ($name: ident, $call:expr, $ret_val: expr) => {{
        let ret = unsafe { $call };
        if ret == 0 {
            Ok($ret_val)
        } else {
            let ret_val = unsafe {* libc::__errno_location() };
            Err($crate::RdmaErrors::OpsFailed(
                stringify!($name).to_string(),
                ret_val,
            ))
        }
    }};
}

pub(crate) use rdma_call;
