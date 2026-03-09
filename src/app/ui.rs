use chrono::Local;
use crossbeam_channel::{unbounded, Receiver};
use iced::alignment::Horizontal;
use iced::time;
use iced::widget::{
    button, column, row, scrollable, text, horizontal_rule, progress_bar,
};
use iced::{Application, Command, Element, Length, Subscription, Theme};

use std::time::Duration;

// Rapport matériel
use crate::app::hw_check::{evaluate_hw, HwCheckResult};

// App logic
use crate::{
    engines::runner::{run_benchmarks, RunnerEvent},
    benchmarks::{
        cpu::{
            CpuMultiCore, CpuIntMath, CpuFloatMath, CpuPrimeCalc, CpuSSE,
            CpuCompression, CpuEncryption, CpuPhysics, CpuSorting, CpuUCT,
        },
        memory::{
            MemoryDBOps, MemoryCachedRead, MemoryUncachedRead, MemoryWrite,
            MemoryAvailable, MemoryLatency, MemoryThreaded,
        },
        disk::{
            DiskSequentialRead, DiskSequentialWrite, DiskRandomIOPS32K,
            DiskRandomIOPS4K,
        },
    },
    app::state::AppState,
};

pub struct OBenchmarkApp {
    state: AppState,
    receiver: Option<Receiver<RunnerEvent>>,
}

#[derive(Debug, Clone)]
pub enum Msg {
    Start,
    Tick,
    Export,
    Restart,
}

/*--------------------------------------------------------------
                    UNITÉS DE MESURE PAR BENCH
--------------------------------------------------------------*/
fn unit_for_bench(name: &str) -> &'static str {
    match name {
        // CPU
        "CPU Multi-Core"
        | "CPU Int Math"
        | "CPU Float Math"
        | "CPU Prime Calc"
        | "CPU SSE Ext"
        | "CPU Physics"
        | "CPU Sorting"
        | "CPU UCT Single" => "ops/s",

        "CPU Compression" | "CPU Encryption" => "MB/s",

        // RAM
        "Mem DB Ops" => "ops/s",
        "Mem Cached Read"
        | "Mem Uncached Read"
        | "Mem Write"
        | "Mem Threaded" => "MB/s",

        "Mem Available" => "MB",
        "Mem Latency" => "accès/s",

        // DISK
        "Disk Seq Read" | "Disk Seq Write" => "MB/s",
        "Disk IOPS 32K QD20" | "Disk IOPS 4K QD1" => "IOPS",

        _ => "",
    }
}

/*--------------------------------------------------------------
                    WIDGET : RAPPORT MATÉRIEL
--------------------------------------------------------------*/
use iced::{Color};

pub fn hw_report_widget(hw: &HwCheckResult) -> Element<'static, Msg> {
    fn color(ok: bool) -> Color {
        if ok {
            Color::from_rgb(0.0, 0.7, 0.0)
        } else {
            Color::from_rgb(0.9, 0.0, 0.0)
        }
    }

    fn item(label: &str, ok: bool) -> Element<'static, Msg> {
        row![
            text(label).size(20),
            text(if ok { "OK" } else { "Insuffisant" })
                .style(iced::theme::Text::Color(color(ok)))
                .size(20)
        ]
        .spacing(12)
        .into()
    }

    column![
        text("Analyse matériel PostgreSQL").size(26),
        item("CPU :", hw.cpu_ok),
        item("RAM :", hw.ram_ok),
        item("Disques :", hw.disk_ok),
        horizontal_rule(1)
    ]
    .spacing(8)
    .into()
}

/*--------------------------------------------------------------
                    APPLICATION ICED
--------------------------------------------------------------*/
impl Application for OBenchmarkApp {
    type Executor = iced::executor::Default;
    type Flags = ();
    type Message = Msg;
    type Theme = Theme;

    fn new(_flags: Self::Flags) -> (Self, Command<Self::Message>) {
        (
            Self {
                state: AppState::Idle,
                receiver: None,
            },
            Command::none(),
        )
    }

    fn title(&self) -> String {
        "OBenchmark".to_string()
    }

    fn theme(&self) -> Theme {
        Theme::Dark
    }

    fn subscription(&self) -> Subscription<Self::Message> {
        time::every(Duration::from_millis(40)).map(|_| Msg::Tick)
    }

    fn update(&mut self, msg: Self::Message) -> Command<Self::Message> {
        match msg {
            Msg::Start => {
                let (tx, rx) = unbounded();

                let benches: Vec<Box<dyn crate::engines::benchmark::Benchmark>> = vec![
                    Box::new(CpuMultiCore),
                    Box::new(CpuIntMath),
                    Box::new(CpuFloatMath),
                    Box::new(CpuPrimeCalc),
                    Box::new(CpuSSE),
                    Box::new(CpuCompression),
                    Box::new(CpuEncryption),
                    Box::new(CpuPhysics),
                    Box::new(CpuSorting),
                    Box::new(CpuUCT),
                    Box::new(MemoryDBOps),
                    Box::new(MemoryCachedRead),
                    Box::new(MemoryUncachedRead),
                    Box::new(MemoryWrite),
                    Box::new(MemoryAvailable),
                    Box::new(MemoryLatency),
                    Box::new(MemoryThreaded),
                    Box::new(DiskSequentialRead),
                    Box::new(DiskSequentialWrite),
                    Box::new(DiskRandomIOPS32K),
                    Box::new(DiskRandomIOPS4K),
                ];

                let total = benches.len();

                self.state = AppState::Running {
                    current_test: String::new(),
                    completed: 0,
                    total,
                };

                std::thread::spawn(move || {
                    run_benchmarks(benches, tx);
                });

                self.receiver = Some(rx);
            }

            Msg::Tick => {
                if let Some(rx) = &self.receiver {
                    while let Ok(event) = rx.try_recv() {
                        match event {
                            RunnerEvent::BenchStarted(name) => {
                                if let AppState::Running { completed, total, .. } = &self.state {
                                    self.state = AppState::Running {
                                        current_test: name,
                                        completed: *completed,
                                        total: *total,
                                    };
                                }
                            }

                            RunnerEvent::BenchFinished(_, _) => {
                                if let AppState::Running { current_test, completed, total } =
                                    &self.state
                                {
                                    self.state = AppState::Running {
                                        current_test: current_test.clone(),
                                        completed: completed + 1,
                                        total: *total,
                                    };
                                }
                            }

                            RunnerEvent::Done(result) => {
                                self.state = AppState::Showing(result.clone());
                            }

                            RunnerEvent::Error(err) => {
                                self.state = AppState::Error(err);
                            }
                        }
                    }
                }
            }

            Msg::Export => {
                if let AppState::Showing(result) = &self.state {
                    let json = serde_json::to_string_pretty(result).unwrap();
                    let _ = std::fs::write(
                        format!("bench_{}.json", Local::now().timestamp()),
                        json,
                    );
                }
            }

            Msg::Restart => {
                self.state = AppState::Idle;
                self.receiver = None;
            }
        }

        Command::none()
    }

    fn view(&self) -> Element<'_, Msg> {
        fn human_bytes(mut bytes: f64) -> String {
            let units = ["B", "KB", "MB", "GB", "TB"];
            let mut i = 0;
            while bytes >= 1024.0 && i < units.len() - 1 {
                bytes /= 1024.0;
                i += 1;
            }
            if i == 0 {
                format!("{} {}", bytes as u64, units[i])
            } else {
                format!("{:.2} {}", bytes, units[i])
            }
        }

        let mut ui = column![text("OBenchmark").size(32), horizontal_rule(1)]
            .padding(16)
            .spacing(12);

        match &self.state {
            AppState::Idle => {
                ui = ui.push(button("Start Benchmark").on_press(Msg::Start));
            }

            AppState::Running { current_test, completed, total } => {
                let global = if *total > 0 {
                    *completed as f32 / *total as f32
                } else {
                    0.0
                };

                ui = ui
                    .push(text(format!("Test en cours : {}", current_test)).size(20))
                    .push(progress_bar(0.0..=1.0, global));
            }

            AppState::Showing(result) => {
                let mut rows = column![
                    text(format!("Score global : {}", result.final_score)).size(24),
                    text(format!("Score CPU : {}", result.cpu_score)).size(20),
                    text(format!("Score RAM : {}", result.mem_score)).size(20),
                    text(format!("Score Disque : {}", result.disk_score)).size(20),
                    horizontal_rule(1),
                ];

                // Rapport matériel PostgreSQL
                if let Some(sysinfo) = &result.system_info {
                    let hw = evaluate_hw(sysinfo);
                    rows = rows.push(hw_report_widget(&hw));
                }

                // Résultats par bench
                for s in &result.scores {
                    rows = rows.push(
                        row![
                            text(&s.name).width(Length::FillPortion(2)),
                            text(format!("{} {}", s.raw_score, unit_for_bench(&s.name)))
                                .width(Length::FillPortion(1))
                                .horizontal_alignment(Horizontal::Right),
                        ]
                    );
                }

                rows = rows.push(horizontal_rule(1)).push(text("System info"));

                if let Some(si) = &result.system_info {
                    rows = rows
                        .push(text(format!("CPU Vendor: {}", si.cpu.vendor.clone().unwrap_or("unknown".to_string()))))
                        .push(text(format!("CPU Model: {}", si.cpu.model.clone().unwrap_or("unknown".to_string()))))
                        .push(text(format!("Logical cores: {}", si.cpu.cores_logical)));

                    let ram_display = if si.ram.total_mb >= 1024 {
                        format!("{:.2} GB", si.ram.total_mb as f64 / 1024.0)
                    } else {
                        format!("{} MB", si.ram.total_mb)
                    };

                    rows = rows
                        .push(text(format!("RAM Total: {}", ram_display)))
                        .push(text(format!("RAM Type: {}", si.ram.ram_type.clone().unwrap_or("unknown".to_string()))));

                    for d in &si.disks {
                        let size_display = if let Some(b) = d.total_bytes {
                            human_bytes(b as f64)
                        } else {
                            "unknown".to_string()
                        };

                        rows = rows.push(text(format!(
                            "Disk: {} {} {} [{}] (size: {}) mount: {:?}",
                            d.vendor.clone().unwrap_or_default(),
                            d.model.clone().unwrap_or_default(),
                            d.name,
                            d.disk_type.clone().unwrap_or("unknown".to_string()),
                            size_display,
                            d.mount_point
                        )));
                    }
                }

                ui = ui.push(
                    scrollable(
                        column![
                            rows,
                            row![
                                button("Export JSON").on_press(Msg::Export),
                                button("Nouvelle analyse").on_press(Msg::Restart)
                            ]
                            .spacing(10)
                        ]
                    )
                );
            }

            AppState::Error(e) => {
                ui = ui.push(text(format!("Erreur : {}", e)));
            }
        }

        ui.into()
    }
}