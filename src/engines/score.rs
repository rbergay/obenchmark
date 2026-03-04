use crate::model::result::BenchScore;

/// Baselines pour score normalisé
/// ~1000 = machine de référence
const CPU_BASELINE: u64 = 50_000_000;
const MEM_BASELINE: u64 = 5000;
const DISK_BASELINE: u64 = 1000;

#[derive(Clone, Copy, Debug, Default)]
pub struct AggregatedScores {
    pub global: u64,
    pub cpu: u64,
    pub mem: u64,
    pub disk: u64,
}

pub fn normalize(name: &str, raw_score: u64) -> u64 {
    // Map many benchmark names to coarse categories so baselines stay meaningful
    let baseline = if name.contains("CPU") {
        CPU_BASELINE
    } else if name.to_lowercase().contains("mem") || name.to_lowercase().contains("memory") {
        MEM_BASELINE
    } else if name.to_lowercase().contains("disk") || name.to_lowercase().contains("iops") {
        DISK_BASELINE
    } else {
        1000
    };

    // Normaliser autour de 1000 par rapport à la baseline
    let mut norm = ((raw_score as f64 / baseline as f64) * 1000.0) as u64;
    // Autoriser un écart plus important entre machines avant saturation
    const PER_BENCH_MAX: u64 = 100_000;
    if norm > PER_BENCH_MAX { norm = PER_BENCH_MAX; }
    norm
}

fn classify(name: &str) -> &'static str {
    if name.contains("CPU") {
        "cpu"
    } else if name.to_lowercase().contains("mem") || name.to_lowercase().contains("memory") {
        "mem"
    } else if name.to_lowercase().contains("disk") || name.to_lowercase().contains("iops") {
        "disk"
    } else {
        "other"
    }
}

pub fn compute_aggregated_scores(scores: &[BenchScore]) -> AggregatedScores {
    // Utiliser u128 pour éviter les débordements intermédiaires
    let mut total_weight_global: u128 = 0;
    let mut total_score_global: u128 = 0;

    let mut total_weight_cpu: u128 = 0;
    let mut total_score_cpu: u128 = 0;

    let mut total_weight_mem: u128 = 0;
    let mut total_score_mem: u128 = 0;

    let mut total_weight_disk: u128 = 0;
    let mut total_score_disk: u128 = 0;

    for s in scores {
        let normalized = normalize(&s.name, s.raw_score) as u128;
        let weight = s.weight as u128;
        eprintln!("[score] {} -> normalized={} weight={}", s.name, normalized, s.weight);

        // global
        total_score_global = total_score_global.saturating_add(normalized.saturating_mul(weight));
        total_weight_global = total_weight_global.saturating_add(weight);

        // par catégorie
        match classify(&s.name) {
            "cpu" => {
                total_score_cpu = total_score_cpu.saturating_add(normalized.saturating_mul(weight));
                total_weight_cpu = total_weight_cpu.saturating_add(weight);
            }
            "mem" => {
                total_score_mem = total_score_mem.saturating_add(normalized.saturating_mul(weight));
                total_weight_mem = total_weight_mem.saturating_add(weight);
            }
            "disk" => {
                total_score_disk =
                    total_score_disk.saturating_add(normalized.saturating_mul(weight));
                total_weight_disk = total_weight_disk.saturating_add(weight);
            }
            _ => {}
        }
    }

    let compute_avg = |total_score: u128, total_weight: u128| -> u64 {
        if total_weight == 0 {
            0
        } else {
            let averaged = (total_score / total_weight) as u128;
            eprintln!(
                "[score] category total_score={} total_weight={} averaged={}",
                total_score, total_weight, averaged
            );
            let capped = if averaged > 999_999u128 { 999_999u128 } else { averaged };
            capped as u64
        }
    };

    AggregatedScores {
        global: compute_avg(total_score_global, total_weight_global),
        cpu: compute_avg(total_score_cpu, total_weight_cpu),
        mem: compute_avg(total_score_mem, total_weight_mem),
        disk: compute_avg(total_score_disk, total_weight_disk),
    }
}

pub fn compute_final_score(scores: &[BenchScore]) -> u64 {
    compute_aggregated_scores(scores).global
}