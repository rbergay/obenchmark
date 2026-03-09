use crate::model::result::SystemInfo;

#[derive(Debug, Clone)]
pub struct HwCheckResult {
    pub cpu_ok: bool,
    pub ram_ok: bool,
    pub disk_ok: bool,
}

// Seuils simples pour PostgreSQL : CPU >= 4 cœurs, RAM >= 8 Go, SSD/NVMe recommandé.
pub fn evaluate_hw(sys: &SystemInfo) -> HwCheckResult {
    let cpu_ok = sys.cpu.cores_logical >= 4;
    let ram_ok = sys.ram.total_mb >= 8 * 1024;

    let mut disk_ok = false;
    for d in &sys.disks {
        if let Some(t) = &d.disk_type {
            let t = t.to_lowercase();
            if t.contains("ssd") || t.contains("nvme") {
                disk_ok = true;
            }
        }
    }

    HwCheckResult { cpu_ok, ram_ok, disk_ok }
}