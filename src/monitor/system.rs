use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::Serialize;
use sysinfo::{Disks, ProcessesToUpdate, System};
use tokio::sync::RwLock;

#[derive(Debug, Clone, Serialize, Default)]
pub struct SystemSnapshot {
    /// 主机名
    pub hostname: String,
    /// 整体 CPU 占用百分比
    pub cpu_usage: f32,
    /// 逻辑 CPU 核数
    pub cpu_cores: usize,
    /// 各核占用
    pub cpu_per_core: Vec<f32>,
    /// 内存总量（字节）
    pub memory_total_bytes: u64,
    /// 内存已用（字节）
    pub memory_used_bytes: u64,
    /// 内存占用百分比
    pub memory_usage: f32,
    /// Swap 总量
    pub swap_total_bytes: u64,
    /// Swap 已用
    pub swap_used_bytes: u64,
    /// 磁盘总量（根分区优先）
    pub disk_total_bytes: u64,
    /// 磁盘已用
    pub disk_used_bytes: u64,
    /// 磁盘占用百分比
    pub disk_usage: f32,
    /// 负载 1/5/15 分钟
    pub load_avg_1: f64,
    pub load_avg_5: f64,
    pub load_avg_15: f64,
    /// 系统启动以来秒数
    pub uptime_secs: u64,
    /// 本进程 CPU 占用
    pub process_cpu: f32,
    /// 本进程内存占用（RSS）
    pub process_memory_bytes: u64,
    pub updated_at: u64,
}

pub type SharedSystemMonitor = Arc<SystemMonitor>;

pub struct SystemMonitor {
    snapshot: RwLock<SystemSnapshot>,
}

impl SystemMonitor {
    pub fn spawn() -> SharedSystemMonitor {
        let this = Arc::new(Self {
            snapshot: RwLock::new(SystemSnapshot::default()),
        });
        let bg = Arc::clone(&this);
        tokio::spawn(async move {
            bg.run().await;
        });
        this
    }

    pub async fn snapshot(&self) -> SystemSnapshot {
        self.snapshot.read().await.clone()
    }

    async fn run(self: Arc<Self>) {
        loop {
            let result = tokio::task::spawn_blocking(collect_snapshot).await;
            if let Ok(snap) = result {
                *self.snapshot.write().await = snap;
            }
            tokio::time::sleep(Duration::from_secs(2)).await;
        }
    }
}

fn collect_snapshot() -> SystemSnapshot {
    let mut sys = System::new();
    sys.refresh_memory();
    sys.refresh_cpu_all();
    // 两次采样才能得到有意义的 CPU 占用
    std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL);
    sys.refresh_cpu_usage();
    sys.refresh_processes(ProcessesToUpdate::All, true);

    let cpu_cores = sys.cpus().len();
    let cpu_per_core: Vec<f32> = sys.cpus().iter().map(|c| c.cpu_usage()).collect();
    let cpu_usage = if cpu_cores == 0 {
        0.0
    } else {
        cpu_per_core.iter().sum::<f32>() / cpu_cores as f32
    };

    let memory_total = sys.total_memory();
    let memory_used = sys.used_memory();
    let memory_usage = if memory_total == 0 {
        0.0
    } else {
        (memory_used as f64 / memory_total as f64 * 100.0) as f32
    };

    let disks = Disks::new_with_refreshed_list();
    let (disk_total, disk_used) = pick_root_disk(&disks);
    let disk_usage = if disk_total == 0 {
        0.0
    } else {
        (disk_used as f64 / disk_total as f64 * 100.0) as f32
    };

    let load = System::load_average();
    let (process_cpu, process_memory) = sysinfo::get_current_pid()
        .ok()
        .and_then(|pid| sys.process(pid).map(|p| (p.cpu_usage(), p.memory())))
        .unwrap_or((0.0, 0));

    SystemSnapshot {
        hostname: System::host_name().unwrap_or_else(|| "unknown".into()),
        cpu_usage,
        cpu_cores,
        cpu_per_core,
        memory_total_bytes: memory_total,
        memory_used_bytes: memory_used,
        memory_usage,
        swap_total_bytes: sys.total_swap(),
        swap_used_bytes: sys.used_swap(),
        disk_total_bytes: disk_total,
        disk_used_bytes: disk_used,
        disk_usage,
        load_avg_1: load.one,
        load_avg_5: load.five,
        load_avg_15: load.fifteen,
        uptime_secs: System::uptime(),
        process_cpu,
        process_memory_bytes: process_memory,
        updated_at: now_millis(),
    }
}

fn pick_root_disk(disks: &Disks) -> (u64, u64) {
    let mut best = (0u64, 0u64);
    for disk in disks.list() {
        let mount = disk.mount_point().to_string_lossy();
        let total = disk.total_space();
        let used = total.saturating_sub(disk.available_space());
        if mount == "/" {
            return (total, used);
        }
        if total > best.0 {
            best = (total, used);
        }
    }
    best
}

fn now_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}
