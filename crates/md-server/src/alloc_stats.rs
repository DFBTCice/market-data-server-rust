//! 分配器诊断（1.1.1，任务书 §三-D1）
//!
//! 默认构建（生产镜像使用的构建）：本模块不改变分配器，`render` 不输出任何内容。
//!
//! `--features jemalloc-stats` 构建（仅用于旁路诊断容器，**不用于生产镜像**）：
//! 使用 jemalloc 作为全局分配器，并在 `/metrics` 暴露
//! `md_jemalloc_{allocated,active,resident,retained,mapped,metadata}_bytes`，用于区分
//!
//! - 逻辑泄漏：`allocated`（应用存活字节）持续上涨；
//! - 分配器碎片 / 保留：`allocated` 持平而 `resident` / `active` 上涨。

#[cfg(feature = "jemalloc-stats")]
#[global_allocator]
static GLOBAL: tikv_jemallocator::Jemalloc = tikv_jemallocator::Jemalloc;

/// 追加 jemalloc 统计到 `/metrics` 文本（未启用 `jemalloc-stats` 时为空操作）
#[cfg(feature = "jemalloc-stats")]
pub fn render(out: &mut String) {
    use tikv_jemalloc_ctl::{epoch, stats};
    // jemalloc 的统计是快照式的，读之前必须推进 epoch
    if epoch::advance().is_err() {
        return;
    }
    let rows: [(&str, &str, Option<usize>); 6] = [
        (
            "allocated",
            "Bytes allocated by the application (live heap as seen by jemalloc)",
            stats::allocated::read().ok(),
        ),
        (
            "active",
            "Bytes in active pages allocated by the application",
            stats::active::read().ok(),
        ),
        (
            "resident",
            "Bytes in physically resident data pages mapped by the allocator",
            stats::resident::read().ok(),
        ),
        (
            "retained",
            "Bytes in virtual memory mappings retained (not returned to the OS)",
            stats::retained::read().ok(),
        ),
        (
            "mapped",
            "Bytes in active extents mapped by the allocator",
            stats::mapped::read().ok(),
        ),
        (
            "metadata",
            "Bytes dedicated to allocator metadata",
            stats::metadata::read().ok(),
        ),
    ];
    for (name, help, val) in rows {
        if let Some(v) = val {
            out.push_str(&format!("# HELP md_jemalloc_{name}_bytes {help}\n"));
            out.push_str(&format!("# TYPE md_jemalloc_{name}_bytes gauge\n"));
            out.push_str(&format!("md_jemalloc_{name}_bytes {v}\n"));
        }
    }
}

#[cfg(not(feature = "jemalloc-stats"))]
pub fn render(_out: &mut String) {}
