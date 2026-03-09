use std::hint::black_box;
use std::thread;
use std::time::Instant;

use anyhow::Result;
use rand::{rngs::StdRng, Rng, SeedableRng};
use rand::seq::SliceRandom;

use crate::engines::benchmark::Benchmark;
use crate::util::sysinfo::get_system_info;

/// Taille par défaut des gros buffers mémoire utilisés pour les tests séquentiels.
const DEFAULT_STREAM_MEM_BYTES: u64 = 512 * 1024 * 1024; // 512 MiB
/// Taille minimale pour garder un signal exploitable même sur petites machines.
const MIN_STREAM_MEM_BYTES: u64 = 64 * 1024 * 1024; // 64 MiB
/// Fraction maximale de la RAM totale à utiliser pour un seul test mémoire.
const MAX_FRACTION_OF_RAM: f64 = 0.25; // 25%

fn bytes_to_mb(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}

/// Choisit une taille de buffer raisonnable en fonction de la RAM disponible.
fn choose_buffer_size_bytes(default: u64, min_bytes: u64) -> u64 {
    let sys = get_system_info();
    let total_bytes = sys.total_memory().saturating_mul(1024); // KB -> bytes
    if total_bytes == 0 {
        return default.max(min_bytes);
    }

    let cap_by_fraction = (total_bytes as f64 * MAX_FRACTION_OF_RAM).round() as u64;
    let mut size = default.min(cap_by_fraction);
    if size < min_bytes {
        size = min_bytes.min(total_bytes / 2);
    }

    // Ne jamais dépasser la moitié de la RAM physique pour ce test.
    size.min(total_bytes / 2).max(min_bytes)
}

/// 1. Opérations type base de données (insertions/lectures aléatoires dans un vecteur).
pub struct MemoryDBOps;
impl Benchmark for MemoryDBOps {
    fn name(&self) -> &str {
        "Mem DB Ops"
    }

    fn weight(&self) -> u64 {
        2
    }

    fn run(&self) -> Result<u64> {
        let mut db: Vec<u64> = Vec::with_capacity(1_000_000);
        let start = Instant::now();
        for i in 0..1_000_000_u64 {
            db.push(i);
            let idx = (i / 2) as usize;
            let _ = black_box(db[idx]);
        }
        let elapsed = start.elapsed().as_secs_f64();
        let ops = (2_000_000.0 / elapsed) as u64; // approx operations per second
        black_box(&db);
        Ok(ops)
    }
}

/// 2. Lecture fortement cachée (petit buffer lu de nombreuses fois).
pub struct MemoryCachedRead;
impl Benchmark for MemoryCachedRead {
    fn name(&self) -> &str {
        "Mem Cached Read"
    }

    fn weight(&self) -> u64 {
        2
    }

    fn run(&self) -> Result<u64> {
        let size = 8 * 1024 * 1024; // 8 MiB
        let data = vec![0u8; size];
        let start = Instant::now();
        let mut sum = 0u64;
        let iterations = 200u64;
        for _ in 0..iterations {
            for &b in &data {
                sum = sum.wrapping_add(b as u64);
            }
        }
        let elapsed = start.elapsed().as_secs_f64();
        black_box(sum);

        let total_bytes = size as u64 * iterations;
        let mb = bytes_to_mb(total_bytes);
        Ok((mb / elapsed.max(1e-6)) as u64)
    }
}

/// 3. Lecture séquentielle sur un grand buffer (faible taux de cache).
pub struct MemoryUncachedRead;
impl Benchmark for MemoryUncachedRead {
    fn name(&self) -> &str {
        "Mem Uncached Read"
    }

    fn weight(&self) -> u64 {
        2
    }

    fn run(&self) -> Result<u64> {
        let size_bytes = choose_buffer_size_bytes(DEFAULT_STREAM_MEM_BYTES, MIN_STREAM_MEM_BYTES);
        let size = size_bytes as usize;

        let mut data = vec![0u8; size];

        // Pré-chauffe : toucher toutes les pages pour éviter de mesurer les fautes de page.
        for b in &mut data {
            *b = b.wrapping_add(1);
        }

        let start = Instant::now();
        let mut sum = 0u64;
        for &b in &data {
            sum = sum.wrapping_add(b as u64);
        }
        let elapsed = start.elapsed().as_secs_f64();
        black_box(sum);

        let mb = bytes_to_mb(size_bytes);
        Ok((mb / elapsed.max(1e-6)) as u64)
    }
}

/// 4. Bande passante d'écriture mémoire séquentielle.
pub struct MemoryWrite;
impl Benchmark for MemoryWrite {
    fn name(&self) -> &str {
        "Mem Write"
    }

    fn weight(&self) -> u64 {
        2
    }

    fn run(&self) -> Result<u64> {
        let size_bytes = choose_buffer_size_bytes(DEFAULT_STREAM_MEM_BYTES, MIN_STREAM_MEM_BYTES);
        let size = size_bytes as usize;
        let mut data = vec![0u8; size];
        let start = Instant::now();
        for i in 0..size {
            data[i] = (i & 0xFF) as u8;
        }
        let elapsed = start.elapsed().as_secs_f64();
        // Empêche l'optimisation agressive de la boucle.
        black_box(&data[0..std::cmp::min(1024, size)]);

        let mb = bytes_to_mb(size_bytes);
        Ok((mb / elapsed.max(1e-6)) as u64)
    }
}

/// 5. RAM disponible (en MB) au moment du benchmark.
pub struct MemoryAvailable;
impl Benchmark for MemoryAvailable {
    fn name(&self) -> &str {
        "Mem Available"
    }

    fn weight(&self) -> u64 {
        1
    }

    fn run(&self) -> Result<u64> {
        let sys = get_system_info();
        Ok((sys.available_memory() / 1024) as u64) // MB
    }
}

/// 6. Latence mémoire via accès aléatoires chaînés (pointer chasing).
pub struct MemoryLatency;
impl Benchmark for MemoryLatency {
    fn name(&self) -> &str {
        "Mem Latency"
    }

    fn weight(&self) -> u64 {
        2
    }

    fn run(&self) -> Result<u64> {
        // Taille de l'ensemble de travail ~80 Mo sur plateforme 64 bits.
        let n: usize = 10_000_000;

        // Construire un cycle pseudo-aléatoire sur [0, n).
        let mut order: Vec<usize> = (0..n).collect();
        let mut rng = StdRng::seed_from_u64(0x4D45_4D00);
        order.shuffle(&mut rng);

        let mut next = vec![0usize; n];
        for i in 0..n {
            let from = order[i];
            let to = order[(i + 1) % n];
            next[from] = to;
        }

        let mut idx = 0usize;
        let steps: usize = 20_000_000;
        let start = Instant::now();
        for _ in 0..steps {
            // Accès pointer-chasing : chaque accès dépend du précédent.
            // get_unchecked est sûr ici car `next` est un cycle valide.
            unsafe {
                idx = *next.get_unchecked(idx);
            }
        }
        let elapsed_secs = start.elapsed().as_secs_f64();
        black_box(idx);

        // Convertir en accès par seconde (plus élevé = mieux)
        let accesses_per_sec = (steps as f64) / elapsed_secs.max(1e-6);
        Ok(accesses_per_sec as u64)
    }
}

/// 7. Bande passante mémoire multi-thread (un buffer par cœur logique).
pub struct MemoryThreaded;
impl Benchmark for MemoryThreaded {
    fn name(&self) -> &str {
        "Mem Threaded"
    }

    fn weight(&self) -> u64 {
        2
    }

    fn run(&self) -> Result<u64> {
        let threads = num_cpus::get();
        // per-thread local buffer size, but cap overall allocation to half RAM
        let sys = get_system_info();
        let total_bytes = sys.total_memory().saturating_mul(1024);
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
                black_box(&local[0..std::cmp::min(1024, local.len())]);
            }));
        }
        for h in handles { h.join().ok(); }
        let elapsed = start.elapsed().as_secs_f64();
        let total_bytes = size_bytes * threads as u64;
        let mb = bytes_to_mb(total_bytes);
        Ok((mb / elapsed.max(1e-6)) as u64)
    }
}