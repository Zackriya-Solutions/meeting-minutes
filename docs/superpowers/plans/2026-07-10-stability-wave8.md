# Wave 8: 稳定性加固 — Implementation Plan

> **For agentic workers:** REQUIRED SUB-KILL: superpowers:subagent-driven-development or superpowers:executing-plans.
> **Reference spec:** `docs/superpowers/specs/2026-07-10-stability-wave8.md`
> **Base branch:** feature/i18n-wave7
> **New branch:** feature/stability-wave8

## Task 1: 创建分支 + commit spec/plan

```bash
cd meetily
git checkout feature/i18n-wave7
git checkout -b feature/stability-wave8
git add docs/superpowers/specs/2026-07-10-stability-wave8.md \
        docs/superpowers/plans/2026-07-10-stability-wave8.md
git commit -m "docs(stability): Wave 8 spec + implementation plan (PR-31~34)"
```

## Task 2: PR-31 LLM retry + 指数退避

### 2.1 `frontend/src-tauri/src/summary/llm_client.rs`

在文件顶部加:

```rust
#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub initial_backoff_ms: u64,
    pub max_backoff_ms: u64,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self { max_retries: 3, initial_backoff_ms: 1000, max_backoff_ms: 8000 }
    }
}

fn is_retryable_status(status: reqwest::StatusCode) -> bool {
    status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS
}

fn sanitize_error(e: &str) -> String { e.replace("Bearer ", "Bearer ***") }
```
把 `generate_summary` 内部 send 逻辑包装:

- 把 `client.post(...).send().await` 包到 retry loop
- 区分 retriable (timeout / connect / 5xx / 429) vs non-retriable
- 每次 sleep `min(initial * 2^attempt, max)` + ±20% jitter
- 错误消息走 `sanitize_error`

### 2.2 单元测试 `#[cfg(test)] mod tests`

- 抽 `retry_send(client, req) -> Result<Response, String>` helper,单元测试覆盖其状态码分支
- 6 个 case:200、500→500→200、timeout×3、401、403、429→200
- 错误消息不含 api_key / Bearer 字段

### 2.3 Verify + commit

```bash
cd frontend/src-tauri
cargo test --lib summary::llm_client
cd ../../..
git add frontend/src-tauri/src/summary/llm_client.rs
git commit -m "feat(stability): LLM client retry with exponential backoff (PR-31)"
```

## Task 3: PR-32 LLM provider failover

### 3.1 新建 `frontend/src-tauri/src/summary/failover.rs`

- `FailoverChain { providers: Vec<LLMProvider>, per_provider_timeout: Duration, retry_policy: RetryPolicy }`
- `pub async fn generate_with_failover(...) -> Result<String, String>` 包装 `generate_summary`
- 单元素 chain 退化为单次调用
- 多元素按顺序尝试,retriable 错误切下一个;non-retriable 立即返回

### 3.2 `processor.rs` 接入

- 加 `pub async fn generate_with_chain(...)` 包装
- 老的 `generate_summary` 调用方逐步迁移(本 PR 不全量迁移,只把 settings 有 chain 配置的路径用新函数)

### 3.3 数据库 `settings` 字段

- 通过现有 `settings` repository 读写 `provider_chain`
- key = `llm.provider_chain`,value = JSON `["openai", "ollama"]`
- 老数据:读不到 → 默认 `[<当前 provider>]`

### 3.4 前端 settings UI

- `frontend/src/app/settings/` 下新建 `ProviderFailoverSection.tsx`
- 复用 shadcn `Select` + `Button`,最多 5 个 provider
- 加 6 个 i18n key(中英,4 locale 同步)

### 3.5 Verify + commit

```bash
cd frontend/src-tauri && cargo test --lib summary && cd ../../..
cd frontend && pnpm run check:i18n && pnpm test:i18n && pnpm build && cd ..
git add -A
git commit -m "feat(stability): LLM provider failover chain (PR-32)"
```
## Task 4: PR-33 Orphan checkpoint 检测 + 前端恢复提示

### 4.1 `database/commands.rs` 新增

- `pub struct OrphanCheckpoint { meeting_folder, chunk_count, estimated_duration_seconds, last_modified_ms }`
- `#[tauri::command] scan_orphan_checkpoints(app_data_dir) -> Result<Vec<OrphanCheckpoint>, String>` (只读扫描)
- `#[tauri::command] recover_orphan_checkpoint(meeting_folder) -> Result<String, String>`
- `#[tauri::command] discard_orphan_checkpoint(meeting_folder) -> Result<(), String>`
- 复用 `incremental_saver::recover_from_checkpoints`

### 4.2 `lib.rs` 注册 + 启动钩子

- `.setup(|app| { ... })` 内 `tauri::async_runtime::spawn`
- `tokio::time::sleep(Duration::from_secs(2))` 延迟避免阻塞 UI 启动
- `app_handle.emit("orphan-checkpoints-detected", orphans)` 发到前端
- `lib.rs` `invoke_handler` 加 3 个 command 注册

### 4.3 前端

- `frontend/src/app/layout.tsx` 加 `<OrphanCheckpointListener />`
- 新建 `frontend/src/components/orphan-checkpoint-dialog.tsx`
- 新建 `frontend/src/components/orphan-checkpoint-listener.tsx`
- 监听事件 → setState orphans → 渲染 Modal
- [恢复] 调 `recover_orphan_checkpoint` Tauri command;成功后从 list 移除
- [丢弃] 调 `discard_orphan_checkpoint`;同样移除
- 加 8 个 i18n key(recording.json + errors.json 两侧)

### 4.4 Verify + commit

```bash
cd frontend/src-tauri && cargo build && cd ../../..
cd frontend && pnpm run check:i18n && pnpm test:i18n && pnpm build && cd ..
git add -A
git commit -m "feat(stability): orphan checkpoint detection + recovery dialog (PR-33)"
```

## Task 5: PR-34 Whisper engine fallback 链

### 5.1 新建 `frontend/src-tauri/src/whisper_engine/fallback.rs`

- `enum EngineKind { Cuda, Metal, Cpu, Parakeet }`
- `FallbackEngine { engines: Vec<EngineKind>, current_idx: AtomicUsize, consecutive_failures: AtomicU32 }`
- `FallbackEngine::new(preferred) -> Result<Self, String>` 按顺序 init,失败 cascade 到下一个
- `record_success()` / `record_failure() -> Option<EngineKind>`:`>= 3` 连续失败切换下一个
- 单元测试覆盖 init cascade / 连续 3 次失败切换 / 全部失败返回 Err

### 5.2 `whisper_engine/mod.rs` 暴露

- `pub mod fallback;` + `pub use fallback::*;`

### 5.3 `parallel_processor.rs` 接入

- `transcribe_chunk` 包装:成功 → `engine.record_success()`,失败 → `engine.record_failure()` 切下一个

### 5.4 Verify + commit

```bash
cd frontend/src-tauri
cargo test --lib whisper_engine::fallback
cd ../../..
git add frontend/src-tauri/src/whisper_engine/fallback.rs \
        frontend/src-tauri/src/whisper_engine/mod.rs \
        frontend/src-tauri/src/whisper_engine/parallel_processor.rs
git commit -m "feat(stability): Whisper engine fallback chain (PR-34)"
```
## Task 6: 收尾

```bash
cd meetily
git log --oneline feature/i18n-wave7..feature/stability-wave8
```

期望:5 commit (1 docs + 4 functional)。

## Self-Review Checklist

- [ ] 4 个 PR 都对应独立 commit,message 沿用 `feat(stability): ... (PR-XX)` pattern
- [ ] PR-31 单测覆盖 6 种 status 路径
- [ ] PR-32 provider_chain 读取走已有 settings repository,无 schema 变更
- [ ] PR-33 启动钩子延迟 2s,不阻塞 UI
- [ ] PR-34 fallback 顺序固定,日志明确 reason
- [ ] 三道闸 + cargo test 全过
- [ ] 错误消息脱敏(grep api_key = 0 hits)
- [ ] 不引入新依赖(Cargo.toml 无变化)
- [ ] 4 locale i18n key 全同步(PR-32 / PR-33)
- [ ] 现有录音崩溃恢复命令 (4 个 Tauri command) 不退化