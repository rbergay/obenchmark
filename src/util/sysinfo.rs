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

    // ----------------------------
    // CPU (cross-platform)
    // ----------------------------
    {
        let s = get_system_info();
        let g = s.global_cpu_info();

        let mut cpu = CpuInfo::default();
        cpu.cores_logical = num_cpus::get();
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

    // ----------------------------
    // RAM (cross-platform)
    // ----------------------------
    {
        let s = get_system_info();
        let total_mb = s.total_memory() / 1024;
        let mut ram = RamInfo::default();
        ram.total_mb = total_mb;
        ram.total_readable = Some(if total_mb >= 1024 {
            format!("{:.2} GB", total_mb as f64 / 1024.0)
        } else {
            format!("{} MB", total_mb)
        });
        sysinfo.ram = ram;
    }

    // ----------------------------
    // Disks (sysinfo base list)
    // ----------------------------
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

    // ----------------------------
    // WINDOWS: WMI + fallback PowerShell
    // ----------------------------
    #[cfg(target_os = "windows")]
    {
        use serde::Deserialize;
        use wmi::WMIConnection;

        // ---- WMI types
        #[derive(Deserialize, Debug, Clone)]
        struct Win32DiskDrive {
            DeviceID: Option<String>,      // \\.\PHYSICALDRIVE0
            Model: Option<String>,
            Manufacturer: Option<String>,
            InterfaceType: Option<String>, // SATA/SCSI/IDE/RAID/USB...
            MediaType: Option<String>,     // SSD / Fixed...
            SerialNumber: Option<String>,
            Size: Option<u64>,
        }
        #[derive(Deserialize, Debug)]
        struct Win32DiskDriveToDiskPartition { Antecedent: String, Dependent: String }
        #[derive(Deserialize, Debug)]
        struct Win32LogicalDiskToPartition   { Antecedent: String, Dependent: String }

        fn wmi_get(ref_str: &str, key: &str) -> Option<String> {
            let needle = format!(r#"{}=""#, key);
            let idx = ref_str.find(&needle)? + needle.len();
            let rem = &ref_str[idx..];
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
                if i.contains("sas")  { return Some("HDD (SAS)".into()); }
                if i.contains("ide")  { return Some("HDD (IDE)".into()); }
                return Some("HDD".into());
            }
            if i.contains("sata") { return Some("SATA".into()); }
            if i.contains("sas")  { return Some("SAS".into()); }
            if i.contains("ide")  { return Some("IDE".into()); }
            if i.contains("raid") { return Some("RAID".into()); }
            if i.contains("usb")  { return Some("USB".into()); }
            None
        }

        // Build WMI connections & maps
        let wmi = WMIConnection::new().ok();

        // Partition -> Letter
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

        // Physical -> Partitions
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

        // Letter -> index in `disks`
        let mut letter_to_idx: HashMap<String, usize> = HashMap::new();
        for (i, d) in disks.iter().enumerate() {
            if let Some(mp) = &d.mount_point {
                if mp.len() >= 2 {
                    let letter = mp[..2].to_uppercase();
                    if letter.ends_with(':') { letter_to_idx.insert(letter, i); }
                }
            }
        }

        // 1) Enrich with Win32_DiskDrive
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
                        dd.InterfaceType.as_deref(),
                        dd.Model.as_deref(),
                        dd.MediaType.as_deref(),
                    );

                    for letter in letters {
                        if let Some(&idx) = letter_to_idx.get(&letter) {
                            let di = &mut disks[idx];
                            if di.vendor.is_none()    { di.vendor = dd.Manufacturer.clone(); }
                            if di.model.is_none()     { di.model = dd.Model.clone(); }
                            if di.disk_type.is_none() { di.disk_type = disk_type.clone(); }
                        }
                    }
                }
            }
        }

        // 2) Fallback PowerShell (si certains disques n'ont toujours pas de type)
        //    On mappe via le SerialNumber (WMI <-> PowerShell)
        let needs_fallback = disks.iter().any(|d| d.disk_type.is_none());
        if needs_fallback {
            // Récupère PowerShell PhysicalDisks en JSON
            #[derive(Deserialize, Debug, Clone)]
            struct PsDisk {
                #[serde(default)]
                SerialNumber: Option<String>,
                #[serde(default)]
                FriendlyName: Option<String>,
                #[serde(default)]
                BusType: Option<String>,   // NVMe / SATA / SAS / RAID / USB / ...
                #[serde(default)]
                MediaType: Option<String>, // SSD / HDD / Unspecified
            }

            fn ps_get_physical_disks() -> Vec<PsDisk> {
                use std::process::Command;
                let cmd = r#"Get-PhysicalDisk | Select SerialNumber,FriendlyName,BusType,MediaType | ConvertTo-Json -Depth 2"#;
                let out = Command::new("powershell")
                    .arg("-NoProfile").arg("-Command").arg(cmd)
                    .output();

                if let Ok(output) = out {
                    if output.status.success() {
                        if let Ok(txt) = String::from_utf8(output.stdout) {
                            // Peut être un objet ou un tableau JSON
                            let trimmed = txt.trim();
                            if trimmed.starts_with('[') {
                                serde_json::from_str::<Vec<PsDisk>>(trimmed).unwrap_or_default()
                            } else {
                                serde_json::from_str::<PsDisk>(trimmed).map(|x| vec![x]).unwrap_or_default()
                            }
                        } else { vec![] }
                    } else { vec![] }
                } else { vec![] }
            }

            let ps_disks = ps_get_physical_disks();

            // Index PS par SerialNumber (uppercase, trim)
            let mut ps_by_sn: HashMap<String, PsDisk> = HashMap::new();
            for p in ps_disks {
                if let Some(sn) = p.SerialNumber.as_ref().map(|s| s.trim().to_uppercase()) {
                    if !sn.is_empty() { ps_by_sn.insert(sn, p); }
                }
            }

            // Pour relier lettre -> SerialNumber, on passe par WMI (Win32_DiskDrive)
            // (on a déjà wmi_drives + physical_to_partitions + partition_to_letter)
            // Construisons: letter -> serialnumber
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

            // Complète disk_type/vendor/model via PS si manquant
            for (letter, idx) in &letter_to_idx {
                if let Some(di) = disks.get_mut(*idx) {
                    if di.disk_type.is_some() { continue; }
                    if let Some(sn) = letter_to_sn.get(letter) {
                        if let Some(ps) = ps_by_sn.get(sn) {
                            // Classifier à partir de BusType + MediaType
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
                            // Vendor/Model: on laisse WMI prioritaire ; PS FriendlyName en fallback
                            if di.model.is_none() {
                                di.model = ps.FriendlyName.clone();
                            }
                        }
                    }
                }
            }

            // Dernier recours : s'il reste des disques sans type, on essaye
            // d'appliquer l'heuristique "disque de C:" = premier disque WMI.
            if disks.iter().any(|d| d.disk_type.is_none()) {
                if let Some(&cidx) = letter_to_idx.get("C:") {
                    if let Some(di) = disks.get_mut(cidx) {
                        if di.disk_type.is_none() {
                            // Cherche un PS disque "NVMe" sinon "SSD" sinon "HDD"
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

    // ----------------------------
    // LINUX: /sys/block
    // ----------------------------
    #[cfg(target_os = "linux")]
    {
        use std::fs;

        #[derive(Clone)]
        struct LinuxExtra {
            vendor: Option<String>,
            model: Option<String>,
            base_type: Option<String>, // SSD/HDD
            bus: Option<String>,       // NVMe/SATA/SAS/IDE
        }

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
                let model  = fs::read_to_string(path.join("device/model")).ok().map(|s| s.trim().to_string());
                let rotational = fs::read_to_string(path.join("queue/rotational")).ok().map(|s| s.trim().to_string());
                let protocol = fs::read_to_string(path.join("device/protocol"))
                    .or_else(|_| fs::read_to_string(path.join("device/transport")))
                    .ok()
                    .map(|s| s.trim().to_string());

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
                if d.vendor.is_none()    { d.vendor = extra.vendor.clone(); }
                if d.model.is_none()     { d.model  = extra.model.clone(); }
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

    // ----------------------------
    // macOS: diskutil info -all
    // ----------------------------
    #[cfg(target_os = "macos")]
    {
        use std::process::Command;
        if let Ok(out) = Command::new("diskutil").arg("info").arg("-all").output() {
            if let Ok(txt) = String::from_utf8(out.stdout) {
                let mut model = None;
                let mut vendor = None;
                for line in txt.lines() {
                    let l = line.trim();
                    if l.starts_with("Device Model:")        { model = Some(l[13..].trim().to_string()); }
                    if l.starts_with("Device Manufacturer:") { vendor = Some(l[20..].trim().to_string()); }
                }
                if let Some(first) = disks.first_mut() {
                    first.vendor = vendor;
                    first.model  = model;
                    if let Some(m) = &first.model {
                        let s = m.to_lowercase();
                        if s.contains("nvme")      { first.disk_type = Some("NVMe".into()); }
                        else if s.contains("ssd")  { first.disk_type = Some("SSD".into()); }
                    }
                }
            }
        }
    }

    // Final
    sysinfo.disks = disks;
    sysinfo
}