use sysinfo::{System, Disks};
use std::collections::HashMap;
use crate::model::result::{SystemInfo, CpuInfo, RamInfo, DiskInfo};

fn human_bytes(mut bytes: f64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut i = 0usize;
    while bytes >= 1024.0 && i < units.len() - 1 {
        bytes /= 1024.0;
        i += 1;
    }
    if i == 0 { format!("{} {}", bytes as u64, units[i]) }
    else { format!("{:.2} {}", bytes, units[i]) }
}

pub fn get_system_info() -> System {
    let mut sys = System::new_all();
    sys.refresh_all();
    sys
}

pub fn get_detailed_system_info() -> SystemInfo {
    let mut sysinfo = SystemInfo::default();

    // ================= CPU MULTI-OS =================
    {
        let s = get_system_info();
        let g = s.global_cpu_info();
        let mut cpu = CpuInfo::default();

        cpu.cores_logical = num_cpus::get();

        let brand = g.brand().trim();
        if !brand.is_empty() { cpu.model = Some(brand.to_string()); }
        let vendor = g.vendor_id().trim();
        if !vendor.is_empty() { cpu.vendor = Some(vendor.to_string()); }
        let freq = g.frequency();
        if freq > 0 { cpu.frequency_mhz = Some(freq as u64); }

        #[cfg(target_os = "windows")]
        {
            use serde::Deserialize;
            use std::process::Command;
            use wmi::WMIConnection;
            #[derive(Deserialize)]
            struct Win32Processor {
                Name: Option<String>, Manufacturer: Option<String>,
                NumberOfLogicalProcessors: Option<u32>, MaxClockSpeed: Option<u32>,
            }
            if let Ok(wmi) = WMIConnection::new() {
                if let Ok(mut rows) = wmi.raw_query::<Win32Processor>(
                    "SELECT Name, Manufacturer, NumberOfLogicalProcessors, MaxClockSpeed FROM Win32_Processor"
                ) {
                    if let Some(p) = rows.pop() {
                        if cpu.model.is_none() { cpu.model = p.Name; }
                        if cpu.vendor.is_none() { cpu.vendor = p.Manufacturer; }
                        if let Some(n) = p.NumberOfLogicalProcessors { cpu.cores_logical = n as usize; }
                        if let Some(m) = p.MaxClockSpeed { cpu.frequency_mhz = Some(m as u64); }
                    }
                }
            }
            if cpu.frequency_mhz.is_none() {
                let cmd = "(Get-CimInstance Win32_Processor).MaxClockSpeed";
                if let Ok(out) = Command::new("powershell").args(["-NoProfile", "-Command", cmd]).output() {
                    if out.status.success() {
                        if let Ok(v) = String::from_utf8(out.stdout) {
                            if let Ok(mhz) = v.trim().parse::<u64>() { cpu.frequency_mhz = Some(mhz); }
                        }
                    }
                }
            }
        }

        #[cfg(target_os = "linux")]
        {
            use std::fs;
            if let Ok(txt) = fs::read_to_string("/proc/cpuinfo") {
                let mut model = None;
                let mut vendor = None;
                for line in txt.lines() {
                    if model.is_none() && line.starts_with("model name") {
                        if let Some(v) = line.split(':').nth(1) { model = Some(v.trim().to_string()); }
                    }
                    if vendor.is_none() && line.starts_with("vendor_id") {
                        if let Some(v) = line.split(':').nth(1) { vendor = Some(v.trim().to_string()); }
                    }
                    if model.is_some() && vendor.is_some() { break; }
                }
                if cpu.model.is_none() { cpu.model = model; }
                if cpu.vendor.is_none() { cpu.vendor = vendor; }
            }
            if cpu.frequency_mhz.is_none() {
                if let Ok(txt) = fs::read_to_string("/sys/devices/system/cpu/cpu0/cpufreq/cpuinfo_max_freq") {
                    if let Ok(khz) = txt.trim().parse::<u64>() { cpu.frequency_mhz = Some(khz / 1000); }
                }
            }
            if cpu.frequency_mhz.is_none() {
                if let Ok(txt) = fs::read_to_string("/proc/cpuinfo") {
                    for line in txt.lines() {
                        if line.starts_with("cpu MHz") {
                            if let Some(v) = line.split(':').nth(1) {
                                if let Ok(mhz) = v.trim().parse::<f64>() {
                                    cpu.frequency_mhz = Some(mhz.round() as u64);
                                    break;
                                }
                            }
                        }
                    }
                }
            }
        }

        #[cfg(target_os = "macos")]
        {
            use std::process::Command;
            if cpu.model.is_none() {
                if let Ok(o) = Command::new("sysctl").args(["-n", "machdep.cpu.brand_string"]).output() {
                    if o.status.success() {
                        if let Ok(s) = String::from_utf8(o.stdout) { cpu.model = Some(s.trim().to_string()); }
                    }
                }
            }
            if cpu.vendor.is_none() {
                if let Ok(o) = Command::new("sysctl").args(["-n", "machdep.cpu.vendor"]).output() {
                    if o.status.success() {
                        if let Ok(s) = String::from_utf8(o.stdout) { cpu.vendor = Some(s.trim().to_string()); }
                    }
                }
            }
            if let Ok(o) = Command::new("sysctl").args(["-n", "hw.ncpu"]).output() {
                if o.status.success() {
                    if let Ok(s) = String::from_utf8(o.stdout) {
                        if let Ok(n) = s.trim().parse::<usize>() { cpu.cores_logical = n; }
                    }
                }
            }
            if cpu.frequency_mhz.is_none() {
                if let Ok(o) = Command::new("sysctl").args(["-n", "hw.cpufrequency_max"]).output() {
                    if o.status.success() {
                        if let Ok(s) = String::from_utf8(o.stdout) {
                            if let Ok(hz) = s.trim().parse::<u64>() { cpu.frequency_mhz = Some(hz / 1_000_000); }
                        }
                    }
                }
            }
        }

        sysinfo.cpu = cpu;
    }

    // ================= RAM MULTI-OS =================
    {
        let s = get_system_info();
        let mut ram = RamInfo::default();
        let total_kb = s.total_memory();
        let mut total_mb = total_kb / 1024;
        ram.total_mb = total_mb;
        ram.total_readable = Some(if total_mb >= 1024 {
            format!("{:.2} GB", total_mb as f64 / 1024.0)
        } else {
            format!("{} MB", total_mb)
        });

        #[cfg(target_os = "windows")]
        {
            use serde::Deserialize;
            use std::process::Command;
            use wmi::WMIConnection;
            #[derive(Deserialize)]
            struct Win32ComputerSystem { TotalPhysicalMemory: Option<String> }
            if let Ok(wmi) = WMIConnection::new() {
                if let Ok(mut rows) = wmi.raw_query::<Win32ComputerSystem>(
                    "SELECT TotalPhysicalMemory FROM Win32_ComputerSystem"
                ) {
                    if let Some(r) = rows.pop() {
                        if let Some(v) = r.TotalPhysicalMemory {
                            if let Ok(bytes) = v.trim().parse::<u64>() {
                                total_mb = (bytes / 1024) / 1024;
                            }
                        }
                    }
                }
            }
            if total_mb == 0 {
                let cmd = "(Get-CimInstance Win32_PhysicalMemory | Measure-Object Capacity -Sum).Sum";
                if let Ok(out) = Command::new("powershell").args(["-NoProfile", "-Command", cmd]).output() {
                    if out.status.success() {
                        if let Ok(v) = String::from_utf8(out.stdout) {
                            if let Ok(bytes) = v.trim().parse::<u64>() {
                                total_mb = (bytes / 1024) / 1024;
                            }
                        }
                    }
                }
            }
            ram.total_mb = total_mb;
            ram.total_readable = Some(if total_mb >= 1024 {
                format!("{:.2} GB", total_mb as f64 / 1024.0)
            } else {
                format!("{} MB", total_mb)
            });
        }

        #[cfg(target_os = "linux")]
        {
            use std::fs;
            if let Ok(txt) = fs::read_to_string("/proc/meminfo") {
                for line in txt.lines() {
                    if line.starts_with("MemTotal:") {
                        if let Some(kb) = line.split_whitespace().nth(1)
                            .and_then(|v| v.parse::<u64>().ok())
                        {
                            total_mb = kb / 1024;
                        }
                        break;
                    }
                }
            }
            ram.total_mb = total_mb;
            ram.total_readable = Some(if total_mb >= 1024 {
                format!("{:.2} GB", total_mb as f64 / 1024.0)
            } else {
                format!("{} MB", total_mb)
            });
        }

        #[cfg(target_os = "macos")]
        {
            use std::process::Command;
            if let Ok(o) = Command::new("sysctl").args(["-n", "hw.memsize"]).output() {
                if o.status.success() {
                    if let Ok(s) = String::from_utf8(o.stdout) {
                        if let Ok(bytes) = s.trim().parse::<u64>() {
                            total_mb = (bytes / 1024) / 1024;
                        }
                    }
                }
            }
            ram.total_mb = total_mb;
            ram.total_readable = Some(if total_mb >= 1024 {
                format!("{:.2} GB", total_mb as f64 / 1024.0)
            } else {
                format!("{} MB", total_mb)
            });
        }

        sysinfo.ram = ram;
    }

    // ================= DISKS (FULL ORIGINAL SECTION) =================

    let mut disks: Vec<DiskInfo> = {
        let sd = Disks::new_with_refreshed_list();
        sd.list()
            .iter()
            .map(|d| {
                let total = d.total_space();
                DiskInfo {
                    name: d.name().to_string_lossy().to_string(),
                    mount_point: Some(d.mount_point().to_string_lossy().to_string()),
                    total_bytes: Some(total),
                    size_readable: Some(human_bytes(total as f64)),
                    vendor: None,
                    model: None,
                    disk_type: None,
                }
            })
            .collect()
    };

    // ---------------- WINDOWS WMI + PowerShell ----------------
    #[cfg(target_os = "windows")]
    {
        use serde::Deserialize;
        use wmi::WMIConnection;
        #[derive(Deserialize, Debug, Clone)]
        struct Win32DiskDrive {
            DeviceID: Option<String>,
            Model: Option<String>,
            Manufacturer: Option<String>,
            InterfaceType: Option<String>,
            MediaType: Option<String>,
            SerialNumber: Option<String>,
            Size: Option<u64>,
        }
        #[derive(Deserialize, Debug)]
        struct Win32DiskDriveToDiskPartition { Antecedent: String, Dependent: String }
        #[derive(Deserialize, Debug)]
        struct Win32LogicalDiskToPartition { Antecedent: String, Dependent: String }

        fn wmi_get(refstr: &str, key: &str) -> Option<String> {
            let needle = format!("{}=\"", key);
            let idx = refstr.find(&needle)? + needle.len();
            let rem = &refstr[idx..];
            let end = rem.find('"')?;
            Some(rem[..end].to_string())
        }
        fn get_device_id(s: &str) -> Option<String> { wmi_get(s, "DeviceID") }

        fn classify_from_wmi(iface: Option<&str>, model: Option<&str>, media: Option<&str>) -> Option<String> {
            let i = iface.unwrap_or("").to_lowercase();
            let m = model.unwrap_or("").to_lowercase();
            let md = media.unwrap_or("").to_lowercase();
            if m.contains("nvme") { return Some("NVMe".into()); }
            if md.contains("ssd") || m.contains("ssd") {
                if i.contains("sata") { return Some("SSD (SATA)".into()); }
                if i.contains("scsi") { return Some("SSD (SCSI mode)".into()); }
                return Some("SSD".into());
            }
            if md.contains("hdd") || md.contains("fixed") || m.contains("hdd") {
                if i.contains("sata") { return Some("HDD (SATA)".into()); }
                if i.contains("sas") { return Some("HDD (SAS)".into()); }
                if i.contains("ide") { return Some("HDD (IDE)".into()); }
                return Some("HDD".into());
            }
            if i.contains("sata") { return Some("SATA".into()); }
            if i.contains("sas") { return Some("SAS".into()); }
            if i.contains("ide") { return Some("IDE".into()); }
            if i.contains("raid") { return Some("RAID".into()); }
            if i.contains("usb") { return Some("USB".into()); }
            None
        }

        let wmi = WMIConnection::new().ok();
        let mut partition_to_letter: HashMap<String, String> = HashMap::new();
        if let Some(ref w) = wmi {
            if let Ok(links) = w.raw_query::<Win32LogicalDiskToPartition>(
                "SELECT Antecedent, Dependent FROM Win32_LogicalDiskToPartition"
            ) {
                for l in links {
                    if let (Some(letter), Some(part)) = (get_device_id(&l.Antecedent), get_device_id(&l.Dependent)) {
                        partition_to_letter.insert(part, letter);
                    }
                }
            }
        }

        let mut physical_to_partitions: HashMap<String, Vec<String>> = HashMap::new();
        if let Some(ref w) = wmi {
            if let Ok(links) = w.raw_query::<Win32DiskDriveToDiskPartition>(
                "SELECT Antecedent, Dependent FROM Win32_DiskDriveToDiskPartition"
            ) {
                for l in links {
                    if let (Some(phys), Some(part)) = (get_device_id(&l.Antecedent), get_device_id(&l.Dependent)) {
                        physical_to_partitions.entry(phys).or_default().push(part);
                    }
                }
            }
        }

        let mut letter_to_idx: HashMap<String, usize> = HashMap::new();
        for (i, d) in disks.iter().enumerate() {
            if let Some(mp) = &d.mount_point {
                if mp.len() >= 2 {
                    let letter = mp[..2].to_uppercase();
                    if letter.ends_with(':') { letter_to_idx.insert(letter, i); }
                }
            }
        }

        let mut wmi_drives: Vec<Win32DiskDrive> = vec![];
        if let Some(ref w) = wmi {
            if let Ok(drives) = w.raw_query::<Win32DiskDrive>(
                "SELECT DeviceID, Model, Manufacturer, InterfaceType, MediaType, SerialNumber, Size FROM Win32_DiskDrive"
            ) {
                wmi_drives = drives;
                for dd in &wmi_drives {
                    let phys = dd.DeviceID.as_deref().unwrap_or("").trim();
                    if phys.is_empty() { continue; }
                    let parts = physical_to_partitions.get(phys).cloned().unwrap_or_default();
                    let mut letters = Vec::<String>::new();
                    for p in parts {
                        if let Some(letter) = partition_to_letter.get(&p) {
                            letters.push(letter.to_uppercase());
                        }
                    }
                    letters.sort();
                    letters.dedup();
                    let disk_type = classify_from_wmi(
                        dd.InterfaceType.as_deref(), dd.Model.as_deref(), dd.MediaType.as_deref(),
                    );
                    for letter in letters {
                        if let Some(&idx) = letter_to_idx.get(&letter) {
                            let di = &mut disks[idx];
                            if di.vendor.is_none() { di.vendor = dd.Manufacturer.clone(); }
                            if di.model.is_none() { di.model = dd.Model.clone(); }
                            if di.disk_type.is_none() { di.disk_type = disk_type.clone(); }
                        }
                    }
                }
            }
        }

        let needs_fallback = disks.iter().any(|d| d.disk_type.is_none());
        if needs_fallback {
            #[derive(Deserialize, Debug, Clone)]
            struct PsDisk { SerialNumber: Option<String>, FriendlyName: Option<String>, BusType: Option<String>, MediaType: Option<String> }
            fn ps_get_physical_disks() -> Vec<PsDisk> {
                use std::process::Command;
                let cmd = r#"Get-PhysicalDisk | Select SerialNumber,FriendlyName,BusType,MediaType | ConvertTo-Json -Depth 2"#;
                let out = Command::new("powershell").args(["-NoProfile", "-Command", cmd]).output();
                if let Ok(output) = out {
                    if output.status.success() {
                        if let Ok(txt) = String::from_utf8(output.stdout) {
                            let trimmed = txt.trim();
                            if trimmed.starts_with('[') {
                                return serde_json::from_str::<Vec<PsDisk>>(trimmed).unwrap_or_default();
                            } else {
                                return serde_json::from_str::<PsDisk>(trimmed).map(|x| vec![x]).unwrap_or_default();
                            }
                        }
                    }
                }
                vec![]
            }

            let ps_disks = ps_get_physical_disks();
            let mut ps_by_sn: HashMap<String, PsDisk> = HashMap::new();
            for p in ps_disks {
                if let Some(sn) = p.SerialNumber.as_ref().map(|s| s.trim().to_uppercase()) {
                    if !sn.is_empty() { ps_by_sn.insert(sn, p); }
                }
            }

            let mut letter_to_sn: HashMap<String, String> = HashMap::new();
            for dd in &wmi_drives {
                let phys = dd.DeviceID.as_deref().unwrap_or("");
                if phys.is_empty() { continue; }
                let sn = dd.SerialNumber.as_ref().map(|s| s.trim().to_uppercase()).unwrap_or_default();
                if sn.is_empty() { continue; }
                let parts = physical_to_partitions.get(phys).cloned().unwrap_or_default();
                for p in parts {
                    if let Some(letter) = partition_to_letter.get(&p) {
                        letter_to_sn.insert(letter.to_uppercase(), sn.clone());
                    }
                }
            }

            for (letter, idx) in &letter_to_idx {
                if let Some(di) = disks.get_mut(*idx) {
                    if di.disk_type.is_some() { continue; }
                    if let Some(sn) = letter_to_sn.get(letter) {
                        if let Some(ps) = ps_by_sn.get(sn) {
                            let bus = ps.BusType.as_deref().unwrap_or("").to_lowercase();
                            let media = ps.MediaType.as_deref().unwrap_or("").to_lowercase();
                            let dtype = if bus.contains("nvme") {
                                "NVMe".to_string()
                            } else if media.contains("ssd") {
                                if bus.contains("sata") { "SSD (SATA)".to_string() } else { "SSD".to_string() }
                            } else if media.contains("hdd") {
                                if bus.contains("sata") { "HDD (SATA)".to_string() }
                                else if bus.contains("sas") { "HDD (SAS)".to_string() }
                                else if bus.contains("ide") { "HDD (IDE)".to_string() }
                                else { "HDD".to_string() }
                            } else if !bus.is_empty() {
                                bus.to_uppercase()
                            } else {
                                "Unknown".to_string()
                            };
                            di.disk_type = Some(dtype);
                            if di.model.is_none() { di.model = ps.FriendlyName.clone(); }
                        }
                    }
                }
            }

            if disks.iter().any(|d| d.disk_type.is_none()) {
                if let Some(&cidx) = letter_to_idx.get("C:") {
                    if let Some(di) = disks.get_mut(cidx) {
                        if di.disk_type.is_none() {
                            let mut dtype = None;
                            for p in ps_by_sn.values() {
                                if p.BusType.as_deref().unwrap_or("").eq_ignore_ascii_case("nvme") {
                                    dtype = Some("NVMe".to_string()); break;
                                }
                            }
                            if dtype.is_none() {
                                for p in ps_by_sn.values() {
                                    if p.MediaType.as_deref().unwrap_or("").eq_ignore_ascii_case("ssd") {
                                        dtype = Some("SSD".to_string()); break;
                                    }
                                }
                            }
                            if dtype.is_none() {
                                for p in ps_by_sn.values() {
                                    if p.MediaType.as_deref().unwrap_or("").eq_ignore_ascii_case("hdd") {
                                        dtype = Some("HDD".to_string()); break;
                                    }
                                }
                            }
                            if let Some(t) = dtype { di.disk_type = Some(t); }
                        }
                    }
                }
            }
        }
    }

    // ---------------- LINUX DISK SECTION ----------------
    #[cfg(target_os = "linux")]
    {
        use std::fs;
        #[derive(Clone)]
        struct LinuxExtra { vendor: Option<String>, model: Option<String>, base_type: Option<String>, bus: Option<String> }

        fn bus_from(protocol: Option<&str>, name: &str) -> Option<String> {
            let p = protocol.unwrap_or("").to_lowercase();
            if name.starts_with("nvme") { return Some("NVMe".into()); }
            if p.contains("sas") { return Some("SAS".into()); }
            if p.contains("sata") || p.contains("ata") { return Some("SATA".into()); }
            if name.starts_with("hd") { return Some("IDE".into()); }
            None
        }

        let mut map = HashMap::<String, LinuxExtra>::new();
        if let Ok(entries) = fs::read_dir("/sys/block") {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().to_string();
                let path = e.path();
                let vendor = fs::read_to_string(path.join("device/vendor")).ok().map(|s| s.trim().to_string());
                let model = fs::read_to_string(path.join("device/model")).ok().map(|s| s.trim().to_string());
                let rotational = fs::read_to_string(path.join("queue/rotational")).ok().map(|s| s.trim().to_string());
                let protocol = fs::read_to_string(path.join("device/protocol"))
                    .or_else(|_| fs::read_to_string(path.join("device/transport")))
                    .ok().map(|s| s.trim().to_string());
                let base_type = match rotational.as_deref() {
                    Some("0") => Some("SSD".into()),
                    Some("1") => Some("HDD".into()),
                    _ => None,
                };
                let bus = bus_from(protocol.as_deref(), &name);
                map.insert(name, LinuxExtra { vendor, model, base_type, bus });
            }
        }

        for d in disks.iter_mut() {
            let raw_name = d.name.clone();
            let dev_name = raw_name
                .split(|c| c == '/' || c == '\\')
                .filter(|s| !s.is_empty())
                .last()
                .unwrap_or(&raw_name)
                .to_string();
            let blk = if dev_name.starts_with("nvme") {
                if let Some(p) = dev_name.rfind('p') { dev_name[..p].to_string() } else { dev_name }
            } else {
                dev_name.trim_end_matches(|c: char| c.is_ascii_digit()).to_string()
            };
            if let Some(extra) = map.get(&blk) {
                if d.vendor.is_none() { d.vendor = extra.vendor.clone(); }
                if d.model.is_none() { d.model = extra.model.clone(); }
                if d.disk_type.is_none() {
                    if let Some(bt) = &extra.base_type {
                        if let Some(bus) = &extra.bus {
                            d.disk_type = Some(format!("{} ({})", bt, bus));
                        } else {
                            d.disk_type = Some(bt.clone());
                        }
                    }
                }
            }
        }
    }

    // ---------------- macOS DISK SECTION ----------------
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        if let Ok(out) = Command::new("diskutil").arg("info").arg("-all").output() {
            if let Ok(txt) = String::from_utf8(out.stdout) {
                let mut model = None;
                let mut vendor = None;
                for line in txt.lines() {
                    let l = line.trim();
                    if l.starts_with("Device Model:") { model = Some(l[13..].trim().to_string()); }
                    if l.starts_with("Device Manufacturer:") { vendor = Some(l[20..].trim().to_string()); }
                }
                if let Some(first) = disks.first_mut() {
                    first.vendor = vendor;
                    first.model = model.clone();
                    if let Some(m) = &model {
                        let s = m.to_lowercase();
                        if s.contains("nvme") { first.disk_type = Some("NVMe".into()); }
                        else if s.contains("ssd") { first.disk_type = Some("SSD".into()); }
                    }
                }
            }
        }
    }

    sysinfo.disks = disks;
    sysinfo
}