use rayon::prelude::*;
use std::time::Instant;
use std::io::Write;
use anyhow::Result;
use crate::engines::benchmark::Benchmark;
use flate2::{Compression, write::ZlibEncoder};
use sha2::{Sha256, Digest};
use rand::{Rng, SeedableRng};
use rand::rngs::StdRng;

// multi-core generic loop
pub struct CpuMultiCore;
impl Benchmark for CpuMultiCore {
    fn name(&self) -> &str { "CPU Multi-Core" }
    fn weight(&self) -> u64 { 3 }

    fn run(&self) -> Result<u64> {
        let start = Instant::now();
        let duration_secs = 5;
        let mut iterations: u64 = 0;
        while start.elapsed().as_secs() < duration_secs {
            let batch: u64 = (0..1_000_000)
                .into_par_iter()
                .map(|i: u64| i.wrapping_mul(6364136223846793005).wrapping_add(1))
                .count() as u64;
            iterations += batch;
        }
        let elapsed = start.elapsed().as_secs_f64();
        Ok((iterations as f64 / elapsed) as u64)
    }
}

// 1. math entier
pub struct CpuIntMath;
impl Benchmark for CpuIntMath {
    fn name(&self) -> &str { "CPU Int Math" }
    fn weight(&self) -> u64 { 2 }
    fn run(&self) -> Result<u64> {
        let start = Instant::now();
        let mut x: u64 = 1;
        while start.elapsed().as_secs() < 5 {
            x = x.wrapping_mul(123456789).wrapping_add(987654321);
            x = x.wrapping_sub(54321);
        }
        Ok(x)
    }
}

// 2. math flottante
pub struct CpuFloatMath;
impl Benchmark for CpuFloatMath {
    fn name(&self) -> &str { "CPU Float Math" }
    fn weight(&self) -> u64 { 2 }
    fn run(&self) -> Result<u64> {
        let start = Instant::now();
        let mut x: f64 = 1.0;
        while start.elapsed().as_secs() < 5 {
            x = x * 1.0000001 + 0.0000001;
            x = x.sin().cos();
        }
        Ok(x as u64)
    }
}

// 3. calcul des nombres premiers
pub struct CpuPrimeCalc;
impl Benchmark for CpuPrimeCalc {
    fn name(&self) -> &str { "CPU Prime Calc" }
    fn weight(&self) -> u64 { 2 }
    fn run(&self) -> Result<u64> {
        let mut count = 0;
        let start = Instant::now();
        let mut n = 2;
        while start.elapsed().as_secs() < 5 {
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
        Ok(count)
    }
}

// 4. instructions étendues (simulated via vector ops)
pub struct CpuSSE;
impl Benchmark for CpuSSE {
    fn name(&self) -> &str { "CPU SSE Ext" }
    fn weight(&self) -> u64 { 2 }
    fn run(&self) -> Result<u64> {
        let mut a = vec![1f32; 1_000_000];
        let b = vec![2f32; 1_000_000];
        let start = Instant::now();
        while start.elapsed().as_secs() < 5 {
            for i in 0..a.len() {
                a[i] = a[i] + b[i];
            }
        }
        Ok(0)
    }
}

// 5. compression
pub struct CpuCompression;
impl Benchmark for CpuCompression {
    fn name(&self) -> &str { "CPU Compression" }
    fn weight(&self) -> u64 { 2 }
    fn run(&self) -> Result<u64> {
        let data = vec![0u8; 10_000_000];
        let start = Instant::now();
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder.write_all(&data)?;
        let _ = encoder.finish()?;
        let elapsed = start.elapsed().as_secs_f64();
        Ok((1.0 / elapsed) as u64)
    }
}

// 6. cryptage (SHA256 loop)
pub struct CpuEncryption;
impl Benchmark for CpuEncryption {
    fn name(&self) -> &str { "CPU Encryption" }
    fn weight(&self) -> u64 { 2 }
    fn run(&self) -> Result<u64> {
        let mut hasher = Sha256::new();
        let data = vec![0u8; 1024 * 1024];
        let start = Instant::now();
        while start.elapsed().as_secs() < 5 {
            hasher.update(&data);
            let _ = hasher.finalize_reset();
        }
        Ok(0)
    }
}

// 7. simulation physique simple
pub struct CpuPhysics;
impl Benchmark for CpuPhysics {
    fn name(&self) -> &str { "CPU Physics" }
    fn weight(&self) -> u64 { 2 }
    fn run(&self) -> Result<u64> {
        let mut pos = vec![0f64; 100000];
        let vel = vec![1f64; 100000];
        let start = Instant::now();
        while start.elapsed().as_secs() < 5 {
            for i in 0..pos.len() {
                pos[i] += vel[i];
            }
        }
        Ok(0)
    }
}

// 8. tri
pub struct CpuSorting;
impl Benchmark for CpuSorting {
    fn name(&self) -> &str { "CPU Sorting" }
    fn weight(&self) -> u64 { 2 }
    fn run(&self) -> Result<u64> {
        let mut rng = StdRng::seed_from_u64(123);
        let mut v: Vec<u64> = (0..1_000_000).map(|_| rng.gen()).collect();
        let start = Instant::now();
        v.sort();
        let elapsed = start.elapsed().as_secs_f64();
        Ok((1.0 / elapsed) as u64)
    }
}

// 9. uct single thread (dummy loop)
pub struct CpuUCT;
impl Benchmark for CpuUCT {
    fn name(&self) -> &str { "CPU UCT Single" }
    fn weight(&self) -> u64 { 2 }
    fn run(&self) -> Result<u64> {
        let start = Instant::now();
        let mut count = 0;
        while start.elapsed().as_secs() < 5 {
            // simulate tree search
            for _ in 0..10000 {
                count += 1;
            }
        }
        Ok(count)
    }
}