use crossbeam_channel::Sender;
use crate::engines::benchmark::Benchmark;
use crate::model::result::{BenchResult, BenchScore};
use crate::engines::score::compute_aggregated_scores;
use crate::util::sysinfo::get_detailed_system_info;

pub enum RunnerEvent {
    BenchStarted(String),
    BenchFinished(String, u64),
    Done(BenchResult),
    Error(String),
}

pub fn run_benchmarks(
    benches: Vec<Box<dyn Benchmark>>,
    tx: Sender<RunnerEvent>,
) {
    std::thread::spawn(move || {
        let mut scores: Vec<BenchScore> = Vec::new();

        for bench in benches {
            let name = bench.name().to_string();
            let weight = bench.weight();

            tx.send(RunnerEvent::BenchStarted(name.clone())).ok();

            match bench.run() {
                Ok(score) => {
                    scores.push(BenchScore {
                        name: name.clone(),
                        raw_score: score,
                        weight,
                    });
                    tx.send(RunnerEvent::BenchFinished(name.clone(), score)).ok();
                }
                Err(e) => {
                    tx.send(RunnerEvent::Error(e.to_string())).ok();
                    return;
                }
            }
        }

        let aggregated = compute_aggregated_scores(&scores);
        let system_info = get_detailed_system_info();

        tx.send(RunnerEvent::Done(BenchResult {
            scores,
            final_score: aggregated.global,
            cpu_score: aggregated.cpu,
            mem_score: aggregated.mem,
            disk_score: aggregated.disk,
            system_info: Some(system_info),
        }))
        .ok();
    });
}