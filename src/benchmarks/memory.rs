use std::time::Instant;
use anyhow::Result;
use crate::engines::benchmark::Benchmark;
use crate::util::sysinfo::get_system_info;
use std::thread;

// 1. Opérations base de données (simulées avec vector)
pub struct MemoryDBOps;
impl Benchmark for MemoryDBOps {
    fn name(&self) -> &str { "Mem DB Ops" }
    fn weight(&self) -> u64 { 2 }

    fn run(&self) -> Result<u64> {
        let mut db = Vec::with_capacity(1_000_000);
        let start = Instant::now();
        for i in 0..1_000_000 {
            db.push(i);
            let _ = db[i / 2];
        }
        let elapsed = start.elapsed().as_secs_f64();
        let ops = (2_000_000.0 / elapsed) as u64; // approx operations per second
        Ok(ops)
    }
}

// 2. Lecture en cache (petit buffer répétitif)
pub struct MemoryCachedRead;
impl Benchmark for MemoryCachedRead {
    fn name(&self) -> &str { "Mem Cached Read" }
    fn weight(&self) -> u64 { 2 }

    fn run(&self) -> Result<u64> {
        let size = 8 * 1024 * 1024; // 8MB
        let data = vec![0u8; size];
        let start = Instant::now();
        let mut _sum = 0u64;
        for _ in 0..100 {
            for &b in &data {
                _sum += b as u64;
            }
        }
        let elapsed = start.elapsed().as_secs_f64();
        let mb = (size as f64 * 100.0) / (1024.0 * 1024.0);
        Ok((mb / elapsed) as u64)
    }
}

// 3. Lecture non cachée (grand buffer)
pub struct MemoryUncachedRead;
impl Benchmark for MemoryUncachedRead {
    fn name(&self) -> &str { "Mem Uncached Read" }
    fn weight(&self) -> u64 { 2 }

    fn run(&self) -> Result<u64> {
        // cap buffer to at most half of total RAM, default 512MB
        let sys = get_system_info();
        let total_bytes = sys.total_memory() * 1024; // sys.total_memory() returns KB
        let default: u64 = 512u64 * 1024 * 1024;
        let half = total_bytes / 2;
        let size_bytes = std::cmp::min(default, half);
        let size = size_bytes as usize;
        let data = vec![0u8; size];
        let start = Instant::now();
        let mut _sum = 0u64;
        for &b in &data {
            _sum += b as u64;
        }
        let elapsed = start.elapsed().as_secs_f64();
        let mb = (size as f64) / (1024.0 * 1024.0);
        Ok((mb / elapsed) as u64)
    }
}

// 4. Écriture mémoire
pub struct MemoryWrite;
impl Benchmark for MemoryWrite {
    fn name(&self) -> &str { "Mem Write" }
    fn weight(&self) -> u64 { 2 }

    fn run(&self) -> Result<u64> {
        // cap buffer to at most half of total RAM, default 512MB
        let sys = get_system_info();
        let total_bytes = sys.total_memory() * 1024; // KB -> bytes
        let default: u64 = 512u64 * 1024 * 1024;
        let half = total_bytes / 2;
        let size_bytes = std::cmp::min(default, half);
        let size = size_bytes as usize;
        let mut data = vec![0u8; size];
        let start = Instant::now();
        for i in 0..size {
            data[i] = (i % 255) as u8;
        }
        let elapsed = start.elapsed().as_secs_f64();
        let mb = (size as f64) / (1024.0 * 1024.0);
        Ok((mb / elapsed) as u64)
    }
}

// 5. RAM disponible
pub struct MemoryAvailable;
impl Benchmark for MemoryAvailable {
    fn name(&self) -> &str { "Mem Available" }
    fn weight(&self) -> u64 { 1 }

    fn run(&self) -> Result<u64> {
        let sys = get_system_info();
        Ok((sys.available_memory() / 1024) as u64) // MB
    }
}

// 6. Latence mémoire (accès aléatoire chaîné)
pub struct MemoryLatency;
impl Benchmark for MemoryLatency {
    fn name(&self) -> &str { "Mem Latency" }
    fn weight(&self) -> u64 { 2 }

    fn run(&self) -> Result<u64> {
        let n = 10_000_000;
        let mut data = vec![0usize; n];
        // construire une liste chaînée
        for i in 0..n {
            data[i] = (i + 1) % n;
        }
        let mut idx = 0;
        let start = Instant::now();
        for _ in 0..n {
            idx = data[idx];
        }
        let elapsed_secs = start.elapsed().as_secs_f64();
        // Convertir en accès par seconde (plus élevé = mieux)
        let accesses_per_sec = (n as f64) / elapsed_secs;
        Ok(accesses_per_sec as u64)
    }
}

// 7. Mémoire fileté
pub struct MemoryThreaded;
impl Benchmark for MemoryThreaded {
    fn name(&self) -> &str { "Mem Threaded" }
    fn weight(&self) -> u64 { 2 }

    fn run(&self) -> Result<u64> {
        let threads = num_cpus::get();
        // per-thread local buffer size, but cap overall allocation to half RAM
        let sys = get_system_info();
        let total_bytes = sys.total_memory() * 1024;
        let per_thread_default: u64 = 100u64 * 1024 * 1024;
        let per_thread_cap = (total_bytes / 2) / threads as u64;
        let size_bytes = std::cmp::min(per_thread_default, per_thread_cap);
        let size = size_bytes as usize;
        let start = Instant::now();
        let mut handles = Vec::new();
        for _ in 0..threads {
            handles.push(thread::spawn(move || {
                let mut local = vec![0u8; size];
                for i in 0..local.len() {
                    local[i] = (i % 255) as u8;
                }
            }));
        }
        for h in handles { h.join().ok(); }
        let elapsed = start.elapsed().as_secs_f64();
        let mb = (size as f64 * threads as f64) / (1024.0 * 1024.0);
        Ok((mb / elapsed) as u64)
    }
}