use std::fs::{File, OpenOptions};
use std::io::{Write, Read, Seek, SeekFrom};
use std::time::Instant;
use anyhow::Result;
use crate::engines::benchmark::Benchmark;
use rand::{rngs::StdRng, SeedableRng, Rng};

// Test 1: Lecture séquentielle
pub struct DiskSequentialRead;

impl Benchmark for DiskSequentialRead {
    fn name(&self) -> &str { "Disk Seq Read" }
    fn weight(&self) -> u64 { 2 }

    fn run(&self) -> Result<u64> {
        let size = 512 * 1024 * 1024; // 512 MB
        let data = vec![1u8; size];

        // Écrire le fichier
        let mut file = File::create("benchmark_seq_read.dat")?;
        file.write_all(&data)?;
        file.sync_all()?;
        drop(file);

        // Tester la lecture
        let start = Instant::now();
        let mut file = File::open("benchmark_seq_read.dat")?;
        let mut buffer = vec![0u8; size];
        file.read_exact(&mut buffer)?;
        let elapsed = start.elapsed().as_secs_f64();

        std::fs::remove_file("benchmark_seq_read.dat")?;

        let speed_mb_s = (size as f64 / (1024.0 * 1024.0)) / elapsed;
        Ok(speed_mb_s as u64)
    }
}

// Test 2: Écriture séquentielle
pub struct DiskSequentialWrite;

impl Benchmark for DiskSequentialWrite {
    fn name(&self) -> &str { "Disk Seq Write" }
    fn weight(&self) -> u64 { 2 }

    fn run(&self) -> Result<u64> {
        let size = 512 * 1024 * 1024; // 512 MB
        let data = vec![1u8; size];

        let start = Instant::now();
        let mut file = File::create("benchmark_seq_write.dat")?;
        file.write_all(&data)?;
        file.sync_all()?;
        let elapsed = start.elapsed().as_secs_f64();

        std::fs::remove_file("benchmark_seq_write.dat")?;

        let speed_mb_s = (size as f64 / (1024.0 * 1024.0)) / elapsed;
        Ok(speed_mb_s as u64)
    }
}

// Test 3: IOPS 32K QD20
pub struct DiskRandomIOPS32K;

impl Benchmark for DiskRandomIOPS32K {
    fn name(&self) -> &str { "Disk IOPS 32K QD20" }
    fn weight(&self) -> u64 { 2 }

    fn run(&self) -> Result<u64> {
        let block_size = 32 * 1024; // 32 KB
        let _queue_depth = 20;
        let total_ops = 10000;
        let file_size = 1024 * 1024 * 1024; // 1 GB

        // Créer le fichier de test
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .open("benchmark_iops_32k.dat")?;
        file.set_len(file_size as u64)?;
        drop(file);

        // Simuler les IOPS avec queue depth (offsets déterministes)
        let mut rng = StdRng::seed_from_u64(123456789);
        let start = Instant::now();
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open("benchmark_iops_32k.dat")?;
        let mut buffer = vec![0u8; block_size];

        for _ in 0..total_ops {
            let offset = rng.gen_range(0..(file_size - block_size)) as u64;
            file.seek(SeekFrom::Start(offset))?;
            let _ = file.read(&mut buffer);
        }
        file.sync_all()?;
        let elapsed = start.elapsed().as_secs_f64();

        std::fs::remove_file("benchmark_iops_32k.dat")?;

        let iops = (total_ops as f64 / elapsed) as u64;
        Ok(iops)
    }
}

// Test 4: IOPS 4K QD1
pub struct DiskRandomIOPS4K;

impl Benchmark for DiskRandomIOPS4K {
    fn name(&self) -> &str { "Disk IOPS 4K QD1" }
    fn weight(&self) -> u64 { 2 }

    fn run(&self) -> Result<u64> {
        let block_size = 4 * 1024; // 4 KB
        let total_ops = 10000;
        let file_size = 1024 * 1024 * 1024; // 1 GB

        // Créer le fichier de test
        let file = OpenOptions::new()
            .write(true)
            .create(true)
            .open("benchmark_iops_4k.dat")?;
        file.set_len(file_size as u64)?;
        drop(file);

        // Simuler les IOPS QD1 (une opération à la fois) avec offsets déterministes
        let mut rng = StdRng::seed_from_u64(987654321);
        let start = Instant::now();
        let mut file = OpenOptions::new()
            .read(true)
            .write(true)
            .open("benchmark_iops_4k.dat")?;
        let mut buffer = vec![0u8; block_size];

        for _ in 0..total_ops {
            let offset = rng.gen_range(0..(file_size - block_size)) as u64;
            file.seek(SeekFrom::Start(offset))?;
            let _ = file.read(&mut buffer);
        }
        file.sync_all()?;
        let elapsed = start.elapsed().as_secs_f64();

        std::fs::remove_file("benchmark_iops_4k.dat")?;

        let iops = (total_ops as f64 / elapsed) as u64;
        Ok(iops)
    }
}