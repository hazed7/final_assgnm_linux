use serde::{Deserialize, Serialize};
use std::fs::{self};
use std::path::PathBuf;
use crate::monitor::SystemMetrics;
use crate::error::MonitorError;
use crate::config::Config;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Snapshot {
    pub iteration: usize,
    pub timestamp: chrono::DateTime<chrono::Local>,
    pub metrics: SystemMetrics,
}

impl Snapshot {
    pub fn new(iteration: usize, metrics: SystemMetrics) -> Self {
        Self {
            iteration,
            timestamp: chrono::Local::now(),
            metrics,
        }
    }

    pub fn print_compact(&self) {
        let m = &self.metrics.memory;
        let l = &self.metrics.load;
        let s = &self.metrics.swap;
        let c = &self.metrics.cpu;

        println!("  MEM: {:.1}% ({}/{} MB) | LOAD: {:.2} ({:.0}%) | CPU: {:.1}% | SWAP: {:.1}%",
                 m.used_percent,
                 m.used_kb / 1024,
                 m.total_kb / 1024,
                 l.load_average_1min,
                 l.load_percent_1min,
                 c.cpu_usage_percent,
                 s.used_percent);

        if !self.metrics.critical_events.is_empty() {
            println!("{} critical events detected", self.metrics.critical_events.len());
        }
        println!();
    }
}

pub fn save_snapshots_incremental(snapshots: &[Snapshot], filename: &str) -> Result<PathBuf, MonitorError> {
    let path = PathBuf::from(filename);
    let json = serde_json::to_string_pretty(snapshots)?;
    fs::write(&path, json)?;
    Ok(path)
}

pub fn load_snapshots_from_file(filename: &str) -> Result<Vec<Snapshot>, MonitorError> {
    let content = fs::read_to_string(filename)?;
    let snapshots: Vec<Snapshot> = serde_json::from_str(&content)?;
    Ok(snapshots)
}

#[derive(Debug, Serialize, Deserialize)]
pub struct FinalReport {
    pub report_id: String,
    pub generation_time: chrono::DateTime<chrono::Local>,
    pub config: Config,
    pub snapshots: Vec<Snapshot>,
    pub statistics: Statistics,
    pub summary: ReportSummary,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Statistics {
    pub total_snapshots: usize,
    pub duration_seconds: i64,
    pub memory_stats: ResourceStats,
    pub cpu_stats: ResourceStats,
    pub load_stats: ResourceStats,
    pub swap_stats: ResourceStats,
    pub total_critical_events: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ResourceStats {
    pub min: f64,
    pub max: f64,
    pub avg: f64,
    pub final_value: f64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReportSummary {
    pub overall_status: String,
    pub critical_issues: usize,
    pub warnings: usize,
}

impl FinalReport {
    pub fn new(snapshots: Vec<Snapshot>, config: Config) -> Self {
        let statistics = Self::calculate_statistics(&snapshots);
        let summary = Self::generate_summary(&snapshots, &statistics);
        let report_id = format!("final_report_{}", 
            chrono::Local::now().format("%Y%m%d_%H%M%S"));

        Self {
            report_id,
            generation_time: chrono::Local::now(),
            config,
            snapshots,
            statistics,
            summary,
        }
    }

    fn calculate_statistics(snapshots: &[Snapshot]) -> Statistics {
        if snapshots.is_empty() {
            return Statistics {
                total_snapshots: 0,
                duration_seconds: 0,
                memory_stats: ResourceStats { min: 0.0, max: 0.0, avg: 0.0, final_value: 0.0 },
                cpu_stats: ResourceStats { min: 0.0, max: 0.0, avg: 0.0, final_value: 0.0 },
                load_stats: ResourceStats { min: 0.0, max: 0.0, avg: 0.0, final_value: 0.0 },
                swap_stats: ResourceStats { min: 0.0, max: 0.0, avg: 0.0, final_value: 0.0 },
                total_critical_events: 0,
            };
        }

        let duration = if snapshots.len() > 1 {
            (snapshots.last().unwrap().timestamp - snapshots.first().unwrap().timestamp)
                .num_seconds()
        } else { 0 };

        let mem_values: Vec<f64> = snapshots.iter().map(|s| s.metrics.memory.used_percent).collect();
        let memory_stats = Self::calc_stats(&mem_values);

        let cpu_values: Vec<f64> = snapshots.iter().map(|s| s.metrics.cpu.cpu_usage_percent).collect();
        let cpu_stats = Self::calc_stats(&cpu_values);

        let load_values: Vec<f64> = snapshots.iter().map(|s| s.metrics.load.load_percent_1min).collect();
        let load_stats = Self::calc_stats(&load_values);

        let swap_values: Vec<f64> = snapshots.iter().map(|s| s.metrics.swap.used_percent).collect();
        let swap_stats = Self::calc_stats(&swap_values);

        let total_critical_events: usize = snapshots.iter()
            .map(|s| s.metrics.critical_events.len())
            .sum();

        Statistics {
            total_snapshots: snapshots.len(),
            duration_seconds: duration,
            memory_stats,
            cpu_stats,
            load_stats,
            swap_stats,
            total_critical_events,
        }
    }

    fn calc_stats(values: &[f64]) -> ResourceStats {
        if values.is_empty() {
            return ResourceStats { min: 0.0, max: 0.0, avg: 0.0, final_value: 0.0 };
        }
        let min = values.iter().cloned().fold(f64::INFINITY, f64::min);
        let max = values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let avg = values.iter().sum::<f64>() / values.len() as f64;
        let final_value = *values.last().unwrap_or(&0.0);
        ResourceStats { min, max, avg, final_value }
    }

    fn generate_summary(snapshots: &[Snapshot], stats: &Statistics) -> ReportSummary {
        let mut critical_issues = 0;
        let mut warnings = 0;

        for snapshot in snapshots {
            for event in &snapshot.metrics.critical_events {
                match event.severity.as_str() {
                    "CRITICAL" => critical_issues += 1,
                    "HIGH" => warnings += 1,
                    _ => {}
                }
            }
        }

        let overall_status = if critical_issues > 0 {
            "CRITICAL"
        } else if warnings > 0 {
            "WARNING"
        } else {
            "STABLE"
        }.to_string();

        ReportSummary { overall_status, critical_issues, warnings }
    }

    pub fn save_to_file(&self) -> Result<PathBuf, MonitorError> {
        let filename = format!("{}.json", self.report_id);
        let path = PathBuf::from(&filename);
        let json = serde_json::to_string_pretty(&self)?;
        fs::write(&path, json)?;
        println!("\nFinal report saved: {}", filename);
        Ok(path)
    }

    pub fn print_summary(&self) {
        println!("\n=== FINAL DIAGNOSTIC REPORT ===");
        println!("Status: {}", self.summary.overall_status);
        println!("Duration: {} seconds ({} minutes)", 
                 self.statistics.duration_seconds,
                 self.statistics.duration_seconds / 60);
        println!("Total Snapshots: {}", self.statistics.total_snapshots);
        println!("-- MEMORY:   min {:.1}% | max {:.1}% | avg {:.1}% | final {:.1}%",
                 self.statistics.memory_stats.min,
                 self.statistics.memory_stats.max,
                 self.statistics.memory_stats.avg,
                 self.statistics.memory_stats.final_value);
        println!("-- CPU:      min {:.1}% | max {:.1}% | avg {:.1}% | final {:.1}%",
                 self.statistics.cpu_stats.min,
                 self.statistics.cpu_stats.max,
                 self.statistics.cpu_stats.avg,
                 self.statistics.cpu_stats.final_value);
        println!("-- LOAD(%):  min {:.1}% | max {:.1}% | avg {:.1}% | final {:.1}%",
                 self.statistics.load_stats.min,
                 self.statistics.load_stats.max,
                 self.statistics.load_stats.avg,
                 self.statistics.load_stats.final_value);
        println!("-- SWAP:     min {:.1}% | max {:.1}% | avg {:.1}% | final {:.1}%",
                 self.statistics.swap_stats.min,
                 self.statistics.swap_stats.max,
                 self.statistics.swap_stats.avg,
                 self.statistics.swap_stats.final_value);
        println!("Critical events: {} | Warnings: {}",
                 self.summary.critical_issues, self.summary.warnings);
    }
}
