use sysinfo::System;
use std::fs;
use crate::model::result::{SystemInfo, CpuInfo, RamInfo, DiskInfo};
use std::process::Command;

fn human_bytes(mut bytes: f64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut i = 0usize;
    while bytes >= 1024.0 && i < units.len() - 1 {
        bytes /= 1024.0;
        i += 1;
    }
    if i == 0 { format!("{} {}", bytes as u64, units[i]) } else { format!("{:.2} {}", bytes, units[i]) }
}

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
    let total_readable = Some(if total_mb >= 1024 { format!("{:.2} GB", total_mb as f64 / 1024.0) } else { format!("{} MB", total_mb) });
    sysinfo.ram = RamInfo { total_mb, modules: Vec::new(), total_readable };

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
                size_readable: total_bytes.map(|b| human_bytes(b as f64)),
            });
        }
    }
    sysinfo.disks = disks;

    // Try dmidecode for memory module details if available (best-effort, may require privileges)
    if let Ok(output) = Command::new("dmidecode").arg("-t").arg("17").output() {
        if output.status.success() {
            if let Ok(txt) = String::from_utf8(output.stdout) {
                // parse Memory Device sections
                let mut current: Option<crate::model::result::MemoryModule> = None;
                for line in txt.lines() {
                    let l = line.trim();
                    if l.starts_with("Memory Device") {
                        if let Some(m) = current.take() {
                            sysinfo.ram.modules.push(m);
                        }
                        current = Some(crate::model::result::MemoryModule::default());
                    } else if let Some(m) = current.as_mut() {
                        if l.starts_with("Manufacturer:") {
                            m.vendor = l.split(':').nth(1).map(|s| s.trim().to_string());
                        } else if l.starts_with("Part Number:") {
                            m.part_number = l.split(':').nth(1).map(|s| s.trim().to_string());
                        } else if l.starts_with("Size:") {
                            if let Some(sz) = l.split(':').nth(1) {
                                let s = sz.trim();
                                if s.ends_with("MB") {
                                    if let Ok(v) = s[..s.len()-2].trim().parse::<u64>() {
                                        m.size_mb = Some(v);
                                    }
                                } else if s.ends_with("GB") {
                                    if let Ok(v) = s[..s.len()-2].trim().parse::<u64>() {
                                        m.size_mb = Some(v * 1024);
                                    }
                                }
                            }
                        }
                    }
                }
                if let Some(m) = current.take() {
                    sysinfo.ram.modules.push(m);
                }
            }
        }
    }

    sysinfo
}