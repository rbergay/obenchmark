use sysinfo::System;
use std::fs;
use crate::model::result::{SystemInfo, CpuInfo, RamInfo, DiskInfo};

pub fn get_system_info() -> System {
    let mut sys = System::new_all();
    sys.refresh_all();
    sys
}

// Best-effort detailed system info for JSON export
pub fn get_detailed_system_info() -> SystemInfo {
    let mut sysinfo = SystemInfo::default();

    // CPU: try /proc/cpuinfo for vendor/model and frequency
    let mut cpu = CpuInfo::default();
    cpu.cores_logical = num_cpus::get();
    if let Ok(contents) = fs::read_to_string("/proc/cpuinfo") {
        for line in contents.lines() {
            if line.starts_with("vendor_id") && cpu.vendor.is_none() {
                cpu.vendor = line.split(':').nth(1).map(|s| s.trim().to_string());
            } else if line.starts_with("model name") && cpu.model.is_none() {
                cpu.model = line.split(':').nth(1).map(|s| s.trim().to_string());
            } else if line.starts_with("cpu MHz") && cpu.frequency_mhz.is_none() {
                if let Some(v) = line.split(':').nth(1) {
                    if let Ok(f) = v.trim().parse::<f64>() {
                        cpu.frequency_mhz = Some(f as u64);
                    }
                }
            }
        }
    }
    sysinfo.cpu = cpu;

    // RAM: total from sysinfo, modules best-effort left empty
    let s = get_system_info();
    let total_mb = s.total_memory() / 1024;
    sysinfo.ram = RamInfo { total_mb, modules: Vec::new() };

    // Disks: enumerate /sys/block and read vendor/model/size. Try to find mountpoint via /proc/mounts
    let mut disks = Vec::new();
    if let Ok(entries) = fs::read_dir("/sys/block") {
        // load mounts for quick lookup
        let mounts = fs::read_to_string("/proc/mounts").unwrap_or_default();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            // read vendor/model if available
            let vendor = fs::read_to_string(entry.path().join("device/vendor")).ok().map(|s| s.trim().to_string());
            let model = fs::read_to_string(entry.path().join("device/model")).ok().map(|s| s.trim().to_string());

            // try to read size (in 512-byte sectors) from sysfs
            let mut total_bytes = None;
            if let Ok(sz_str) = fs::read_to_string(entry.path().join("size")) {
                if let Ok(sectors) = sz_str.trim().parse::<u64>() {
                    total_bytes = Some(sectors.saturating_mul(512));
                }
            }

            // find mount point by scanning /proc/mounts for device name
            let mut mount = None;
            for line in mounts.lines() {
                if line.contains(&format!("/dev/{}", name)) {
                    if let Some(mp) = line.split_whitespace().nth(1) {
                        mount = Some(mp.to_string());
                        break;
                    }
                }
            }

            disks.push(DiskInfo {
                name: name.clone(),
                vendor,
                model,
                mount_point: mount,
                total_bytes,
            });
        }
    }
    sysinfo.disks = disks;

    sysinfo
}