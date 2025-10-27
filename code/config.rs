use clap::Parser;
use serde::{Deserialize, Serialize};

#[derive(Parser, Debug, Clone, Serialize, Deserialize)]
#[command(author, version, about = None, long_about = None)]
pub struct Config {
    #[arg(short = 'm', long, default_value_t = 50)]
    pub leak_size_mb: usize,
    
    #[arg(short = 'l', long, default_value_t = 10)]
    pub leak_interval_sec: u64,
    
    #[arg(short = 'i', long, default_value_t = 60)]
    pub monitor_interval_sec: u64,
    
    #[arg(short = 'c', long, default_value_t = 2)]
    pub cpu_workers: usize,
    
    #[arg(short = 'd', long, default_value_t = false)]
    pub disk_stress: bool,
    
    #[arg(short = 's', long, default_value = "snapshots_incremental.json")]
    pub snapshot_file: String,
    
    #[arg(short = 'n', long, default_value_t = 1)]
    pub save_every_n_snapshots: usize,
}
