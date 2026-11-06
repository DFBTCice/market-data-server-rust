# 198 旁路金丝雀报告：行情服务 1.2.0（产物 `cbc6b053…`）

- 时间：2026-11-06 16:05–16:46Z（北京时间 11-07 00:05–00:46）。依据：主审复核意见 §四；主审 11-07 00:04 批准跑金丝雀，**未换版**。
- 产物：`dist/md-server-1.2.0-12192d9-linux-x86_64`，SHA-256 `cbc6b0531b833f38fa5e4666be286d24d01ad169230ba855c8a5f126aed887bf`（源码 `42bc897`）。
- 工具：`evidence/1.2.0/tools/canary_198.py`（第 1 部分 + 第 2 部分）、`canary_consumers.mjs`（三类消费者）、`recheck_config_kept.mjs`、`summarize_canary_stats.py`。
- 证据：`nas-198/canary/`（第一次完整运行）、`nas-198/canary-rerun/`（第 2 部分修正判据后重跑）、`nas-198/cleanup-check.txt`。

## 首段

**第 1 部分（字节对比）：通过。** 旁路 1.1.1 与开关关闭的 1.2.0 同窗口 240 秒，`gate-results.json`：push-gateway 0、push-legacy 0、noquote-legacy 0、subs 0、metrics 0、selftest 0；`quote_raw` 1，经核对只有已裁定不计的 `quote.binance.BNBUSDT` 数量差异（1396 条中 10 条），`quote.okx.BNB-USDT-SWAP` 1532 条逐字节相同。

**第 2 部分（开关打开、报价补订关闭）：通过。** 修正判据后的重跑里，试跑（闲置 2 分钟）和现网参数复跑（闲置 10、保护 10、检查 60 秒）的 `summary.json` 都是 `pass: true`，12 项判据全部满足。
第一次运行这两部分的 `pass` 是 `false`，原因在我的判据脚本，不在产物：脚本把现网补入的运行期 K 线（botd 登记的 `binance SNDKUSDT 15m`）误当成配置订阅，它被正确地闲置退订后，`config_subscriptions_kept` 误报为 `false`；其余 11 项都通过。按服务端 GET 返回的 `source` 字段复核，第一次两部分的配置订阅在全部 83 / 178 个采样里一条不少（复核脚本带正对照）。判据修正后重跑第 2 部分，结果见上。

**从运行中容器读回的二进制 sha**：5 个 1.2.0 旁路容器（off、short×2、prod×2）全部 = `cbc6b053…87bf`；读回的配置：off 与现网 `config.final.yaml` 逐字节相同，prod 与换版配置 `config.release.yaml` 逐字节相同。

**报价话题确认未被补订**：4 次开关打开运行，订阅 `quote.binance.BTCUSDT`、`quote.binance.ETHUSDT`、`quote.okx.BTC-USDT-SWAP`、`quote.okx.ETH-USDT-SWAP` 后，订阅列表从未出现新的 QUOTE，这 4 个话题都收到 0 条；容器日志 `on_demand_quote=false`，按需补订只有 2 条 TICK、0 条 QUOTE；配置名单里的 BNB 报价照常推送。

**现网**：前后 5 次快照 `process_start_time_seconds` 都是 `1792785202`（未重启），RSS 96.5–97.3 MiB，`md_ws_active_clients` 2；结束后 `RestartCount 0`、健康。金丝雀对现网只发 GET。

**清理**：旁路容器（`purpose=md-runtime-subs-1.2.0`）与 `md-server:exp-120-*` 镜像已全部删除（`cleanup-check.txt`）；JWT `/tmp/pt_jwt` 与登录脚本已删除。

**需要业务方决定**：主审复核本报告后，确定换版窗口（低峰）。换版按 `SWAP_PLAN_1.2.0.md`，届时需要再开一次 Portainer 会话。

---

## 一、做了什么

| 阶段 | 容器（各 256 MiB、2 核、restart=no、错开 5 秒启动） | 内容 |
|---|---|---|
| 第 1 部分（16:06–16:14Z） | `old` = md-server:1.1.1 原样；`off` = 1.2.0 + 现网配置；`short` = 1.2.0 + 试跑配置 | old/off 灌同一份现网订阅（14 条），同窗口 240 秒抓 Gateway 14 个话题（12 个 tick/kline + 2 个 quote）、旧 `/ws` 12 个话题并 compare；short 跑三类消费者 420 秒 |
| 第 2 部分 b（16:14–16:29Z） | `prod` = 1.2.0 + `config.release.yaml` | 灌现网订阅，三类消费者现网参数 900 秒 |
| 第 2 部分重跑（16:30–16:45Z） | `short`、`prod` 同时运行 | 判据修正后，两种参数各跑一次（灌入当时的现网订阅 17 条） |

三类消费者（`canary_consumers.mjs`）：
- **OMS 类**：全程订阅 `tick.binance.BNBUSDT`、`tick.okx.BNB-USDT-SWAP`；t=30s 订阅从未登记的 `tick.okx.SOL-USDT-SWAP`；发现 1stbotv3 类的 SOLUSDT TICK 被退后，在同一连接重订 `tick.binance.SOLUSDT`。
- **1stbotv3 类**：POST 登记 SOLUSDT 的 TICK 与 KLINE(1m) 并订阅；按 botd 顺序先退 WS 话题，约 50ms 后 DELETE 自己登记的 KLINE（TICK 不 DELETE）。
- **报价类**：订阅上述 4 个名单外报价话题与名单内 `quote.binance.BNBUSDT`、`quote.okx.BNB-USDT-SWAP`。

判据修正：`config_subscriptions_kept` 改为以初始采样里服务端 GET 的 `source=config` 为准（`summary.json` 里列出 `config_keys` 与 `initial_runtime_keys_gone`）。新增 `recheck_config_kept.mjs` 用原始 `timeline.jsonl` 复核，并内置正对照（删掉一个配置键必须报缺失）。

## 二、怎么验证的（原文）

### 1. 第 1 部分（`canary/`）

```
$ cat evidence/1.2.0/nas-198/canary/gate-results.json
{ "push-gateway": 0, "push-legacy": 0, "noquote-legacy": 0, "subs": 0, "metrics": 0, "selftest": 0, "quote_raw": 1 }
```

- `compare-push-gateway.log` / `compare-push-legacy.log`：`RESULT: PASS`（tick / kline 共 12 个话题，含现网的 `tick.binance.SNDKUSDT`、`kline.15m.binance.SNDKUSDT` 等）。
- `compare-metrics.log`：`old 样本 698, new 样本 704; 删除=0, 新增=6`；`TYPE 变更/删除=0, HELP 变更/删除=0`；新增的是 6 个 `md_runtime_subscriptions*` 族。
- `compare-subs.log`：`old=14 条, new=14 条, 缺失=0, 非预期多出=0`。
- `compare-noquote-legacy.log`：`off-legacy: 数据消息 61037 条, 其中 quote 0 条`。
- `compare-quote.log`：`quote.okx.BNB-USDT-SWAP` `对应记录 1532 条…完全相同`；`quote.binance.BNBUSDT` `对应记录 1396 条中 10 条不同`（挂单数量，docs/33 §4 已裁定不计；与 1.1.1 对 1.1.0 时同类）。
- old / off 容器日志：WARN / ERROR 都是 0 行。
- 原始抓取：`captures.tar.gz`（116 个文件，逐文件 sha256 见 `raw-sha256.txt`；解包后可用 `compare.mjs` 重跑）。

### 2. 第 2 部分（判据修正后的重跑，`canary-rerun/`）

```
2026-11-06T16:37:31.740423+00:00 switch-on short exit=0; see …/canary-rerun/short/summary.json
2026-11-06T16:45:35.160503+00:00 switch-on prod exit=0; see …/canary-rerun/prod/summary.json
```

prod（现网参数）容器日志原文：

```
16:30:22.781Z INFO md_runtime_subs: runtime subscriptions: idle unsubscribe and on-demand subscribe ENABLED (idle_minutes=10, check_interval_seconds=60, startup_grace_minutes=10, max_runtime_subscriptions=200, on_demand_quote=false)
16:30:59.860Z INFO md_runtime_subs: runtime subscription added on demand: exchange=okx type=TICK symbol=SOL-USDT-SWAP interval=- receivers=1 connector=okx
16:41:22.984Z INFO md_runtime_subs: runtime subscription idle-unsubscribed: exchange=binance type=KLINE symbol=BTCUSDT interval=30m idle=10m53s connector=binance
16:41:22.984Z INFO md_runtime_subs: runtime subscription idle-unsubscribed: exchange=binance type=KLINE symbol=SNDKUSDT interval=5m idle=10m53s connector=binance
16:41:22.984Z INFO md_runtime_subs: runtime subscription idle-unsubscribed: exchange=binance type=TICK symbol=SNDKUSDT interval=- idle=10m53s connector=binance
16:41:22.984Z INFO md_runtime_subs: runtime subscription idle-unsubscribed: exchange=binance type=KLINE symbol=BNBUSDT interval=5m idle=10m53s connector=binance
16:41:22.984Z INFO md_runtime_subs: runtime subscription idle-unsubscribed: exchange=binance type=KLINE symbol=SNDKUSDT interval=30m idle=10m53s connector=binance
16:42:22.984Z INFO md_runtime_subs: runtime subscription idle-unsubscribed: exchange=binance type=TICK symbol=SOLUSDT interval=- idle=10m52s connector=binance
16:42:23.500Z INFO md_runtime_subs: runtime subscription added on demand: exchange=binance type=TICK symbol=SOLUSDT interval=- receivers=1 connector=binance
```

被退的 5 条运行期订阅是从现网补入、旁路里没人接收的（botd 当时在现网浏览 K 线登记的），保护期结束、闲置满 10 分钟后才退，配置订阅未动。

| 判据（`summary.json`） | short（闲置 2 分钟） | prod（现网参数） |
|---|---|---|
| OMS 类话题不被退、重复 0 | 通过（2101 / 761 条，重复 0） | 通过（3539 / 1328 条，重复 0） |
| 配置订阅全程不少（10 条，按 `source=config`） | 通过 | 通过 |
| 1stbotv3 类：先退 WS 再 DELETE 的 KLINE 立即消失、不补回 | DELETE t=91.7 → t=91.7 已消失 | DELETE t=61.2 → t=61.3 已消失 |
| 1stbotv3 类：TICK 闲置到期被退、重订前不补回 | 取消 t=91.5 → t=219.2 被退 | 取消 t=61.2 → t=714.3 被退（日志 `idle=10m52s`） |
| 补入但没人接收的运行期订阅被退 | 5 条（SNDKUSDT TICK + 4 条 K 线） | 同左，保护期后 |
| 重订已退话题 → 自动补订、推送不翻倍 | 5 秒内补回，648 条，重复 0 | 5 秒内补回（日志 0.5 秒），366 条，重复 0 |
| 从未登记的 tick 话题按需补订 | OKX SOL 5 秒内，1997 条 | OKX SOL 5 秒内，3014 条 |
| 名单外报价不补订 QUOTE、无数据 | 4 个话题 0 条，订阅列表无新 QUOTE | 同左 |
| 名单内 BNB 报价照常推送 | 3419 / 2532 条 | 7266 / 5553 条 |
| `md_quotes_processed` 速率（订阅报价话题前→后） | 106.0 → 145.4 条/秒（比值 1.37） | 106.5 → 110.6 条/秒（比值 1.04） |

结束时计数（两者相同）：`md_runtime_subscriptions 4`、`idle_unsubscribed_total 6`、`demand_subscribed_total 2`、`demand_failed_total 0`、`limit_rejected_total 0`、`md_ticks_dropped 0`、`md_quotes_dropped 0`。
报价速率随行情波动（第一次运行两部分为 1.09、1.14）；判据是比值 < 2，且名单外话题 0 条、订阅列表无新 QUOTE。若误补了币安 BTC / ETH bookTicker，入站会成倍增加。

### 3. 第一次运行的第 2 部分（`canary/short`、`canary/prod`）与复核

- `canary/short/summary.json`、`canary/prod/summary.json`：`pass: false`，唯一不满足的是 `config_subscriptions_kept`，其余 11 项为 `true`（包括报价三项）。
- 复核（原始 timeline，按 `source=config`）：

```
$ node evidence/1.2.0/tools/recheck_config_kept.mjs evidence/1.2.0/nas-198/canary/short/timeline.jsonl
"samples": 83, "missing": [], "initial_runtime_keys_gone": ["binance|1|SNDKUSDT|", "binance|2|SNDKUSDT|15m"],
"config_subscriptions_kept": true, "positive_control": {"removed": "t=213.5 okx|1|BTC-USDT-SWAP|", "caught": true}
$ node evidence/1.2.0/tools/recheck_config_kept.mjs evidence/1.2.0/nas-198/canary/prod/timeline.jsonl
"samples": 178, "missing": [], "initial_runtime_keys_gone": ["binance|1|SNDKUSDT|", "binance|2|SNDKUSDT|15m"],
"config_subscriptions_kept": true, "positive_control": {"removed": "t=454.5 binance|2|ETHUSDT|5m", "caught": true}
```

### 4. 二进制与配置读回

```
canary/off.image-binary-sha256.txt            cbc6b0531b833f38fa5e4666be286d24d01ad169230ba855c8a5f126aed887bf
canary/short.image-binary-sha256.txt          cbc6b0531b833f38fa5e4666be286d24d01ad169230ba855c8a5f126aed887bf
canary/prod.image-binary-sha256.txt           cbc6b0531b833f38fa5e4666be286d24d01ad169230ba855c8a5f126aed887bf
canary-rerun/short.image-binary-sha256.txt    cbc6b0531b833f38fa5e4666be286d24d01ad169230ba855c8a5f126aed887bf
canary-rerun/prod.image-binary-sha256.txt     cbc6b0531b833f38fa5e4666be286d24d01ad169230ba855c8a5f126aed887bf
canary/prod.config.yaml == config.release.yaml；canary-rerun/prod.config.yaml == config.release.yaml
old/off config == evidence/1.1.1/final-artifact/config.final.yaml
```

### 5. 旁路容器内存与 CPU（docker stats 每 10 秒，`stats-summary.json`）

| 运行 / 容器 | 样本数 | 内存 usage 均值 / 最大（MiB） | anon 均值（MiB） | CPU 均值 / 最大（% 单核） |
|---|---|---|---|---|
| 第 1 部分 old（1.1.1） | 19 | 47.6 / 53.0 | 46.4 | 6.83 / 18.35 |
| 第 1 部分 off（1.2.0 关闭） | 19 | 39.3 / 42.8 | 38.4 | 7.23 / 15.60 |
| 第 1 部分 short | 33 | 13.3 / 13.8 | 12.7 | 4.91 / 8.65 |
| 第一次 prod | 79 | 49.8 / 54.9 | 47.9 | 4.88 / 12.01 |
| 重跑 short | 32 | 47.8 / 52.7 | 45.9 | 4.56 / 10.56 |
| 重跑 prod | 74 | 35.8 / 36.8 | 33.8 | 4.43 / 14.61 |

old 与 off 同时承载两路抓取（Gateway 14 个话题（12 个 tick/kline + 2 个 quote） + 旧 `/ws` 12 个话题），内存与 CPU 同量级（off 内存略低）。各容器都远低于 256 MiB 限额；同一产物不同容器的内存差异来自 jemalloc 高水位与启动时刻的行情量，不是本功能引入的增长。容器内 `/metrics` 的 RSS：重跑 short 52.6→54.3 MiB、prod 46.6→42.1 MiB。

### 6. 现网前后快照（`canary/prod-readonly.jsonl`、`canary-rerun/prod-readonly.jsonl`）

| 时刻（UTC） | process_start_time_seconds | RSS（MiB） | md_ws_active_clients | 订阅数 |
|---|---|---|---|---|
| 16:06:00 第一次前 | 1792785202 | 97.0 | 2 | 14 |
| 16:14:20 第 1 部分后 | 1792785202 | 97.3 | 2 | 14 |
| 16:29:33 第一次后 | 1792785202 | 96.5 | 2 | 16 |
| 16:30:01 重跑前 | 1792785202 | 96.8 | 2 | 17 |
| 16:45:35 重跑后 | 1792785202 | 97.2 | 2 | 14 |

现网订阅数的起落是 botd 在现网浏览 K 线时自己的 POST / DELETE；金丝雀消费者的 POST / DELETE 只发往旁路容器端口（28122 / 28123），对现网只发 GET。`cleanup-check.txt`（16:46:05Z）：现网仍是 `md-server:1.1.1`（`3250e021…`）、`RestartCount 0`、启动于 `2026-10-23T19:53:21Z`、healthy。

### 7. 清理

```
$ cat evidence/1.2.0/nas-198/cleanup-check.txt
containers with purpose=md-runtime-subs-1.2.0 or named md-exp-120-*: none
images md-server:exp-120-* / md-server:1.2.0: none
md-server* containers: [('/md-server-1.1.0-quote-bnb-bak', …, 'created'), ('/md-server', 'md-server:1.1.1', 'running')]
```

`md-server-1.1.0-quote-bnb-bak` 是 1.1.1 换版时保留的旧回退容器（金丝雀前就在），未动。

## 三、没做什么与原因

- **换版**：按指示等主审复核金丝雀结果后再定窗口；未构建 `md-server:1.2.0` 发布镜像，未动现网容器。
- **第 1 部分没有重跑**：第一次已通过，判据问题只涉及第 2 部分的配置订阅判定。
- 第 2 部分重跑是两个容器同时跑（不是先后），省 8 分钟；每个容器独立灌订阅、独立消费者，互不影响。

## 四、兼容性声明

第 1 部分在 198 上用最终产物重验了 docs/24 §三 的 1、2、3、7 条（推送逐字节、订阅列表、旧 `/ws` 无 quote、指标只增），第 8 条（资源同量级）见 §二-5；4、5、6 条与第二轮报告相同（proto 未动、旧配置可用、md-tests 真实执行）。开关打开、报价补订关闭时，名单外报价话题收不到数据，与 1.1.1 相同。

## 五、需要对方做的事

1. **主审**：复核本报告，特别是第一次运行的判据误报与修正后的重跑（`canary/` 与 `canary-rerun/` 都保留了原始 `timeline.jsonl` 和 `summary.json`）。
2. **业务方**：复核通过后确定换版窗口（低峰），届时再开一次 Portainer 临时会话；我按 `SWAP_PLAN_1.2.0.md` 执行，执行前把时间告诉你转主审。

账号凭据处理：只在一次前台命令里作为环境变量传给登录脚本（不进 argv，前台命令不进终端快照文件）；登录脚本放在 `/tmp`、不含凭据，只把 JWT 以 600 权限写入 `/tmp/pt_jwt`；结束后 JWT 与登录脚本都已删除；仓库、证据目录与临时日志里检索不到密码。
