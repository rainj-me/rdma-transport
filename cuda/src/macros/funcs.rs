#[macro_export]
macro_rules! cuda_call {
    ($a:ident, $e:expr) => {
        {
            let ret = unsafe {$e};
            if ret == cuda_sys::CUDA_SUCCESS {
                Ok(())
            } else {
                Err($crate::CudaErrors::OpsFailed(stringify!($a).to_string(), ret))
            }

        }
    }
}
