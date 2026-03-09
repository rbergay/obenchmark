use sysinfo::System;

use crate::model::result::{SystemInfo, CpuInfo, DiskInfo};

fn human_bytes(mut bytes: f64) -> String {
    let units = ["B", "KB", "MB", "GB", "TB"];
    let mut i = 0usize;
    while bytes >= 1024.0 && i < units.len() - 1 {
        bytes /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{} {}", bytes as u64, units[i])
    } else {
        format!("{:.2} {}", bytes, units[i])
    }
}

pub fn get_system_info() -> System {
    let mut sys = System::new_all();
    sys.refresh_all();
    sys
}

pub fn get_detailed_system_info() -> SystemInfo {
    let mut sysinfo = SystemInfo::default();

    // ---------------------------------------------------------
    // CPU cross‑platform
    // ---------------------------------------------------------
    {
        let mut cpu = CpuInfo::default();
        cpu.cores_logical = num_cpus::get();

        let s = get_system_info();
        let g = s.global_cpu_info();

        let brand = g.brand().trim();
        if !brand.is_empty() {
            cpu.model = Some(brand.to_string());
        }

        let vendor = g.vendor_id().trim();
        if !vendor.is_empty() {
            cpu.vendor = Some(vendor.to_string());
        }

        let freq = g.frequency();
        if freq > 0 {
            cpu.frequency_mhz = Some(freq as u64);
        }

        sysinfo.cpu = cpu;
    }

    // ---------------------------------------------------------
    // RAM (cross‑platform)
    // ---------------------------------------------------------
    {
        let s = get_system_info();
        let total_mb = s.total_memory() / 1024;

        sysinfo.ram.total_mb = total_mb;
        sysinfo.ram.total_readable = Some(if total_mb >= 1024 {
            format!("{:.2} GB", total_mb as f64 / 1024.0)
        } else {
            format!("{} MB", total_mb)
        });
    }

    // ---------------------------------------------------------
    // Linux: CPU & RAM modules (dmidecode)
    // ---------------------------------------------------------
    #[cfg(target_os = "linux")]
    {
        use std::fs;
        use std::process::Command;

        // Fallback CPU info via /proc/cpuinfo si sysinfo est incomplet.
        if sysinfo.cpu.vendor.is_none()
            || sysinfo.cpu.model.is_none()
            || sysinfo.cpu.frequency_mhz.is_none()
        {
            if let Ok(contents) = fs::read_to_string("/proc/cpuinfo") {
                for line in contents.lines() {
                    let line = line.trim();

                    if sysinfo.cpu.vendor.is_none() && line.starts_with("vendor_id") {
                        if let Some(v) = line.split(':').nth(1) {
                            let v = v.trim();
                            if !v.is_empty() {
                                sysinfo.cpu.vendor = Some(v.to_string());
                            }
                        }
                    } else if sysinfo.cpu.model.is_none() && line.starts_with("model name") {
                        if let Some(v) = line.split(':').nth(1) {
                            let v = v.trim();
                            if !v.is_empty() {
                                sysinfo.cpu.model = Some(v.to_string());
                            }
                        }
                    } else if sysinfo.cpu.frequency_mhz.is_none() && line.starts_with("cpu MHz") {
                        if let Some(v) = line.split(':').nth(1) {
                            if let Ok(f) = v.trim().parse::<f64>() {
                                if f > 0.0 {
                                    sysinfo.cpu.frequency_mhz = Some(f as u64);
                                }
                            }
                        }
                    }
                }
            }
        }

        // RAM modules via dmidecode (si présent et autorisé).
        if let Ok(out) = Command::new("dmidecode").arg("-t").arg("17").output() {
            if out.status.success() {
                if let Ok(txt) = String::from_utf8(out.stdout) {
                    let mut current = MemoryModule::default();
                    let mut hist = HashMap::new();

                    for l in txt.lines() {
                        let line = l.trim();

                        if line.starts_with("Memory Device") {
                            if current.memory_type.is_some() {
                                hist.entry(current.memory_type.clone().unwrap())
                                    .and_modify(|v| *v += 1)
                                    .or_insert(1);
                                sysinfo.ram.modules.push(current);
                            }
                            current = MemoryModule::default();
                        }

                        if let Some(v) = line.strip_prefix("Manufacturer:") {
                            current.vendor = Some(v.trim().to_string());
                        }
                        if let Some(v) = line.strip_prefix("Part Number:") {
                            current.part_number = Some(v.trim().to_string());
                        }
                        if let Some(v) = line.strip_prefix("Type:") {
                            let t = v.trim();
                            if t.starts_with("DDR") {
                                current.memory_type = Some(t.into());
                            }
                        }
                        if let Some(v) = line.strip_prefix("Size:") {
                            let s = v.trim();
                            if s.ends_with("GB") {
                                if let Ok(n) = s[..s.len() - 2].trim().parse::<u64>() {
                                    current.size_mb = Some(n * 1024);
                                }
                            }
                            if s.ends_with("MB") {
                                if let Ok(n) = s[..s.len() - 2].trim().parse::<u64>() {
                                    current.size_mb = Some(n);
                                }
                            }
                        }
                    }

                    sysinfo.ram.ram_type = hist
                        .into_iter()
                        .max_by_key(|(_, c)| *c)
                        .map(|(t, _)| t);
                }
            }
        }
    }

    // ---------------------------------------------------------
    // macOS RAM (pas de modules possible)
    // ---------------------------------------------------------
    #[cfg(target_os = "macos")]
    {
        sysinfo.ram.ram_type = None;
    }

#[cfg(target_os = "windows")]
{
    use serde::Deserialize;
    use wmi::WMIConnection;

    // ---------- CPU via WMI ----------
    #[derive(Deserialize, Debug)]
    struct Win32Processor {
        Name: Option<String>,
        Manufacturer: Option<String>,
        MaxClockSpeed: Option<u32>,   // MHz
        CurrentClockSpeed: Option<u32>,
    }

    if let Ok(wmi) = WMIConnection::new() {
        if let Ok(procs) = wmi.raw_query::<Win32Processor>(
            "SELECT Name, Manufacturer, MaxClockSpeed, CurrentClockSpeed FROM Win32_Processor"
        ) {
            // Prends le premier CPU logique (suffisant pour le modèle/fabricant)
            if let Some(p) = procs.into_iter().next() {
                // Model
                if let Some(name) = p.Name.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                    if sysinfo.cpu.model.as_ref().map(|m| m.is_empty()).unwrap_or(true) {
                        sysinfo.cpu.model = Some(name.to_string());
                    }
                }

                // Vendor
                if let Some(v) = p.Manufacturer.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                    if sysinfo.cpu.vendor.as_ref().map(|m| m.is_empty()).unwrap_or(true) {
                        sysinfo.cpu.vendor = Some(v.to_string());
                    }
                }

                // Fréquence (MHz) : MaxClockSpeed prioritaire, sinon CurrentClockSpeed, sinon conserve sysinfo
                if sysinfo.cpu.frequency_mhz.is_none() {
                    if let Some(max) = p.MaxClockSpeed {
                        if max > 0 { sysinfo.cpu.frequency_mhz = Some(max as u64); }
                    } else if let Some(cur) = p.CurrentClockSpeed {
                        if cur > 0 { sysinfo.cpu.frequency_mhz = Some(cur as u64); }
                    }
                }
            }
        }
    }

    // ---------- RAM TYPE via WMI ----------
    #[derive(Deserialize, Debug)]
    struct Win32PhysicalMemory {
        Manufacturer: Option<String>,
        PartNumber: Option<String>,
        Capacity: Option<u64>,        // IMPORTANT
        SMBIOSMemoryType: Option<u16>,
        MemoryType: Option<u16>,
    }

    // Mapping SMBIOS à jour (cf. spec) :
    // 20=DDR, 21=DDR2, 24=DDR3, 26=DDR4, 34=DDR5
    fn map_smbios_memory_type(code: u16) -> Option<&'static str> {
        match code {
            20 => Some("DDR"),
            21 => Some("DDR2"),
            24 => Some("DDR3"),
            26 => Some("DDR4"),
            34 => Some("DDR5"),
            _ => None,
        }
    }

    if let Ok(wmi) = WMIConnection::new() {
        if let Ok(mem_modules) = wmi.raw_query::<Win32PhysicalMemory>(
            "SELECT Manufacturer, PartNumber, Capacity, SMBIOSMemoryType, MemoryType FROM Win32_PhysicalMemory"
        ) {
            let mut type_hist: std::collections::HashMap<String, u32> = std::collections::HashMap::new();

            for m in mem_modules {
                // Type depuis SMBIOSMemoryType, sinon MemoryType
                let mem_type = m.SMBIOSMemoryType
                    .and_then(map_smbios_memory_type)
                    .or_else(|| m.MemoryType.and_then(map_smbios_memory_type))
                    .map(|s| s.to_string());

                if let Some(t) = &mem_type {
                    *type_hist.entry(t.clone()).or_insert(0) += 1;
                }

                let size_mb = m.Capacity.map(|b| b / 1024 / 1024);

                sysinfo.ram.modules.push(crate::model::result::MemoryModule {
                    vendor: m.Manufacturer.as_ref().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
                    part_number: m.PartNumber.as_ref().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()),
                    size_mb,
                    memory_type: mem_type,
                });
            }

            // Type RAM dominant si pas déjà fixé
            if sysinfo.ram.ram_type.is_none() {
                if let Some((t, _)) = type_hist.into_iter().max_by_key(|(_, c)| *c) {
                    sysinfo.ram.ram_type = Some(t);
                }
            }
        }
    }
}

    // ---------------------------------------------------------
    // DISKS via sysinfo (cross‑platform)
    // ---------------------------------------------------------
    let mut disks = vec![];
    {
        let sd = sysinfo::Disks::new_with_refreshed_list();
        for d in sd.list() {
            let total = d.total_space();

            disks.push(DiskInfo {
                name: d.name().to_string_lossy().to_string(),
                mount_point: Some(d.mount_point().to_string_lossy().to_string()),
                total_bytes: Some(total),
                size_readable: Some(human_bytes(total as f64)),
                vendor: None,
                model: None,
                disk_type: None,
            });
        }
    }

 // ---------------------------------------------------------
// Windows DISKS — WMI mapping PHYSICALDRIVE -> partitions -> drive letters
// ---------------------------------------------------------
#[cfg(target_os = "windows")]
{
    use serde::Deserialize;
    use wmi::WMIConnection;
    use std::collections::HashMap;

    #[derive(Deserialize, Debug)]
    struct Win32DiskDrive {
        DeviceID: Option<String>,      // e.g. "\\.\PHYSICALDRIVE0"
        Model: Option<String>,
        Manufacturer: Option<String>,
        InterfaceType: Option<String>, // e.g. "NVMe", "SATA", "SCSI"
        MediaType: Option<String>,     // sometimes "SSD", "HDD", or "Fixed hard disk media"
    }

    #[derive(Deserialize, Debug)]
    struct Win32DiskDriveToDiskPartition {
        Antecedent: String, // Win32_DiskDrive reference
        Dependent: String,  // Win32_DiskPartition reference
    }

    #[derive(Deserialize, Debug)]
    struct Win32LogicalDiskToPartition {
        Antecedent: String, // Win32_LogicalDisk reference (Drive letter)
        Dependent: String,  // Win32_DiskPartition reference
    }

    fn extract_quoted_value(s: &str, key: &str) -> Option<String> {
        // parses WMI REF strings like:
        // \\...:Win32_LogicalDisk.DeviceID="C:"
        let needle = format!(r#"{}=""#, key);
        let start = s.find(&needle)? + needle.len();
        let rest = &s[start..];
        let end = rest.find('"')?;
        Some(rest[..end].to_string())
    }

    fn extract_device_id(ref_str: &str) -> Option<String> {
        extract_quoted_value(ref_str, "DeviceID")
    }

    fn normalize_disk_type(
    interface: Option<&str>,
    model: Option<&str>,
    media: Option<&str>
) -> Option<String> {

    let iface = interface.unwrap_or("").to_lowercase();
    let model = model.unwrap_or("").to_lowercase();
    let media = media.unwrap_or("").to_lowercase();

    // 1. NVMe — détecté via modèle (le plus fiable)
    if model.contains("nvme") {
        return Some("NVMe".into());
    }

    // 2. SSD — via media ou modèle
    if media.contains("ssd") || model.contains("ssd") {
        // Bus SATA ?
        if iface.contains("sata") {
            return Some("SSD (SATA)".into());
        }
        return Some("SSD".into());
    }

    // 3. HDD — via media ou modèle
    if media.contains("hdd") || media.contains("fixed") || model.contains("hdd") {
        if iface.contains("sata") {
            return Some("HDD (SATA)".into());
        }
        if iface.contains("ide") {
            return Some("HDD (IDE)".into());
        }
        if iface.contains("sas") {
            return Some("HDD (SAS)".into());
        }
        return Some("HDD".into());
    }

    // 4. Bus fallback
    if iface.contains("sata") {
        return Some("SATA".into());
    }
    if iface.contains("ide") {
        return Some("IDE".into());
    }
    if iface.contains("sas") {
        return Some("SAS".into());
    }
    if iface.contains("scsi") {
        return Some("SCSI".into());
    }

    None
}

    if let Ok(wmi_con) = WMIConnection::new() {
        // 1) Partition -> Letter mapping (ex: "Disk #0, Partition #1" -> "C:")
        let mut partition_to_letter: HashMap<String, String> = HashMap::new();
        if let Ok(links) = wmi_con.raw_query::<Win32LogicalDiskToPartition>(
            "SELECT Antecedent, Dependent FROM Win32_LogicalDiskToPartition"
        ) {
            for l in links {
                if let (Some(letter), Some(partition)) = (
                    extract_device_id(&l.Antecedent),
                    extract_device_id(&l.Dependent),
                ) {
                    // letter ex: "C:"
                    partition_to_letter.insert(partition, letter);
                }
            }
        }

        // 2) PhysicalDrive -> Partitions mapping (ex: "\\.\PHYSICALDRIVE0" -> ["Disk #0, Partition #1", ...])
        let mut physical_to_partitions: HashMap<String, Vec<String>> = HashMap::new();
        if let Ok(links) = wmi_con.raw_query::<Win32DiskDriveToDiskPartition>(
            "SELECT Antecedent, Dependent FROM Win32_DiskDriveToDiskPartition"
        ) {
            for l in links {
                if let (Some(physical), Some(partition)) = (
                    extract_device_id(&l.Antecedent),
                    extract_device_id(&l.Dependent),
                ) {
                    physical_to_partitions.entry(physical).or_default().push(partition);
                }
            }
        }

        // 3) Construire un index lettre -> index dans `disks` (issus de sysinfo) :
        // sysinfo mount_point est du genre "C:\\" ; on normalise en "C:"
        let mut disk_index_by_letter: HashMap<String, usize> = HashMap::new();
        for (idx, di) in disks.iter().enumerate() {
            if let Some(mp) = &di.mount_point {
                let mp_trim = mp.trim();
                if mp_trim.len() >= 2 {
                    let letter = &mp_trim[..2]; // "C:"
                    if letter.chars().nth(1) == Some(':')
                        && letter.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
                    {
                        disk_index_by_letter.insert(letter.to_uppercase(), idx);
                    }
                }
            }
        }

        // 4) Parcourir les DiskDrives et propager model/manufacturer/type vers chaque lettre correspondante
        if let Ok(drives) = wmi_con.raw_query::<Win32DiskDrive>(
            "SELECT DeviceID, Model, Manufacturer, InterfaceType, MediaType FROM Win32_DiskDrive"
        ) {
            for d in drives {
                let physical = d.DeviceID
                    .as_deref()
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_string);
                let vendor = d.Manufacturer
                    .as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
                let model = d.Model
                    .as_deref().map(str::trim).filter(|s| !s.is_empty()).map(str::to_string);
                let disk_type = normalize_disk_type(
                    d.InterfaceType.as_deref(),
                    d.Model.as_deref(),
                    d.MediaType.as_deref(),
                );

                let Some(physical) = physical else { continue; };

                // Récupère toutes les lettres associées à ce physical drive
                let mut letters: Vec<String> = physical_to_partitions
                    .get(&physical).cloned().unwrap_or_default()
                    .into_iter()
                    .filter_map(|p| partition_to_letter.get(&p).cloned())
                    .collect();

                letters.sort();
                letters.dedup();

                // Applique les infos WMI à chaque disque sysinfo correspondant à ces lettres
                for letter in letters {
                    if let Some(&idx) = disk_index_by_letter.get(&letter.to_uppercase()) {
                        if let Some(di) = disks.get_mut(idx) {
                            if di.vendor.is_none() { di.vendor = vendor.clone(); }
                            if di.model.is_none() { di.model = model.clone(); }
                            if di.disk_type.is_none() { di.disk_type = disk_type.clone(); }
                        }
                    }
                }
            }
        }
    }
}

    // ---------------------------------------------------------
    // Linux: enrichissement via /sys/block (vendor / model / type HDD/SSD + bus)
    // ---------------------------------------------------------
    #[cfg(target_os = "linux")]
    {
        use std::fs;

        #[derive(Clone)]
        struct LinuxDiskExtra {
            vendor: Option<String>,
            model: Option<String>,
            base_type: Option<String>, // "HDD" | "SSD"
            bus: Option<String>,       // "IDE" | "SATA" | "SAS" | "NVMe" | ...
        }

        fn classify_bus(protocol: Option<&str>, name: &str) -> Option<String> {
            let pname = protocol.unwrap_or("").to_lowercase();
            if name.starts_with("nvme") {
                return Some("NVMe".to_string());
            }
            if pname.contains("sas") {
                return Some("SAS".to_string());
            }
            if pname.contains("sata") || pname.contains("ata") {
                return Some("SATA".to_string());
            }
            if name.starts_with("hd") {
                return Some("IDE".to_string());
            }
            None
        }

        let mut map = HashMap::<String, LinuxDiskExtra>::new();

        if let Ok(entries) = fs::read_dir("/sys/block") {
            for e in entries.flatten() {
                let name = e.file_name().to_string_lossy().to_string();

                let vendor = fs::read_to_string(e.path().join("device/vendor"))
                    .ok()
                    .map(|s| s.trim().to_string());

                let model = fs::read_to_string(e.path().join("device/model"))
                    .ok()
                    .map(|s| s.trim().to_string());

                let rotational = fs::read_to_string(e.path().join("queue/rotational"))
                    .ok()
                    .map(|s| s.trim().to_string());

                let protocol = fs::read_to_string(e.path().join("device/protocol"))
                    .or_else(|_| fs::read_to_string(e.path().join("device/transport")))
                    .ok()
                    .map(|s| s.trim().to_string());

                let base_type = match rotational.as_deref() {
                    Some("0") => Some("SSD".into()),
                    Some("1") => Some("HDD".into()),
                    _ => None,
                };

                let bus = classify_bus(protocol.as_deref(), &name);

                map.insert(
                    name,
                    LinuxDiskExtra {
                        vendor,
                        model,
                        base_type,
                        bus,
                    },
                );
            }
        }

        for disk in disks.iter_mut() {
            // Exemple de name() sous Linux: "/dev/sda", "/dev/sda1", "/dev/nvme0n1p1"
            let raw_name = disk.name.clone();
            let dev_name = raw_name
                .split(|c| c == '/' || c == '\\')
                .filter(|s| !s.is_empty())
                .last()
                .unwrap_or(&raw_name)
                .to_string();

            // Pour les disques classiques: "sda1" -> "sda"
            // Pour NVMe: "nvme0n1p1" -> "nvme0n1"
            let block_name = if dev_name.starts_with("nvme") {
                if let Some(p_pos) = dev_name.rfind('p') {
                    dev_name[..p_pos].to_string()
                } else {
                    dev_name
                }
            } else {
                dev_name
                    .trim_end_matches(|c: char| c.is_ascii_digit())
                    .to_string()
            };

            if let Some(extra) = map.get(&block_name) {
                disk.vendor = extra.vendor.clone().or(disk.vendor.take());
                disk.model = extra.model.clone().or(disk.model.take());

                if disk.disk_type.is_none() {
                    if let Some(bt) = &extra.base_type {
                        if let Some(bus) = &extra.bus {
                            disk.disk_type = Some(format!("{} ({})", bt, bus));
                        } else {
                            disk.disk_type = Some(bt.clone());
                        }
                    }
                }
            }
        }
    }

    // ---------------------------------------------------------
    // macOS diskutil
    // ---------------------------------------------------------
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;

        if let Ok(out) = Command::new("diskutil").arg("info").arg("-all").output() {
            if let Ok(txt) = String::from_utf8(out.stdout) {
                let mut model = None;
                let mut vendor = None;

                for line in txt.lines() {
                    let l = line.trim();
                    if l.starts_with("Device Model:") {
                        model = Some(l[13..].trim().to_string());
                    }
                    if l.starts_with("Device Manufacturer:") {
                        vendor = Some(l[20..].trim().to_string());
                    }
                }

                if let Some(first) = disks.first_mut() {
                    first.model = model;
                    first.vendor = vendor;

                    if let Some(m) = &first.model {
                        let s = m.to_lowercase();
                        if s.contains("nvme") {
                            first.disk_type = Some("NVMe".into());
                        } else if s.contains("ssd") {
                            first.disk_type = Some("SSD".into());
                        }
                    }
                }
            }
        }
    }

    sysinfo.disks = disks;

    sysinfo
}