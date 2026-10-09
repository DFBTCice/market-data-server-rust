#!/usr/bin/env bash
# 周期性采样 md-server 的 /metrics 中进程级指标，输出 TSV（任务书 §三-8：CPU/内存观察）。
#
# 用法: scripts/compat/sample_process.sh <base-url> <总秒数> <采样间隔秒> <输出.tsv>
# 例:   scripts/compat/sample_process.sh http://192.168.1.198:28081 600 30 out.tsv
#
# 列: iso_utc  rss_bytes  vsz_bytes  cpu_seconds_total  open_fds  ws_active_clients
set -u
base="${1:?base-url}"; total="${2:?seconds}"; every="${3:?interval}"; out="${4:?out.tsv}"

printf 'ts_utc\trss_bytes\tvsz_bytes\tcpu_seconds_total\topen_fds\tws_active_clients\n' > "$out"
end=$(( $(date +%s) + total ))
while :; do
  m="$(curl -s -m 5 "$base/metrics" || true)"
  val() { printf '%s\n' "$m" | awk -v k="$1" '$1==k {print $2; exit}'; }
  printf '%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$(date -u +%FT%TZ)" \
    "$(val process_resident_memory_bytes)" "$(val process_virtual_memory_bytes)" \
    "$(val process_cpu_seconds_total)" "$(val process_open_fds)" "$(val md_ws_active_clients)" >> "$out"
  [ "$(date +%s)" -ge "$end" ] && break
  sleep "$every"
done
