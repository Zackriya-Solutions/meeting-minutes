# Wave 8: 稳定性加固（Rust 后端 + 前端恢复提示）

> **For agentic workers:** REQUIRED SUB-KILL: superpowers:subagent-driven-development (recommended) or superpowers:executing-plans.
> **基于分支:** feature/i18n-wave7

## 背景与痛点清单

完成 Wave 1-7 i18n 后,根据用户硬约束（系统稳定运行 + 识别率 + 增加用户基数）优先级,Wave 8 聚焦**稳定性 P1 主轴**。已对 Rust 后端做完整侦察,识别现状与缺口:

| 稳定性路径 | 现状(已实现) | 缺口(本 wave 要修) |
|------------|--------------|---------------------|
| 录音崩溃恢复 | IncrementalAudioSaver 30s checkpoint + 4 个 Tauri command (recover / has / cleanup / scan) | app 启动时未自动扫描孤立 checkpoints;前端无恢复对话框 |
| Whisper 转写 | ParallelProcessor 默认 max_workers=2, max_retries=3, retry_delay_ms=1000, enable_fallback_mode=true, 资源监控 + auto-pause/auto-resume | Whisper engine 本身(CPU/Metal/CUDA)失败时未自动切换 |
| LLM 调用 | REQUEST_TIMEOUT_DURATION = 300s, request_future.await 单次调用 | 无 retry;无 rate limit 处理;无 provider failover;Ollama 客户端本身已有 retry+exponential backoff |
| Audio backend | AudioCaptureBackend ScreenCaptureKit + CoreAudio 全局单例 + 单元测试 | 只有手动切换接口,无自动 fallback 触发(本 wave 不动,defer) |

## Goals

**P1 (本 wave)**:最大化稳定性收益,命中"系统稳定运行 + 识别率"硬约束。
**P2 (defer to Wave 9+)**:ja-JP / ko-KR UI 扩展(继续增加用户基数)。
**P3 (defer)**:CI / build 优化、数据库迁移、streaming retry。

本 wave 4 个 PR,全部聚焦 Rust 后端 + 最小前端改动,沿用一 PR 一 commit pattern。

## Scope
| PR | 范畴 | 主文件 | 期望变更 |
|----|------|--------|----------|
| 31 | LLM retry + 指数退避 | frontend/src-tauri/src/summary/llm_client.rs | generate_summary 包 retry loop;3 次重试,exp backoff 1s/2s/4s;区分 transient (5xx/timeout/network/429) vs permanent (401/403/400/parse);错误消息脱敏 |
| 32 | LLM provider failover | 新建 frontend/src-tauri/src/summary/failover.rs + processor.rs 接入 | provider_chain 配置;primary 失败按顺序尝试 secondary;复用 PR-31 retry;老数据兼容(默认 = 当前单一 provider,UI opt-in) |
| 33 | Orphan checkpoint 检测 + 前端恢复提示 | lib.rs 启动钩子 + database/commands.rs 新增命令 + 新前端组件 | 启动扫描 .checkpoints 残留 → emit 事件 → Modal 弹窗 [恢复] / [丢弃] |
| 34 | Whisper engine fallback 链 | 新建 frontend/src-tauri/src/whisper_engine/fallback.rs | engine init 失败按顺序 CUDA → Metal → CPU → Parakeet;转写阶段连续 3 个 chunk 失败触发切换 |

合计:预计 4 functional PR + 1 docs commit = **5 commit**。
## Architecture

### A. PR-31 LLM retry

`generate_summary` 内部对 transient 错误自动重试,不改调用方签名。

- 新增 `RetryPolicy { max_retries: u32, initial_backoff_ms: u64, max_backoff_ms: u64 }`,默认 `{ max_retries: 3, initial_backoff_ms: 1000, max_backoff_ms: 8000 }`
- 区分错误:
  - **Retryable**: reqwest timeout, connect error, 5xx, 429 (rate limit)
  - **Non-retryable**: 401, 403, 400, parse error, cancellation
- 重试间隔 = `min(initial_backoff * 2^attempt, max_backoff)` + ±20% jitter
- 每次 retry `log::warn!("LLM retry attempt={} reason={}", ...)`
- 错误消息格式:`LLM call failed after 3 retries: <sanitized reason>`,api_key/Bearer 完全屏蔽

### B. PR-32 LLM provider failover

用户配置多 provider 时,primary 失败自动切到 secondary。

- 新建 `summary/failover.rs`:
  - `FailoverChain { providers: Vec<LLMProvider>, per_provider_timeout: Duration, retry_policy: RetryPolicy }`
  - `generate_with_failover(...)`:依次尝试,non-retriable 错误(401/403/400)立即返回(不切下一个 provider),retriable 错误(5xx/timeout/network)切下一个
- `processor.rs` 接入:新增 `pub async fn generate_with_failover_chain(...)` 包装现有 `generate_summary` 调用
- 数据库 `settings` 表加 `provider_chain` (JSON 序列化);启动读不到时默认 = `[<当前 provider>]`
- 前端 settings 页:新增 `ProviderFailoverSection` (最多 5 个 provider,默认折叠,显式 opt-in 才启用)
- i18n:settings.json 两侧加 6 个 key:provider_failover / provider_failover_hint / primary_provider / fallback_providers / add_provider / remove_provider
### C. PR-33 Orphan checkpoint 检测

app 启动时检测上次崩溃留下的 `.checkpoints/`,主动询问是否恢复。

- `lib.rs` `tauri::Builder::setup` 内启动钩子(延迟 2s,避免阻塞 UI):
  - 调用 `database::commands::scan_orphan_checkpoints(app_data_dir)` 返回 `Vec<OrphanCheckpoint>`
  - 通过 `app.emit("orphan-checkpoints-detected", ...)` 发到前端
- `database/commands.rs` 新增(不动 schema,只读):
  - `scan_orphan_checkpoints(app_data_dir) -> Result<Vec<OrphanCheckpoint>, String>`
    - 扫描 `<app_data>/meetings/*/` 下含 `.checkpoints/audio_chunk_*.mp4` 的目录
    - 返回 `Vec<{ meeting_folder, chunk_count, estimated_duration_seconds, last_modified_ms }>`
  - `recover_orphan_checkpoint(meeting_folder) -> Result<PathBuf, String>` (复用 `incremental_saver::recover_from_checkpoints`)
  - `discard_orphan_checkpoint(meeting_folder) -> Result<(), String>` (删 `.checkpoints/` 目录)
- 前端 `app/layout.tsx` 新增 `<OrphanCheckpointListener />` 监听事件
- 新建 `<OrphanCheckpointDialog orphans={...} onRecover={...} onDiscard={...} />`
- 每个孤儿显示:folder name(取 meeting_folder basename)/ chunk 数 / 估算时长 / [恢复] [丢弃] 按钮
- i18n:`recording.json` + `errors.json` 两侧加 8 个 key:orphan_checkpoints_title / orphan_checkpoints_description / orphan_checkpoint_chunks / orphan_checkpoint_duration / orphan_checkpoint_recover / orphan_checkpoint_discard / orphan_checkpoint_recovered / orphan_checkpoint_discarded

### D. PR-34 Whisper engine fallback

engine init 失败 + 连续 chunk 失败时自动切到下一个 backend。

- 新建 `whisper_engine/fallback.rs`:
  - `FallbackEngine { engines: Vec<EngineKind>, current_idx: AtomicUsize, consecutive_failures: AtomicU32 }`
  - `EngineKind { Cuda, Metal, Cpu, Parakeet }`
  - `FallbackEngine::new(preferred: Vec<EngineKind>) -> Result<Self, String>`
    - 按顺序尝试 init,前一个失败(panic / OOM / 加载失败)记 log `fallback from Cuda to Metal due to <reason>` 然后试下一个
    - 全部失败返回 `Err`(由调用方处理,录音继续但转写降级到 Parakeet 或挂起)
  - `record_success()` / `record_failure() -> Option<EngineKind>`
    - 维护 `consecutive_failures: AtomicU32`,成功 reset,失败 +1
    - `>= 3` 时切换下一个 engine 并 reset 计数
- `whisper_engine/mod.rs` 暴露 `FallbackEngine`
- 单元测试覆盖:init failure cascade / 连续 3 次失败触发切换 / 全部失败返回 Err
## Cross-cutting constraints

1. **不引入新依赖**:不引 `tokio-retry` / `backoff` crate,用 `std::thread::sleep` + tokio 自实现;不引 `reqwest-retry` middleware
2. **不改 build 配置**:不动 `next.config.js` / `tailwind.config.ts` / `tauri.conf.json` / Cargo.toml (除非新内部 crate 模块,那也无需改)
3. **一 PR 一 commit**:沿用 Wave 1-7 pattern
4. **三道闸**:
   - `pnpm run check:i18n`(PR-32 / PR-33 涉及 i18n 时必跑)
   - `pnpm test:i18n`
   - `pnpm build` (前端 11 static pages)
   - **新增**: `cd frontend/src-tauri && cargo test --lib` (Wave 8 全部 4 PR 必跑)
5. **不动前端 i18n 已有 JSON**:新增 key 走完整 4 locale 同步流程(check-i18n 自动覆盖)
6. **隐私**:所有错误消息不能 echo api_key / token / 内网 URL;log 输出走 sanitized 字符串
7. **不动数据库 schema**:PR-32 provider_chain 用 JSON 序列化进已有 settings.value 字段,不 ALTER TABLE

## Acceptance criteria

- [ ] PR-31: 单元测试覆盖 500/timeout/200/401/403/429 六种路径;max_retries=3 默认;非 transient 立即返回;错误消息不含 api_key
- [ ] PR-32: provider_chain=[A,B] 时 A retriable 失败 → B 成功;单元素 chain 行为退化为 PR-31;UI opt-in 才启用
- [ ] PR-33: 启动后孤儿 checkpoint 在 5s 内出现在 Modal;恢复/丢弃后清单移除;4 locale 全部翻译;不引新依赖
- [ ] PR-34: CUDA 不可用时自动 fallback 到 Metal/CPU;3 个连续 chunk 失败触发 engine 切换;日志明确 `fallback from X to Y due to Z`
- [ ] 所有改动 `cargo test --lib` 全过;前端 `pnpm build` 11 static pages;无新增依赖
- [ ] 错误消息脱敏:`grep -r "Bearer [a-zA-Z0-9]" frontend/src-tauri/src/` 命中 = 0
## Out of scope (deferred to Wave 9+)

- ja-JP / ko-KR UI 扩展(继续增加用户基数)
- LLM streaming retry(需要重构 response 流处理)
- 前端录音实时波形增强
- 数据库 migration 框架(provider_chain 当前走 JSON-in-setting 容错)
- Audio backend auto-fallback(ScreenCaptureKit ↔ CoreAudio)
- Whisper 模型自动下载/校验/版本管理

## Risk & Mitigation

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| LLM retry 让用户等待 3 次超时(最长 ~24s) | M | M | exp backoff 渐进式;前端显示 "Retrying (2/3)...";cancellation token 可中断 |
| Provider failover 把用户扣费账单放大 | L | H | 默认 chain = 当前单一 provider,**UI 显式 opt-in 才启用**;settings 页加警告横幅 |
| Orphan checkpoint 误判(用户手动备份目录) | L | M | 只扫描 `<app_data>/meetings/*/`,不扫描 Documents/Desktop;扫描时间 < 1s |
| Whisper fallback 让识别率下降 | M | M | fallback 顺序固定 CUDA → Metal → CPU → Parakeet;Parakeet 仅作最后兜底;用户可在 settings 关闭 fallback |
| 数据库 settings 表存 JSON 兼容性 | L | L | 读不到 chain 字段时回退到当前 provider;不破坏老数据 |

## 备注

- Wave 8 优先用 Rust 测试 (`cargo test`) 作为稳定性回归保护
- 前端改动(PR-32 settings UI + PR-33 Modal)严格最小化,新增组件 ≤ 3 个
- 一律沿用 Wave 7 之后的分支命名 `feature/stability-wave8`
- PR-31 → PR-32 强依赖(后者用前者 RetryPolicy);PR-33 / PR-34 互相独立,可并行 PR