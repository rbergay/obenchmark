use serde::{Serialize, Deserialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct BenchScore {
    pub name: String,
    pub raw_score: u64,
    pub weight: u64,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct BenchResult {
    pub scores: Vec<BenchScore>,
    pub final_score: u64,
    #[serde(default)]
    pub cpu_score: u64,
    #[serde(default)]
    pub mem_score: u64,
    #[serde(default)]
    pub disk_score: u64,
    pub system_info: Option<SystemInfo>,
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct CpuInfo {
    pub vendor: Option<String>,
    pub model: Option<String>,
    pub cores_logical: usize,
    pub cores_physical: Option<usize>,
    pub frequency_mhz: Option<u64>,
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct RamInfo {
    pub total_mb: u64,
    #[serde(default)]
    pub ram_type: Option<String>,
    pub modules: Vec<MemoryModule>,
    pub total_readable: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct MemoryModule {
    pub vendor: Option<String>,
    pub part_number: Option<String>,
    pub size_mb: Option<u64>,
    #[serde(default)]
    pub memory_type: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct DiskInfo {
    pub name: String,
    pub vendor: Option<String>,
    pub model: Option<String>,
    #[serde(default)]
    pub disk_type: Option<String>, // "HDD" | "SSD" | "NVMe" | "Unknown"
    pub mount_point: Option<String>,
    pub total_bytes: Option<u64>,
    pub size_readable: Option<String>,
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct SystemInfo {
    pub cpu: CpuInfo,
    pub ram: RamInfo,
    pub disks: Vec<DiskInfo>,
}