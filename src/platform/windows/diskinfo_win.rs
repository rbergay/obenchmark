use std::ffi::c_void;
use std::fs::File;
use std::io::{Read};
use std::os::windows::fs::OpenOptionsExt;
use std::os::windows::io::AsRawHandle;
use std::ptr::null_mut;
use winapi::shared::ntdef::ULONG;
use winapi::shared::winerror::ERROR_INSUFFICIENT_BUFFER;
use winapi::um::fileapi::CreateFileA;
use winapi::um::handleapi::INVALID_HANDLE_VALUE;
use winapi::um::winbase::FILE_FLAG_BACKUP_SEMANTICS;
use winapi::um::winioctl::{
    IOCTL_STORAGE_QUERY_PROPERTY,
    STORAGE_PROPERTY_QUERY,
    StorageDeviceProperty,
    STORAGE_DEVICE_DESCRIPTOR,
};

#[derive(Debug, Clone)]
pub struct NativeDiskInfo {
    pub model: Option<String>,
    pub vendor: Option<String>,
    pub bus: Option<String>,
    pub disk_type: Option<String>,
}

/// Safe wrapper for IOCTL
unsafe fn query_device_property(handle: *mut c_void) -> Option<Vec<u8>> {
    let mut query = STORAGE_PROPERTY_QUERY {
        PropertyId: StorageDeviceProperty,
        QueryType: 0,
        AdditionalParameters: [0],
    };

    let mut out_buf = vec![0u8; 1024];
    let mut bytes_returned: ULONG = 0;

    let ok = winapi::um::ioapiset::DeviceIoControl(
        handle,
        IOCTL_STORAGE_QUERY_PROPERTY,
        &mut query as *mut _ as *mut c_void,
        std::mem::size_of::<STORAGE_PROPERTY_QUERY>() as u32,
        out_buf.as_mut_ptr() as *mut c_void,
        out_buf.len() as u32,
        &mut bytes_returned,
        null_mut(),
    );

    if ok == 0 {
        return None;
    }

    Some(out_buf)
}

/// Extract a C-string from offset inside STORAGE_DEVICE_DESCRIPTOR
fn extract_string(buf: &[u8], offset: u32) -> Option<String> {
    if offset == 0 || offset as usize >= buf.len() {
        return None;
    }
    let slice = &buf[offset as usize..];
    let end = slice.iter().position(|&c| c == 0)?;
    String::from_utf8(slice[..end].to_vec()).ok()
}

/// Determine actual disk type from BUS + Model
fn classify(bus: &str, model: &str) -> String {
    let b = bus.to_lowercase();
    let m = model.to_lowercase();

    if b.contains("nvme") || m.contains("nvme") {
        return "NVMe".into();
    }
    if b.contains("sata") {
        if m.contains("ssd") { return "SSD (SATA)".into(); }
        return "HDD (SATA)".into();
    }
    if b.contains("sas") { return "SAS".into(); }
    if b.contains("ide") { return "IDE".into(); }
    if b.contains("raid") { return "RAID".into(); }
    if b.contains("usb")  { return "USB".into(); }

    if m.contains("ssd") {
        return "SSD".into();
    }
    "HDD".into()
}

pub fn get_native_disk_info(drive_index: u32) -> Option<NativeDiskInfo> {
    let path = format!("\\\\.\\PhysicalDrive{}", drive_index);
    let handle = unsafe {
        CreateFileA(
            path.as_ptr() as *const i8,
            winapi::um::winnt::GENERIC_READ,
            winapi::um::winnt::FILE_SHARE_READ | winapi::um::winnt::FILE_SHARE_WRITE,
            null_mut(),
            winapi::um::fileapi::OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS,
            null_mut(),
        )
    };

    if handle == INVALID_HANDLE_VALUE {
        return None;
    }

    let data = unsafe { query_device_property(handle) }?;
    let desc = unsafe { &*(data.as_ptr() as *const STORAGE_DEVICE_DESCRIPTOR) };

    let vendor = extract_string(&data, desc.VendorIdOffset);
    let model = extract_string(&data, desc.ProductIdOffset);

    let bus_type = match desc.BusType {
        1  => "SCSI",
        2  => "ATAPI",
        3  => "ATA",
        4  => "IEEE1394",
        5  => "SSA",
        6  => "FIBRE",
        7  => "USB",
        8  => "RAID",
        9  => "iSCSI",
        10 => "SAS",
        11 => "SATA",
        17 => "NVMe",
        _  => "Unknown",
    }.to_string();

    let disk_type = classify(&bus_type, model.as_deref().unwrap_or(""));

    Some(NativeDiskInfo {
        model,
        vendor,
        bus: Some(bus_type),
        disk_type: Some(disk_type),
    })
}