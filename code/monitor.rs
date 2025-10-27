use serde::{Deserialize, Serialize};
use std::fs;
use crate::error::MonitorError;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SystemMetrics {
    pub timestamp: chrono::DateTime<chrono::Local>,
    pub memory: MemoryMetrics,
    pub cpu: CpuMetrics,
    pub swap: SwapMetrics,
    pub load: LoadMetrics,
    pub critical_events: Vec<CriticalEvent>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MemoryMetrics {
    pub total_kb: u64,
    pub available_kb: u64,
    pub used_kb: u64,
    pub used_percent: f64,
    pub free_kb: u64,
    pub buffers_kb: u64,
    pub cached_kb: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CpuMetrics {
    pub user_time: u64,
    pub system_time: u64,
    pub idle_time: u64,
    pub iowait_time: u64,
    pub total_time: u64,
    pub cpu_usage_percent: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LoadMetrics {
    pub load_average_1min: f64,
    pub load_average_5min: f64,
    pub load_average_15min: f64,
    pub cpu_count: usize,
    pub load_percent_1min: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct SwapMetrics {
    pub total_kb: u64,
    pub used_kb: u64,
    pub free_kb: u64,
    pub used_percent: f64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CriticalEvent {
    pub event_type: String,
    pub severity: String,
    pub description: String,
    pub timestamp: chrono::DateTime<chrono::Local>,
}

pub fn collect_metrics() -> Result<SystemMetrics, MonitorError> {
    let memory = collect_memory_metrics()?;
    let cpu = collect_cpu_metrics()?;
    let load = collect_load_metrics()?;
    let swap = collect_swap_metrics()?;
    let critical_events = detect_critical_events(&memory, &load, &swap);

    Ok(SystemMetrics {
        timestamp: chrono::Local::now(),
        memory,
        cpu,
        swap,
        load,
        critical_events,
    })
}

fn collect_memory_metrics() -> Result<MemoryMetrics, MonitorError> {
    let content = fs::read_to_string("/proc/meminfo")
        .map_err(|e| MonitorError::FileRead(format!("/proc/meminfo: {}", e)))?;

    let mut total = 0u64;
    let mut available = 0u64;
    let mut free = 0u64;
    let mut buffers = 0u64;
    let mut cached = 0u64;

    for line in content.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 { continue; }
        let value = parts[1].parse::<u64>().unwrap_or(0);

        match parts[0] {
            "MemTotal:" => total = value,
            "MemAvailable:" => available = value,
            "MemFree:" => free = value,
            "Buffers:" => buffers = value,
            "Cached:" => cached = value,
            _ => {}
        }
    }

    let used = total.saturating_sub(available);
    let used_percent = if total > 0 {
        (used as f64 / total as f64) * 100.0
    } else { 0.0 };

    Ok(MemoryMetrics {
        total_kb: total,
        available_kb: available,
        used_kb: used,
        used_percent,
        free_kb: free,
        buffers_kb: buffers,
        cached_kb: cached,
    })
}

static mut PREV_CPU_STATS: Option<(u64, u64, u64, u64)> = None;

fn collect_cpu_metrics() -> Result<CpuMetrics, MonitorError> {
    let content = fs::read_to_string("/proc/stat")
        .map_err(|e| MonitorError::FileRead(format!("/proc/stat: {}", e)))?;

    let line = content.lines().next()
        .ok_or_else(|| MonitorError::ParseError("Empty /proc/stat".into()))?;

    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.len() < 5 {
        return Err(MonitorError::ParseError("Invalid /proc/stat format".into()));
    }

    let user: u64 = parts[1].parse().unwrap_or(0);
    let system: u64 = parts[3].parse().unwrap_or(0);
    let idle: u64 = parts[4].parse().unwrap_or(0);
    let iowait: u64 = parts.get(5).and_then(|s| s.parse().ok()).unwrap_or(0);

    let total: u64 = parts[1..].iter()
        .filter_map(|s| s.parse::<u64>().ok())
        .sum::<u64>();

    let cpu_usage_percent = unsafe {
        if let Some((prev_user, prev_system, prev_idle, prev_total)) = PREV_CPU_STATS {
            let user_delta = user.saturating_sub(prev_user);
            let system_delta = system.saturating_sub(prev_system);
            let idle_delta = idle.saturating_sub(prev_idle);
            let total_delta = total.saturating_sub(prev_total);

            if total_delta > 0 {
                let active = user_delta + system_delta;
                (active as f64 / total_delta as f64) * 100.0
            } else { 0.0 }
        } else { 0.0 }
    };

    unsafe { PREV_CPU_STATS = Some((user, system, idle, total)); }

    Ok(CpuMetrics {
        user_time: user,
        system_time: system,
        idle_time: idle,
        iowait_time: iowait,
        total_time: total,
        cpu_usage_percent,
    })
}

fn collect_load_metrics() -> Result<LoadMetrics, MonitorError> {
    let content = fs::read_to_string("/proc/loadavg")
        .map_err(|e| MonitorError::FileRead(format!("/proc/loadavg: {}", e)))?;

    let parts: Vec<&str> = content.split_whitespace().collect();
    if parts.len() < 3 {
        return Err(MonitorError::ParseError("Invalid /proc/loadavg format".into()));
    }

    let load_1 = parts[0].parse::<f64>()
        .map_err(|e| MonitorError::ParseError(format!("Load average: {}", e)))?;
    let load_5 = parts[1].parse::<f64>()
        .map_err(|e| MonitorError::ParseError(format!("Load average: {}", e)))?;
    let load_15 = parts[2].parse::<f64>()
        .map_err(|e| MonitorError::ParseError(format!("Load average: {}", e)))?;

    let cpu_count = num_cpus();
    let load_percent_1min = if cpu_count > 0 {
        (load_1 / cpu_count as f64) * 100.0
    } else { 0.0 };

    Ok(LoadMetrics {
        load_average_1min: load_1,
        load_average_5min: load_5,
        load_average_15min: load_15,
        cpu_count,
        load_percent_1min,
    })
}

fn num_cpus() -> usize {
    fs::read_to_string("/proc/cpuinfo")
        .ok()
        .map(|content| content.lines().filter(|l| l.starts_with("processor")).count())
        .unwrap_or(1)
}

fn collect_swap_metrics() -> Result<SwapMetrics, MonitorError> {
    let content = fs::read_to_string("/proc/meminfo")
        .map_err(|e| MonitorError::FileRead(format!("/proc/meminfo: {}", e)))?;

    let mut total = 0u64;
    let mut free = 0u64;

    for line in content.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 2 { continue; }
        let value = parts[1].parse::<u64>().unwrap_or(0);

        match parts[0] {
            "SwapTotal:" => total = value,
            "SwapFree:" => free = value,
            _ => {}
        }
    }

    let used = total.saturating_sub(free);
    let used_percent = if total > 0 {
        (used as f64 / total as f64) * 100.0
    } else { 0.0 };

    Ok(SwapMetrics {
        total_kb: total,
        used_kb: used,
        free_kb: free,
        used_percent,
    })
}

fn detect_critical_events(
    memory: &MemoryMetrics,
    load: &LoadMetrics,
    swap: &SwapMetrics,
) -> Vec<CriticalEvent> {
    let mut events = Vec::new();
    let now = chrono::Local::now();

    if memory.used_percent > 85.0 {
        events.push(CriticalEvent {
            event_type: "MEMORY_CRITICAL".into(),
            severity: "CRITICAL".into(),
            description: format!("Memory usage at {:.1}%", memory.used_percent),
            timestamp: now,
        });
    } else if memory.used_percent > 70.0 {
        events.push(CriticalEvent {
            event_type: "MEMORY_WARNING".into(),
            severity: "HIGH".into(),
            description: format!("Memory usage at {:.1}%", memory.used_percent),
            timestamp: now,
        });
    }

    if load.load_percent_1min > 250.0 {
        events.push(CriticalEvent {
            event_type: "LOAD_CRITICAL".into(),
            severity: "CRITICAL".into(),
            description: format!("Load at {:.1}% (avg: {:.2})", 
                                 load.load_percent_1min, load.load_average_1min),
            timestamp: now,
        });
    } else if load.load_percent_1min > 150.0 {
        events.push(CriticalEvent {
            event_type: "LOAD_WARNING".into(),
            severity: "HIGH".into(),
            description: format!("Load at {:.1}% (avg: {:.2})", 
                                 load.load_percent_1min, load.load_average_1min),
            timestamp: now,
        });
    }

    if swap.total_kb > 0 && swap.used_percent > 60.0 {
        events.push(CriticalEvent {
            event_type: "SWAP_CRITICAL".into(),
            severity: "CRITICAL".into(),
            description: format!("Swap usage at {:.1}%", swap.used_percent),
            timestamp: now,
        });
    } else if swap.total_kb > 0 && swap.used_percent > 30.0 {
        events.push(CriticalEvent {
            event_type: "SWAP_WARNING".into(),
            severity: "HIGH".into(),
            description: format!("Swap usage at {:.1}%", swap.used_percent),
            timestamp: now,
        });
    }

    events
}
