use anyhow::Result;
use rdma_transport::devices::list_devices;
use rdma_core_sys::{ibv_get_device_guid, ibv_get_device_name};


pub fn main() -> Result<()> {
    let devices = list_devices();
    for device in devices {
        let name = unsafe {
            std::ffi::CStr::from_ptr(ibv_get_device_name(device))
        };
        
        let guid: u64 = unsafe {
            ibv_get_device_guid(device)
        };
        
        println!("device name: {:?}, device guid: {:x}", name.to_string_lossy(), guid.to_be());
    }

    Ok(())
}