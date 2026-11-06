#!/usr/bin/env bash
# 1.2.0 开工前核对（任务书第〇节）：本地 / GitHub / 198 三方一致性。只读：只做 git fetch 与 198 的 HTTP GET。
# 用法：bash evidence/1.2.0/tools/preflight.sh [输出目录]
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../../.." && pwd)"
OUT="${1:-$ROOT/evidence/1.2.0/preflight}"
MD="${MD_URL:-http://192.168.1.198:8081}"
mkdir -p "$OUT"
cd "$ROOT"

{
  echo "# captured_at_utc=$(date -u +%FT%TZ)"
  echo "## git fetch --all --tags"
  git fetch --all --tags 2>&1
  echo "## 当前分支与 HEAD"
  git branch --show-current
  git rev-parse HEAD
  echo "## origin/main"
  git rev-parse origin/main
  echo "## 标签 -> 提交"
  for t in v1.0.0 v1.1.0 v1.1.1; do echo "$t -> $(git rev-list -n1 "$t")"; done
  echo "## HEAD...origin/main 领先/落后（左=仅 HEAD，右=仅 origin/main）"
  git rev-list --left-right --count HEAD...origin/main
  git log --oneline HEAD..origin/main
  echo "## 已跟踪文件改动（应为空）"
  git status --porcelain --untracked-files=no
  echo "## 未跟踪文件（目录折叠）"
  git status --porcelain | grep '^??' || true
  echo "## 未跟踪文件数（展开）"
  git status --porcelain --untracked-files=all | grep -c '^??' || true
} > "$OUT/git-state.txt" 2>&1

{
  echo "## 源码树哈希（crates / Cargo.lock / Cargo.toml / Dockerfile / config.yaml）"
  for r in a06fe20 v1.1.1 HEAD origin/main; do
    echo "$r crates=$(git rev-parse "${r}:crates") Cargo.lock=$(git rev-parse "${r}:Cargo.lock") Cargo.toml=$(git rev-parse "${r}:Cargo.toml") Dockerfile=$(git rev-parse "${r}:Dockerfile") config=$(git rev-parse "${r}:config.yaml")"
  done
  echo "## git diff --stat v1.1.1 origin/main -- crates Cargo.toml Cargo.lock Dockerfile Dockerfile.cn config.yaml deploy .github（应为空）"
  git diff --stat v1.1.1 origin/main -- crates Cargo.toml Cargo.lock Dockerfile Dockerfile.cn config.yaml deploy .github
  echo "## v1.1.1..origin/main 中 docs/evidence/*.md 以外的改动"
  git diff --name-only v1.1.1 origin/main | grep -Ev '^(docs/|evidence/|.*\.md$)' || true
  echo "## a06fe20 / 56f14fd 是否为 origin/main 祖先"
  git merge-base --is-ancestor a06fe20 origin/main && echo "a06fe20 ancestor: yes"
  git merge-base --is-ancestor 56f14fd origin/main && echo "56f14fd ancestor: yes"
} > "$OUT/source-diff.txt" 2>&1

{
  echo "# captured_at_utc=$(date -u +%FT%TZ) target=$MD"
  echo "## GET /health"
  curl -sS -m 10 "$MD/health"; echo
  echo "## /metrics 关键项"
  curl -sS -m 10 "$MD/metrics" > "$OUT/198.metrics.txt"
  grep -E '^(process_start_time_seconds|process_resident_memory_bytes|md_ws_active_clients|md_ws_subscriptions_active|md_jemalloc_)' "$OUT/198.metrics.txt"
  echo "## process_start_time_seconds 换算"
  ts=$(awk '/^process_start_time_seconds/{print $2}' "$OUT/198.metrics.txt")
  python3 -c "import datetime,sys;print(datetime.datetime.fromtimestamp(int(float(sys.argv[1])),datetime.timezone.utc).isoformat())" "$ts"
  echo "## evidence/1.1.1 换版记录（从运行容器读回的二进制 sha）"
  python3 -c "import json;d=json.load(open('evidence/1.1.1/nas-198/post-swap-24h/swap-result.json'));print({k:d[k] for k in ('started_at_utc','new_image_id','binary_sha256')})"
  echo "## evidence/1.1.1 产物清单"
  head -2 evidence/1.1.1/build-logs/dist-sha256.txt
  echo "## GET /api/v1/subscriptions"
  curl -sS -m 10 "$MD/api/v1/subscriptions" | tee "$OUT/198.subscriptions.json"; echo
  python3 - "$OUT/198.subscriptions.json" <<'PY'
import collections, json, sys
names = {1: "TICK", 2: "KLINE", 3: "QUOTE"}
subs = json.load(open(sys.argv[1]))
c = collections.Counter((names.get(s["data_type"], s["data_type"]), s["exchange"]) for s in subs)
print("total", len(subs), dict(sorted(c.items())))
PY
} > "$OUT/198-readonly.txt" 2>&1

cat "$OUT/git-state.txt" "$OUT/source-diff.txt" "$OUT/198-readonly.txt"
