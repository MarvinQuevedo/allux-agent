use std::sync::Arc;
use std::time::Duration;

use sysinfo::System;
use tokio::sync::RwLock;
use tokio::time::sleep;

pub struct SystemMetrics {
    pub cpu_usage: f32,
    pub ram_used: u64,
    pub ram_total: u64,
}

impl SystemMetrics {
    pub fn new() -> Self {
        Self {
            cpu_usage: 0.0,
            ram_used: 0,
            ram_total: 0,
        }
    }

    /// Format RAM as "X.XGB / Y.YGB".
    pub fn ram_display(&self) -> String {
        let used_gb = self.ram_used as f64 / 1_073_741_824.0;
        let total_gb = self.ram_total as f64 / 1_073_741_824.0;
        format!("{used_gb:.1}/{total_gb:.0}GB")
    }
}

pub type SharedMetrics = Arc<RwLock<SystemMetrics>>;

pub fn new_shared() -> SharedMetrics {
    Arc::new(RwLock::new(SystemMetrics::new()))
}

/// Spawns the background metrics collector. Returns the join handle.
pub fn spawn_collector(metrics: SharedMetrics) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut sys = System::new_all();
        // First refresh doesn't give accurate CPU — wait a tick.
        sleep(Duration::from_millis(500)).await;

        loop {
            sys.refresh_cpu();
            sys.refresh_memory();

            let cpu_usage = sys.global_cpu_info().cpu_usage();
            let ram_used = sys.used_memory();
            let ram_total = sys.total_memory();

            {
                let mut w = metrics.write().await;
                w.cpu_usage = cpu_usage;
                w.ram_used = ram_used;
                w.ram_total = ram_total;
            }

            sleep(Duration::from_secs(2)).await;
        }
    })
}
