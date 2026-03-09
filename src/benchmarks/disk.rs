use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{anyhow, Result};
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};

use crate::engines::benchmark::Benchmark;

/// Taille des fichiers utilisés pour les tests.
const SEQ_FILE_SIZE_BYTES: u64 = 512 * 1024 * 1024; // 512 MiB
const RAND_FILE_SIZE_BYTES: u64 = 1 * 1024 * 1024 * 1024; // 1 GiB

/// Tailles de blocs.
const SEQ_BLOCK_SIZE: usize = 1024 * 1024; // 1 MiB
const RAND_32K_BLOCK_SIZE: usize = 32 * 1024;
const RAND_4K_BLOCK_SIZE: usize = 4 * 1024;

/// Durée des tests random (proche CrystalDiskMark).
const RANDOM_TEST_DURATION_SECS: u64 = 5;

/// Profondeurs de file approximatives (nb de threads).
const QD_32K: usize = 20; // "QD20"
const QD_4K: usize = 1; // "QD1"

fn temp_file(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("obenchmark_{}.bin", name))
}

/// Crée un fichier de taille `target_size` rempli de zéros.
/// Réduit la taille par 2 en cas d'erreur pour gérer le manque d'espace disque.
fn create_test_file_best_effort(name: &str, target_size: u64) -> Result<(PathBuf, u64)> {
    const MIN_SIZE: u64 = 64 * 1024 * 1024; // 64 MiB minimum

    let mut size = target_size;
    let mut last_err: Option<anyhow::Error> = None;
    let path = temp_file(name);

    while size >= MIN_SIZE {
        match create_file_with_size(&path, size) {
            Ok(_) => return Ok((path.clone(), size)),
            Err(e) => {
                last_err = Some(e);
                size /= 2;
            }
        }
    }

    Err(last_err.unwrap_or_else(|| anyhow!("Espace disque insuffisant pour le benchmark disque")))
}

fn create_file_with_size(path: &Path, size: u64) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if path.exists() {
        let _ = std::fs::remove_file(path);
    }

    let mut file = File::create(path)?;

    // Pré-allocation + écriture séquentielle avec un buffer de 1 MiB.
    // On ne mesure PAS ce temps dans les scores.
    file.set_len(size)?;

    let buffer = vec![0u8; SEQ_BLOCK_SIZE];
    let mut remaining = size;
    while remaining > 0 {
        let to_write = std::cmp::min(remaining, buffer.len() as u64) as usize;
        file.write_all(&buffer[..to_write])?;
        remaining -= to_write as u64;
    }
    file.sync_all()?;
    Ok(())
}

fn bytes_per_sec_to_mb_per_sec(bytes: u64, secs: f64) -> u64 {
    if secs <= 0.0 {
        return 0;
    }
    let mb = (bytes as f64) / (1024.0 * 1024.0);
    (mb / secs) as u64
}

fn random_read_iops(path: &Path, file_size: u64, block_size: usize, queue_depth: usize) -> Result<u64> {
    let duration = Duration::from_secs(RANDOM_TEST_DURATION_SECS);
    let start = Instant::now();

    // Limite supérieure pour les offsets aléatoires.
    let max_offset = file_size
        .checked_sub(block_size as u64)
        .ok_or_else(|| anyhow!("Fichier de test trop petit pour la taille de bloc"))?;

    let mut handles = Vec::with_capacity(queue_depth);

    for thread_id in 0..queue_depth {
        let path = path.to_owned();
        let start = start; // Instant est Copy

        let handle = thread::spawn(move || -> Result<u64> {
            let mut rng = StdRng::seed_from_u64(0xD15C_0000 + thread_id as u64);
            let mut file = OpenOptions::new().read(true).open(&path)?;
            let mut buffer = vec![0u8; block_size];
            let mut ops: u64 = 0;

            while start.elapsed() < duration {
                let offset = rng.gen_range(0..=max_offset);
                file.seek(SeekFrom::Start(offset))?;
                file.read_exact(&mut buffer)?;
                ops += 1;
            }

            Ok(ops)
        });

        handles.push(handle);
    }

    let mut total_ops: u64 = 0;
    for h in handles {
        total_ops += h.join().unwrap_or_else(|_| Ok(0))?;
    }

    let secs = duration.as_secs().max(1) as u64;
    Ok(total_ops / secs)
}

/// Lecture séquentielle gros blocs (MB/s).
pub struct DiskSequentialRead;

impl Benchmark for DiskSequentialRead {
    fn name(&self) -> &str {
        "Disk Seq Read"
    }

    fn weight(&self) -> u64 {
        2
    }

    fn run(&self) -> Result<u64> {
        let (path, size) = create_test_file_best_effort("seq_read", SEQ_FILE_SIZE_BYTES)?;

        // Mesure de la lecture seule.
        let mut file = OpenOptions::new().read(true).open(&path)?;
        let mut buffer = vec![0u8; SEQ_BLOCK_SIZE];
        let start = Instant::now();
        let mut read_bytes: u64 = 0;

        loop {
            let n = file.read(&mut buffer)?;
            if n == 0 {
                break;
            }
            read_bytes += n as u64;
        }

        let elapsed = start.elapsed().as_secs_f64();
        let _ = std::fs::remove_file(&path);
        Ok(bytes_per_sec_to_mb_per_sec(std::cmp::min(read_bytes, size), elapsed))
    }
}

/// Écriture séquentielle gros blocs (MB/s).
pub struct DiskSequentialWrite;

impl Benchmark for DiskSequentialWrite {
    fn name(&self) -> &str {
        "Disk Seq Write"
    }

    fn weight(&self) -> u64 {
        2
    }

    fn run(&self) -> Result<u64> {
        let path = temp_file("seq_write");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if path.exists() {
            let _ = std::fs::remove_file(&path);
        }

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&path)?;

        let buffer = vec![0u8; SEQ_BLOCK_SIZE];
        let mut remaining = SEQ_FILE_SIZE_BYTES;

        let start = Instant::now();
        let mut written: u64 = 0;
        while remaining > 0 {
            let to_write = std::cmp::min(remaining, buffer.len() as u64) as usize;
            file.write_all(&buffer[..to_write])?;
            written += to_write as u64;
            remaining -= to_write as u64;
        }
        file.sync_all()?;
        let elapsed = start.elapsed().as_secs_f64();

        let _ = std::fs::remove_file(&path);
        Ok(bytes_per_sec_to_mb_per_sec(written, elapsed))
    }
}

/// IOPS en lecture aléatoire 32K avec profondeur de file ~20.
pub struct DiskRandomIOPS32K;

impl Benchmark for DiskRandomIOPS32K {
    fn name(&self) -> &str {
        "Disk IOPS 32K QD20"
    }

    fn weight(&self) -> u64 {
        2
    }

    fn run(&self) -> Result<u64> {
        let (path, size) = create_test_file_best_effort("rand_32k", RAND_FILE_SIZE_BYTES)?;
        let iops = random_read_iops(&path, size, RAND_32K_BLOCK_SIZE, QD_32K)?;
        let _ = std::fs::remove_file(&path);
        Ok(iops)
    }
}

/// IOPS en lecture aléatoire 4K queue depth 1 (latence).
pub struct DiskRandomIOPS4K;

impl Benchmark for DiskRandomIOPS4K {
    fn name(&self) -> &str {
        "Disk IOPS 4K QD1"
    }

    fn weight(&self) -> u64 {
        2
    }

    fn run(&self) -> Result<u64> {
        let (path, size) = create_test_file_best_effort("rand_4k", RAND_FILE_SIZE_BYTES)?;
        let iops = random_read_iops(&path, size, RAND_4K_BLOCK_SIZE, QD_4K)?;
        let _ = std::fs::remove_file(&path);
        Ok(iops)
    }
}