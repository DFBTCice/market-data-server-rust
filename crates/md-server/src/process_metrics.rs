//! 进程级标准 Prometheus 指标
//!
//! 暴露与 Go `process_exporter` 等价的指标（生产环境 Linux 必备）：
//! - `process_resident_memory_bytes`：常驻内存（RSS）
//! - `process_virtual_memory_bytes`：虚拟内存
//! - `process_cpu_seconds_total`：CPU 累计时间（user + sys）
//! - `process_open_fds`：当前打开的文件描述符数
//! - `process_max_fds`：最大文件描述符上限
//! - `process_start_time_seconds`：启动时间（UNIX 秒）
//!
//! 实现策略：仅在 Linux 上读 `/proc/self/{stat,limits,fd}`，零外部依赖。
//! 非 Linux 平台仅暴露 `process_start_time_seconds`，其它字段保留为 0。

use std::sync::OnceLock;
use std::time::SystemTime;

static START_TIME_SECS: OnceLock<u64> = OnceLock::new();

/// 启动时调用一次，记录服务启动 UNIX 时间戳
pub fn init() {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let _ = START_TIME_SECS.set(now);
}

#[derive(Debug, Clone, Default)]
pub struct ProcessSnapshot {
    pub rss_bytes: u64,
    pub vms_bytes: u64,
    pub cpu_seconds_total: f64,
    pub open_fds: u64,
    pub max_fds: u64,
    pub start_time_seconds: u64,
}

#[cfg(target_os = "linux")]
pub fn collect() -> ProcessSnapshot {
    use std::fs;
    let mut snap = ProcessSnapshot {
        start_time_seconds: *START_TIME_SECS.get().unwrap_or(&0),
        ..Default::default()
    };

    // ---- /proc/self/stat 解析 ----
    // 字段顺序参考 proc(5)：
    //   pid (comm) state ppid pgrp session tty_nr tpgid flags
    //   minflt cminflt majflt cmajflt utime stime cutime cstime
    //   priority nice num_threads itrealvalue starttime
    //   vsize rss ...
    // 注意 (comm) 可能含空格 + 括号，需用最后一个 ')' 切分
    if let Ok(stat) = fs::read_to_string("/proc/self/stat") {
        if let Some(end) = stat.rfind(')') {
            let rest = &stat[end + 1..];
            let fields: Vec<&str> = rest.split_whitespace().collect();
            // rest 从 state 开始，所以索引偏移：state=0, utime=11, stime=12, vsize=20, rss=21
            let utime: u64 = fields.get(11).and_then(|s| s.parse().ok()).unwrap_or(0);
            let stime: u64 = fields.get(12).and_then(|s| s.parse().ok()).unwrap_or(0);
            // sysconf(_SC_CLK_TCK) 在大多数 Linux 上是 100，hardcode 简化（与 process_exporter 一致）
            const TICKS_PER_SEC: f64 = 100.0;
            snap.cpu_seconds_total = (utime + stime) as f64 / TICKS_PER_SEC;
            snap.vms_bytes = fields.get(20).and_then(|s| s.parse().ok()).unwrap_or(0);
            let rss_pages: u64 = fields.get(21).and_then(|s| s.parse().ok()).unwrap_or(0);
            // 页大小用 sysconf(_SC_PAGESIZE)，x86_64 上是 4KB
            snap.rss_bytes = rss_pages.saturating_mul(4096);
        }
    }

    // ---- /proc/self/fd 数文件描述符 ----
    if let Ok(entries) = fs::read_dir("/proc/self/fd") {
        snap.open_fds = entries.filter(|e| e.is_ok()).count() as u64;
    }

    // ---- /proc/self/limits 解析最大 fd ----
    if let Ok(limits) = fs::read_to_string("/proc/self/limits") {
        for line in limits.lines() {
            if line.starts_with("Max open files") {
                // "Max open files            65536                65536                files"
                let parts: Vec<&str> = line.split_whitespace().collect();
                // parts: ["Max", "open", "files", "65536", "65536", "files"]
                snap.max_fds = parts.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
                break;
            }
        }
    }

    snap
}

#[cfg(not(target_os = "linux"))]
pub fn collect() -> ProcessSnapshot {
    // 非 Linux 平台（macOS 开发）仅暴露启动时间；其他指标在 Linux 生产环境才采集
    ProcessSnapshot {
        start_time_seconds: *START_TIME_SECS.get().unwrap_or(&0),
        ..Default::default()
    }
}

/// 渲染为 Prometheus exposition 格式
pub fn render(snap: &ProcessSnapshot, out: &mut String) {
    let now_sec = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    let uptime = now_sec.saturating_sub(snap.start_time_seconds);

    out.push_str("# HELP process_resident_memory_bytes Resident memory size (RSS) in bytes\n");
    out.push_str("# TYPE process_resident_memory_bytes gauge\n");
    out.push_str(&format!("process_resident_memory_bytes {}\n", snap.rss_bytes));

    out.push_str("# HELP process_virtual_memory_bytes Virtual memory size in bytes\n");
    out.push_str("# TYPE process_virtual_memory_bytes gauge\n");
    out.push_str(&format!("process_virtual_memory_bytes {}\n", snap.vms_bytes));

    out.push_str("# HELP process_cpu_seconds_total Total user + system CPU time spent (seconds)\n");
    out.push_str("# TYPE process_cpu_seconds_total counter\n");
    out.push_str(&format!("process_cpu_seconds_total {}\n", snap.cpu_seconds_total));

    out.push_str("# HELP process_open_fds Number of open file descriptors\n");
    out.push_str("# TYPE process_open_fds gauge\n");
    out.push_str(&format!("process_open_fds {}\n", snap.open_fds));

    out.push_str("# HELP process_max_fds Maximum number of open file descriptors\n");
    out.push_str("# TYPE process_max_fds gauge\n");
    out.push_str(&format!("process_max_fds {}\n", snap.max_fds));

    out.push_str("# HELP process_start_time_seconds Start time of the process since unix epoch (seconds)\n");
    out.push_str("# TYPE process_start_time_seconds gauge\n");
    out.push_str(&format!("process_start_time_seconds {}\n", snap.start_time_seconds));

    out.push_str("# HELP process_uptime_seconds Uptime of the process (seconds)\n");
    out.push_str("# TYPE process_uptime_seconds gauge\n");
    out.push_str(&format!("process_uptime_seconds {}\n", uptime));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_produces_all_metrics() {
        init();
        let snap = collect();
        let mut out = String::new();
        render(&snap, &mut out);
        assert!(out.contains("process_resident_memory_bytes"));
        assert!(out.contains("process_virtual_memory_bytes"));
        assert!(out.contains("process_cpu_seconds_total"));
        assert!(out.contains("process_open_fds"));
        assert!(out.contains("process_start_time_seconds"));
        assert!(out.contains("process_uptime_seconds"));
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn collect_returns_nonzero_on_linux() {
        let snap = collect();
        // Linux 上至少 RSS 和 fd 应该 > 0
        assert!(snap.rss_bytes > 0, "RSS should be > 0 on Linux");
        assert!(snap.open_fds > 0, "open_fds should be > 0 on Linux");
    }
}
