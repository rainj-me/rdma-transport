macro_rules! rdma_call {
    ($name: ident, $call:expr) => {
        {
            let ret = unsafe {$call};
            if ret == 0 {
                Ok(())
            } else {
                Err($crate::RdmaErrors::OpsFailed(stringify!($name).to_string(), ret))
            }
        }
    };
    ($name: ident, $call:expr, $ret_val: expr) => {
        {
            let ret = unsafe {$call};
            if ret == 0 {
                Ok($ret_val)
            } else {
                Err($crate::RdmaErrors::OpsFailed(stringify!($name).to_string(), ret))
            }
        }
    };
}

pub(crate)  use rdma_call;
