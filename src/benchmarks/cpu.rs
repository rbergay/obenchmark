use rayon::prelude::*;
use std::hint::black_box;
use std::io::Write;
use std::time::{Duration, Instant};

use anyhow::Result;
use flate2::{write::ZlibEncoder, Compression};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use sha2::{Digest, Sha256};

use crate::engines::benchmark::Benchmark;

const CPU_BENCH_DURATION: Duration = Duration::from_secs(5);
const MIN_ELAPSED_SEC: f64 = 1e-6;

// multi-core generic loop
pub struct CpuMultiCore;
impl Benchmark for CpuMultiCore {
    fn name(&self) -> &str {
        "CPU Multi-Core"
    }

    fn weight(&self) -> u64 {
        3
    }

    fn run(&self) -> Result<u64> {
        let start = Instant::now();
        let mut iterations: u64 = 0;

        while start.elapsed() < CPU_BENCH_DURATION {
            let batch: u64 = (0u64..1_000_000)
                .into_par_iter()
                .map(|i| {
                    // Mélange entier relativement coûteux et difficile à optimiser.
                    let v = i
                        .wrapping_mul(6364136223846793005)
                        .wrapping_add(1)
                        .rotate_left(17)
                        ^ 0x9E37_79B9_7F4A_7C15;
                    black_box(v);
                    1u64
                })
                .sum();
            iterations += batch;
        }
        let elapsed = start.elapsed().as_secs_f64().max(MIN_ELAPSED_SEC);
        Ok((iterations as f64 / elapsed) as u64)
    }
}

// 1. math entier
pub struct CpuIntMath;
impl Benchmark for CpuIntMath {
    fn name(&self) -> &str {
        "CPU Int Math"
    }

    fn weight(&self) -> u64 {
        2
    }

    fn run(&self) -> Result<u64> {
        let start = Instant::now();
        let mut x: u64 = 1;
        let mut count: u64 = 0;
        let mut acc: u64 = 0;

        while start.elapsed() < CPU_BENCH_DURATION {
            x = x.wrapping_mul(123456789).wrapping_add(987654321);
            x = x.wrapping_sub(54321);
            x ^= x.rotate_left(13);
            acc = acc.wrapping_add(x);
            count += 1;
        }
        black_box(acc);

        let elapsed = start.elapsed().as_secs_f64().max(MIN_ELAPSED_SEC);
        Ok((count as f64 / elapsed) as u64)
    }
}

// 2. math flottante
pub struct CpuFloatMath;
impl Benchmark for CpuFloatMath {
    fn name(&self) -> &str {
        "CPU Float Math"
    }

    fn weight(&self) -> u64 {
        2
    }

    fn run(&self) -> Result<u64> {
        let start = Instant::now();
        let mut x: f64 = 1.0;
        let mut count: u64 = 0;
        let mut acc: f64 = 0.0;

        while start.elapsed() < CPU_BENCH_DURATION {
            x = x.mul_add(1.000_000_1, 0.000_000_1);
            x = (x.sin() + x.cos()).tan();
            acc += x;
            count += 1;
        }
        black_box(acc);

        let elapsed = start.elapsed().as_secs_f64().max(MIN_ELAPSED_SEC);
        Ok((count as f64 / elapsed) as u64)
    }
}

// 3. calcul des nombres premiers
pub struct CpuPrimeCalc;
impl Benchmark for CpuPrimeCalc {
    fn name(&self) -> &str {
        "CPU Prime Calc"
    }

    fn weight(&self) -> u64 {
        2
    }

    fn run(&self) -> Result<u64> {
        let mut count = 0;
        let start = Instant::now();
        let mut n = 2;
        while start.elapsed() < CPU_BENCH_DURATION {
            let mut is_prime = true;
            for i in 2..((n as f64).sqrt() as u64 + 1) {
                if n % i == 0 {
                    is_prime = false;
                    break;
                }
            }
            if is_prime { count += 1; }
            n += 1;
        }
        black_box(count);
        Ok(count)
    }
}

// 4. instructions étendues (simulated via vector ops)
pub struct CpuSSE;
impl Benchmark for CpuSSE {
    fn name(&self) -> &str {
        "CPU SSE Ext"
    }

    fn weight(&self) -> u64 {
        2
    }

    fn run(&self) -> Result<u64> {
        let mut a = vec![1f32; 1_000_000];
        let b = vec![2f32; 1_000_000];
        let start = Instant::now();
        let mut count: u64 = 0;
        while start.elapsed() < CPU_BENCH_DURATION {
            for i in 0..a.len() {
                a[i] = a[i] + b[i];
            }
            count += 1;
        }
        let checksum: f32 = a.iter().copied().sum();
        black_box(checksum);

        let elapsed = start.elapsed().as_secs_f64().max(MIN_ELAPSED_SEC);
        Ok(((a.len() as u64 * count) as f64 / elapsed) as u64)
    }
}

// 5. compression
pub struct CpuCompression;
impl Benchmark for CpuCompression {
    fn name(&self) -> &str {
        "CPU Compression"
    }

    fn weight(&self) -> u64 {
        2
    }

    fn run(&self) -> Result<u64> {
        let data = vec![0u8; 10_000_000];
        let start = Instant::now();
        let mut total_bytes: u64 = 0;

        while start.elapsed() < CPU_BENCH_DURATION {
            let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
            encoder.write_all(&data)?;
            let compressed = encoder.finish()?;
            black_box(&compressed);
            total_bytes += data.len() as u64;
        }

        let elapsed = start.elapsed().as_secs_f64().max(MIN_ELAPSED_SEC);
        let mb_per_sec = (total_bytes as f64 / (1024.0 * 1024.0)) / elapsed;
        Ok(mb_per_sec as u64)
    }
}

// 6. cryptage (SHA256 loop)
pub struct CpuEncryption;
impl Benchmark for CpuEncryption {
    fn name(&self) -> &str {
        "CPU Encryption"
    }

    fn weight(&self) -> u64 {
        2
    }

    fn run(&self) -> Result<u64> {
        let data = vec![0u8; 1024 * 1024];
        let mut hasher = Sha256::new();
        let start = Instant::now();
        let mut count: u64 = 0;
        let mut total_bytes: u64 = 0;

        while start.elapsed() < CPU_BENCH_DURATION {
            hasher.update(&data);
            let digest = hasher.finalize_reset();
            black_box(digest);
            count += 1;
            total_bytes += data.len() as u64;
        }

        let elapsed = start.elapsed().as_secs_f64().max(MIN_ELAPSED_SEC);
        let mb_per_sec = (total_bytes as f64 / (1024.0 * 1024.0)) / elapsed;
        Ok(mb_per_sec as u64)
    }
}

// 7. simulation physique simple
pub struct CpuPhysics;
impl Benchmark for CpuPhysics {
    fn name(&self) -> &str {
        "CPU Physics"
    }

    fn weight(&self) -> u64 {
        2
    }

    fn run(&self) -> Result<u64> {
        let mut pos = vec![0f64; 100000];
        let vel = vec![1f64; 100000];
        let start = Instant::now();
        let mut count: u64 = 0;
        while start.elapsed() < CPU_BENCH_DURATION {
            for i in 0..pos.len() {
                pos[i] += vel[i];
            }
            count += 1;
        }
        let energy: f64 = pos.iter().map(|p| p * p).sum();
        black_box(energy);

        let elapsed = start.elapsed().as_secs_f64().max(MIN_ELAPSED_SEC);
        Ok(((pos.len() as u64 * count) as f64 / elapsed) as u64)
    }
}

// 8. tri
pub struct CpuSorting;
impl Benchmark for CpuSorting {
    fn name(&self) -> &str {
        "CPU Sorting"
    }

    fn weight(&self) -> u64 {
        2
    }

    fn run(&self) -> Result<u64> {
        let mut rng = StdRng::seed_from_u64(123);
        let start = Instant::now();
        let mut total_items: u64 = 0;

        while start.elapsed() < CPU_BENCH_DURATION {
            let mut v: Vec<u64> = (0..1_000_000).map(|_| rng.gen()).collect();
            v.sort_unstable();
            black_box(&v[0..std::cmp::min(16, v.len())]);
            total_items += v.len() as u64;
        }

        let elapsed = start.elapsed().as_secs_f64().max(MIN_ELAPSED_SEC);
        let items_per_sec = (total_items as f64 / elapsed) as u64;
        Ok(items_per_sec)
    }
}

// 9. uct single thread (dummy loop)
pub struct CpuUCT;
impl Benchmark for CpuUCT {
    fn name(&self) -> &str {
        "CPU UCT Single"
    }

    fn weight(&self) -> u64 {
        2
    }

    fn run(&self) -> Result<u64> {
        let start = Instant::now();
        let mut rng = StdRng::seed_from_u64(0xC0FFEE);
        let mut count: u64 = 0;
        let mut value_acc: f64 = 0.0;

        // Petite simulation de Monte-Carlo / UCT très simplifiée.
        while start.elapsed() < CPU_BENCH_DURATION {
            let mut visits = 1.0;
            let mut total_value = 0.0;

            for _ in 0..10_000 {
                let reward: f64 = rng.gen();
                total_value += reward;
                visits += 1.0;
                let uct = total_value / visits + (2.0 * (visits.ln() + 1.0)).sqrt();
                value_acc += uct;
                count += 1;
            }
        }

        black_box(value_acc);
        Ok(count)
    }
}