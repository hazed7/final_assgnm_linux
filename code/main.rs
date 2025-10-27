mod error;
mod memory_leak;
mod monitor;
mod report;
mod config;

use std::sync::{Arc, atomic::{AtomicBool, Ordering}, Mutex};
use std::thread;
use std::time::Duration;
use anyhow::Result;
use clap::Parser;
use crate::config::Config;
use crate::report::{Snapshot, FinalReport, save_snapshots_incremental};
use signal_hook::consts::{SIGTERM, SIGINT};
use signal_hook::iterator::Signals;

fn main() -> Result<()> {
    let config = Config::parse();

    println!("=== SYSTEM MONITOR ===");
    println!("[CONFIG] Memory leak: {} MB every {} seconds", 
             config.leak_size_mb, config.leak_interval_sec);
    println!("[CONFIG] Monitor interval: {} seconds", config.monitor_interval_sec);
    println!("[CONFIG] CPU stress workers: {}", config.cpu_workers);
    println!("[CONFIG] Snapshot file: {}", config.snapshot_file);
    println!();

    let running = Arc::new(AtomicBool::new(true));
    let snapshots = Arc::new(Mutex::new(Vec::new()));

    {
        let r = running.clone();
        let snap_clone = snapshots.clone();
        let cfg_clone = config.clone();
        thread::spawn(move || {
            let mut signals = Signals::new(&[SIGINT, SIGTERM]).expect("Failed to register signals");
            for sig in signals.forever() {
                println!("\n[!] Received signal {}, saving data and shutting down...", sig);
                r.store(false, Ordering::SeqCst);

                let snaps = snap_clone.lock().unwrap();
                if !snaps.is_empty() {
                    println!("[EMERGENCY] Saving {} snapshots before termination...", snaps.len());
                    let report = FinalReport::new(snaps.clone(), cfg_clone.clone());
                    if let Err(e) = report.save_to_file() {
                        eprintln!("[ERROR] Failed to save emergency report: {}", e);
                    } else {
                        report.print_summary();
                    }
                }
                std::process::exit(0);
            }
        });
    }

    let leak_running = running.clone();
    let leak_config = config.clone();
    let leak_handle = thread::spawn(move || {
        memory_leak::spawn_leak_worker(leak_running, leak_config);
    });

    let mut cpu_handles = Vec::new();
    if config.cpu_workers > 0 {
        for i in 0..config.cpu_workers {
            let cpu_running = running.clone();
            let handle = thread::spawn(move || {
                memory_leak::spawn_cpu_stress(cpu_running, i);
            });
            cpu_handles.push(handle);
        }
    }

    let reporter_running = running.clone();
    let reporter_handle = thread::spawn(move || {
        fn fmt_bytes_gb(b: u64) -> f64 { (b as f64) / (1024.0 * 1024.0 * 1024.0) }
        fn fmt_big(n: u64) -> String {
            const K: f64 = 1_000.0;
            const M: f64 = 1_000_000.0;
            const B: f64 = 1_000_000_000.0;
            let nf = n as f64;
            if nf >= B { format!("{:.1} B", nf / B) }
            else if nf >= M { format!("{:.1} M", nf / M) }
            else if nf >= K { format!("{:.1} K", nf / K) }
            else { n.to_string() }
        }

        let interval = Duration::from_secs(5);
        while reporter_running.load(Ordering::SeqCst) {
            let leak_gb = fmt_bytes_gb(memory_leak::leak_total_bytes());
            let workers = memory_leak::cpu_active_workers();
            let cycles = memory_leak::cpu_total_cycles();
            println!(
                "[STATUS] leak: {:.2} GB | cpu workers: {} | burned: {} cycles",
                leak_gb, workers, fmt_big(cycles)
            );
            thread::sleep(interval);
        }
    });

    thread::sleep(Duration::from_secs(2));
    println!("[MONITOR] Starting monitoring...\n");

    let mut iteration = 0;
    while running.load(Ordering::SeqCst) {
        iteration += 1;
        println!("=== Snapshot #{} at {} ===", 
                 iteration, 
                 chrono::Local::now().format("%H:%M:%S"));

        match monitor::collect_metrics() {
            Ok(metrics) => {
                let snapshot = Snapshot::new(iteration, metrics);
                snapshot.print_compact();

                {
                    let mut snaps = snapshots.lock().unwrap();
                    snaps.push(snapshot.clone());
                }

                if iteration % config.save_every_n_snapshots == 0 || iteration == 1 {
                    let snaps = snapshots.lock().unwrap();
                    if let Err(e) = save_snapshots_incremental(&snaps, &config.snapshot_file) {
                        eprintln!("[ERROR] Failed to save snapshots: {}", e);
                    } else {
                        println!("Saved {} snapshots to disk", snaps.len());
                    }
                }
            }
            Err(e) => eprintln!("[ERROR] Failed to collect metrics: {}", e),
        }

        for _ in 0..config.monitor_interval_sec {
            if !running.load(Ordering::SeqCst) { break; }
            thread::sleep(Duration::from_secs(1));
        }
    }

    println!("\n[*] Stopping workers...");
    running.store(false, Ordering::SeqCst);
    reporter_handle.join().ok();

    leak_handle.join().expect("Leak worker thread panicked");
    for handle in cpu_handles {
        handle.join().expect("CPU worker thread panicked");
    }

    println!("\n[*] Generating final report from {} snapshots...", 
             snapshots.lock().unwrap().len());
    let final_report = FinalReport::new(snapshots.lock().unwrap().clone(), config);
    final_report.save_to_file()?;
    final_report.print_summary();

    println!("\nShutdown complete.");
    Ok(())
}
