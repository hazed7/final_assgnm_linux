use std::sync::{Arc, atomic::{AtomicBool, AtomicU64, Ordering}};
use std::thread;
use std::time::Duration;

static LEAK_TOTAL_BYTES: AtomicU64 = AtomicU64::new(0);
static CPU_TOTAL_CYCLES: AtomicU64 = AtomicU64::new(0);
static CPU_ACTIVE_WORKERS: AtomicU64 = AtomicU64::new(0);

pub fn leak_total_bytes() -> u64 { LEAK_TOTAL_BYTES.load(Ordering::Relaxed) }
pub fn cpu_total_cycles() -> u64 { CPU_TOTAL_CYCLES.load(Ordering::Relaxed) }
pub fn cpu_active_workers() -> u64 { CPU_ACTIVE_WORKERS.load(Ordering::Relaxed) }

#[inline]
fn page_size() -> usize {
    use libc::{sysconf, _SC_PAGESIZE};
    unsafe {
        let ps = sysconf(_SC_PAGESIZE);
        if ps > 0 { ps as usize } else { 4096 }
    }
}

pub fn spawn_leak_worker(running: Arc<AtomicBool>, config: crate::config::Config) {
    let mut buf: Vec<Vec<u8>> = Vec::new();
    let step_bytes = (config.leak_size_mb as usize).saturating_mul(1024 * 1024);
    let sleep = Duration::from_secs(config.leak_interval_sec as u64);
    let ps = page_size();

    while running.load(Ordering::SeqCst) {
        let mut chunk = vec![0u8; step_bytes];

        let len = chunk.len();
        let mut i = 0usize;
        while i < len {
            chunk[i] = 1;
            i = i.saturating_add(ps);
        }
        if len > 0 { chunk[len - 1] = chunk[len - 1].wrapping_add(1); }

        LEAK_TOTAL_BYTES.fetch_add(step_bytes as u64, Ordering::Relaxed);
        buf.push(chunk);

        thread::sleep(sleep);
    }

    std::hint::black_box(&buf);
}

pub fn spawn_cpu_stress(running: Arc<AtomicBool>, _idx: usize) {
    CPU_ACTIVE_WORKERS.fetch_add(1, Ordering::Relaxed);

    const CHUNK: u64 = 400_000_000;
    while running.load(Ordering::SeqCst) {
        let mut x: u64 = 0;
        for _ in 0..CHUNK {
            x = x.wrapping_add(1);
        }
        std::hint::black_box(x);
        CPU_TOTAL_CYCLES.fetch_add(CHUNK, Ordering::Relaxed);
    }

    CPU_ACTIVE_WORKERS.fetch_sub(1, Ordering::Relaxed);
}
