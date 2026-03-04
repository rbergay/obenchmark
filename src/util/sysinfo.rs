use sysinfo::System;
use std::fs;
use crate::model::result::{SystemInfo, CpuInfo, RamInfo, DiskInfo};
use std::process::Command;
use std::collections::HashMap;

#[cfg(windows)]
use wmi::{COMLibrary, WMIConnection};
#[cfg(windows)]
use serde::Deserialize;

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

    // CPU
    let mut cpu = CpuInfo::default();
    cpu.cores_logical = num_cpus::get();
    // Fallback cross-platform via sysinfo
    {
        let s = get_system_info();
        let g = s.global_cpu_info();
        if cpu.model.is_none() {
            let brand = g.brand().trim();
            if !brand.is_empty() {
                cpu.model = Some(brand.to_string());
            }
        }
        if cpu.vendor.is_none() {
            let v = g.vendor_id().trim();
            if !v.is_empty() {
                cpu.vendor = Some(v.to_string());
            }
        }
        let freq = g.frequency();
        if cpu.frequency_mhz.is_none() && freq > 0 {
            cpu.frequency_mhz = Some(freq as u64);
        }
    }
    // Linux enrichment via /proc/cpuinfo (vendor/model/frequency)
    #[cfg(target_os = "linux")]
    {
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
    }
    sysinfo.cpu = cpu;

    // RAM: total from sysinfo, modules/type best-effort
    let s = get_system_info();
    let total_mb = s.total_memory() / 1024;
    let total_readable = Some(if total_mb >= 1024 { format!("{:.2} GB", total_mb as f64 / 1024.0) } else { format!("{} MB", total_mb) });
    sysinfo.ram = RamInfo { total_mb, ram_type: None, modules: Vec::new(), total_readable };

    // Disks: always try cross-platform sysinfo first (mountpoint/size).
    let mut disks: Vec<DiskInfo> = Vec::new();
    {
        let sys_disks = sysinfo::Disks::new_with_refreshed_list();
        for d in sys_disks.list() {
            let name = d.name().to_string_lossy().to_string();
            let mount_point = Some(d.mount_point().to_string_lossy().to_string());
            let total_bytes = Some(d.total_space());
            disks.push(DiskInfo {
                name,
                vendor: None,
                model: None,
                disk_type: None,
                mount_point,
                total_bytes,
                size_readable: total_bytes.map(|b| human_bytes(b as f64)),
            });
        }
    }

    // Linux enrichment via /sys: vendor/model/rotational + better mapping to /dev names
    #[cfg(target_os = "linux")]
    {
        let mut sysfs_by_name: HashMap<String, (Option<String>, Option<String>, Option<String>)> = HashMap::new();
        if let Ok(entries) = fs::read_dir("/sys/block") {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                let vendor = fs::read_to_string(entry.path().join("device/vendor"))
                    .ok()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                let model = fs::read_to_string(entry.path().join("device/model"))
                    .ok()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty());
                let rotational = fs::read_to_string(entry.path().join("queue/rotational"))
                    .ok()
                    .map(|s| s.trim().to_string());
                let disk_type = match rotational.as_deref() {
                    Some("1") => Some("HDD".to_string()),
                    Some("0") => Some("SSD".to_string()),
                    _ => None,
                };
                sysfs_by_name.insert(name, (vendor, model, disk_type));
            }
        }

        for di in disks.iter_mut() {
            // sysinfo disk name can be like "sda1" or "nvme0n1p2" or similar; try to map to base block device
            let mut key = di.name.clone();
            // strip partition suffixes: sda1 -> sda, nvme0n1p2 -> nvme0n1
            if key.starts_with("nvme") {
                if let Some(pos) = key.rfind('p') {
                    // nvme0n1p2 => nvme0n1
                    if key[pos+1..].chars().all(|c| c.is_ascii_digit()) {
                        key.truncate(pos);
                    }
                }
            } else {
                while key.chars().last().is_some_and(|c| c.is_ascii_digit()) {
                    key.pop();
                }
            }

            if let Some((vendor, model, disk_type)) = sysfs_by_name.get(&key) {
                if di.vendor.is_none() { di.vendor = vendor.clone(); }
                if di.model.is_none() { di.model = model.clone(); }
                if di.disk_type.is_none() { di.disk_type = disk_type.clone(); }
                if di.disk_type.is_none() && key.starts_with("nvme") {
                    di.disk_type = Some("NVMe".to_string());
                }
            }
        }
    }

    // Windows enrichment via WMI: disk model/vendor/type + RAM type/modules
    #[cfg(windows)]
    {
        #[derive(Deserialize, Debug)]
        struct Win32DiskDrive {
            Model: Option<String>,
            Manufacturer: Option<String>,
            InterfaceType: Option<String>,
            MediaType: Option<String>,
        }

        #[derive(Deserialize, Debug)]
        struct Win32PhysicalMemory {
            Manufacturer: Option<String>,
            PartNumber: Option<String>,
            Capacity: Option<String>,
            SMBIOSMemoryType: Option<u16>,
            MemoryType: Option<u16>,
        }

        fn map_smbios_memory_type(code: u16) -> Option<&'static str> {
            // SMBIOSMemoryType codes (subset)
            match code {
                0 | 1 | 2 => None, // unknown/other
                18 => Some("DDR"),
                24 => Some("DDR3"),
                26 => Some("DDR4"),
                34 => Some("DDR5"),
                _ => None,
            }
        }

        fn normalize_disk_type(
            interface_type: Option<&str>,
            model: Option<&str>,
            media_type: Option<&str>,
        ) -> Option<String> {
            let iface = interface_type.unwrap_or("").to_lowercase();
            let m = model.unwrap_or("").to_lowercase();
            let media = media_type.unwrap_or("").to_lowercase();
            if iface.contains("nvme") || m.contains("nvme") {
                return Some("NVMe".to_string());
            }
            // WMI is often vague; best effort heuristics
            if media.contains("ssd") {
                return Some("SSD".to_string());
            }
            if media.contains("hdd") {
                return Some("HDD".to_string());
            }
            None
        }

        if let Ok(com) = COMLibrary::new() {
            if let Ok(wmi_con) = WMIConnection::new(com.into()) {
                // RAM modules
                if let Ok(mem_modules) = wmi_con.raw_query::<Win32PhysicalMemory>("SELECT Manufacturer, PartNumber, Capacity, SMBIOSMemoryType, MemoryType FROM Win32_PhysicalMemory") {
                    let mut type_hist: HashMap<String, u32> = HashMap::new();
                    for m in mem_modules {
                        let size_mb = m.Capacity
                            .as_deref()
                            .and_then(|s| s.parse::<u64>().ok())
                            .map(|b| b / (1024 * 1024));
                        let mt = m.SMBIOSMemoryType
                            .and_then(map_smbios_memory_type)
                            .or_else(|| m.MemoryType.and_then(map_smbios_memory_type))
                            .map(|s| s.to_string());
                        if let Some(t) = &mt {
                            *type_hist.entry(t.clone()).or_insert(0) += 1;
                        }
                        sysinfo.ram.modules.push(crate::model::result::MemoryModule {
                            vendor: m.Manufacturer.clone().filter(|s| !s.trim().is_empty()),
                            part_number: m.PartNumber.clone().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
                            size_mb,
                            memory_type: mt,
                        });
                    }
                    // ram_type = type le plus fréquent
                    if sysinfo.ram.ram_type.is_none() {
                        if let Some((t, _)) = type_hist.into_iter().max_by_key(|(_, c)| *c) {
                            sysinfo.ram.ram_type = Some(t);
                        }
                    }
                }

                // Disk drives (best-effort; mapping to volumes is non-trivial => on enrichit globalement)
                if let Ok(drives) = wmi_con.raw_query::<Win32DiskDrive>("SELECT Model, Manufacturer, InterfaceType, MediaType FROM Win32_DiskDrive") {
                    // Apply first non-empty details to unknown disk entries (simple heuristic)
                    let mut i = 0usize;
                    for d in drives {
                        let vendor = d.Manufacturer.clone().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
                        let model = d.Model.clone().map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
                        let disk_type = normalize_disk_type(
                            d.InterfaceType.as_deref(),
                            d.Model.as_deref(),
                            d.MediaType.as_deref(),
                        );
                        if let Some(di) = disks.get_mut(i) {
                            if di.vendor.is_none() { di.vendor = vendor.clone(); }
                            if di.model.is_none() { di.model = model.clone(); }
                            if di.disk_type.is_none() { di.disk_type = disk_type.clone(); }
                        }
                        i += 1;
                    }
                }
            }
        }
    }

    sysinfo.disks = disks;

    // Linux: dmidecode pour détails modules RAM + type (best-effort, peut nécessiter privilèges)
    #[cfg(target_os = "linux")]
    {
        if let Ok(output) = Command::new("dmidecode").arg("-t").arg("17").output() {
            if output.status.success() {
                if let Ok(txt) = String::from_utf8(output.stdout) {
                    let mut type_hist: HashMap<String, u32> = HashMap::new();
                    // parse Memory Device sections
                    let mut current: Option<crate::model::result::MemoryModule> = None;
                    for line in txt.lines() {
                        let l = line.trim();
                        if l.starts_with("Memory Device") {
                            if let Some(m) = current.take() {
                                if let Some(t) = &m.memory_type {
                                    *type_hist.entry(t.clone()).or_insert(0) += 1;
                                }
                                sysinfo.ram.modules.push(m);
                            }
                            current = Some(crate::model::result::MemoryModule::default());
                        } else if let Some(m) = current.as_mut() {
                            if l.starts_with("Manufacturer:") {
                                m.vendor = l.split(':').nth(1).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
                            } else if l.starts_with("Part Number:") {
                                m.part_number = l.split(':').nth(1).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
                            } else if l.starts_with("Type:") {
                                // "Type: DDR4" etc. (ignore "Type Detail")
                                if !l.starts_with("Type Detail:") {
                                    m.memory_type = l.split(':').nth(1).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
                                }
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
                        if let Some(t) = &m.memory_type {
                            *type_hist.entry(t.clone()).or_insert(0) += 1;
                        }
                        sysinfo.ram.modules.push(m);
                    }

                    if sysinfo.ram.ram_type.is_none() {
                        if let Some((t, _)) = type_hist.into_iter().max_by_key(|(_, c)| *c) {
                            sysinfo.ram.ram_type = Some(t);
                        }
                    }
                }
            }
        }
    }

    sysinfo
}