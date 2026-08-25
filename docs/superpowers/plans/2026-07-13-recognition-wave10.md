# Wave 10 Plan: Recognition Rate Improvement

> **基于 spec:** docs/superpowers/specs/2026-07-13-recognition-wave10.md
> **基于分支:** feature/i18n-wave9
> **新分支:** feature/recognition-wave10

## Task 1 (PR-38) ASR Benchmark 框架

**目标**: 建立可重复的多引擎对比基线

### Step 1.1 — Corpus 准备
- [ ] 收集 20 段会议录音样本（MIT 授权公开数据集）
  - 8 段英文（1对1 / 小组 / 大型）
  - 8 段中文（同样 3 种规模）
  - 4 段中英混合
- [ ] 每段配 ground truth 转写（人工校对）
- [ ] 存 `scripts/asr_bench/corpus/{en,zh,mixed}/audio.wav + truth.txt`
- [ ] 生成 `corpus_manifest.json` (audio_path, truth_path, language, type, duration_sec)

### Step 1.2 — Engine Adapter
- [ ] 定义 `EngineAdapter` interface:
  ```ts
  interface EngineAdapter {
    name: string;
    transcribe(audioPath: string, lang: string): Promise<{text: string, rtf: number, peakMemMb: number}>;
  }
  ```
- [ ] 实现 4 个 adapter:
  - `whisper_api.ts` (OpenAI API 调用)
  - `faster_whisper.ts` (本地 faster-whisper)
  - `whisper_cpp.ts` (本地 whisper.cpp CLI)
  - `parakeet_onnx.ts` (本地 ONNX, PR-39 启用)

### Step 1.3 — Runner + WER 计算
- [ ] `runner.ts`: 顺序跑所有引擎对所有样本，输出 CSV
- [ ] `wer.ts`: 实现 WER (Word Error Rate) + CER (Character Error Rate) 算法
- [ ] 输出列: engine / sample / language / duration / rtf / peak_mem_mb / wer / cer
- [ ] 输出汇总: `bench_results/baseline_$(date).csv` + `baseline_summary.md`

### Step 1.4 — 首次 baseline
- [ ] 运行 benchmark（4 引擎 × 20 样本）
- [ ] 写入 `docs/asr_bench_baseline.md`（决策依据）
- [ ] 决定 PR-39 启用 Parakeet 的依据（WER 优势是否 >20%）

### Step 1.5 — Tests
- [ ] unit test: `wer.ts` 已知输入输出
- [ ] integration test: runner 跑 1 个样本不出错

### Step 1.6 — 三道闸 + commit
- [ ] `pnpm test` 通过
- [ ] `pnpm build` 通过
- [ ] commit: `feat(recognition): ASR benchmark framework (PR-38)`

## Task 2 (PR-39) Parakeet 引擎启用

**前置**: PR-38 证明 Parakeet WER 优势

### Step 2.1 — Cargo 集成
- [ ] 加 dep: `ort = "2.0"` (ONNX runtime)
- [ ] 加 dep: `tokenizers = "0.20"` (Parakeet 用 SentencePiece)
- [ ] 新文件: `src-tauri/src/asr/parakeet.rs`
- [ ] feature flag: `parakeet = ["ort", "tokenizers"]` (默认 off)
- [ ] 下载脚本: `scripts/download_parakeet_model.sh` (~500MB ONNX)

### Step 2.2 — 模型加载 + 转写
- [ ] 实现 `ParakeetEngine::new(model_path: PathBuf)`
- [ ] 实现 `transcribe(audio: &[f32]) -> Result<String>`
- [ ] 性能: NVIDIA GPU 用 CUDA execution provider, CPU fallback
- [ ] 内存: 监控 peak RSS, 记录到 metric

### Step 2.3 — 引擎注册 + 路由
- [ ] `AsrEngine` enum 加 `Parakeet` variant
- [ ] 设置 → 转录模型 → 新选项: "Parakeet (NVIDIA GPU recommended)"
- [ ] 选择 Parakeet 时: 检查 GPU 可用性 + 模型文件存在 + feature enabled
- [ ] 失败时: 自动 fallback 到 Whisper (沿用 PR-34 chain)

### Step 2.4 — UI 翻译 (6 locale)
- [ ] en-US / en-GB: "Parakeet (NVIDIA GPU recommended)"
- [ ] zh-CN: "Parakeet (推荐 NVIDIA GPU)"
- [ ] zh-TW: "Parakeet (推薦 NVIDIA GPU)"
- [ ] ja-JP: "Parakeet (NVIDIA GPU 推奨)"
- [ ] ko-KR: "Parakeet (NVIDIA GPU ??)"

### Step 2.5 — Tests
- [ ] unit test: ParakeetEngine 模型加载失败时返回 Err
- [ ] integration test: feature flag off 时不编译 Parakeet
- [ ] manual test: NVIDIA GPU 上 transcribe 1 段 < 5s

### Step 2.6 — 三道闸 + commit
- [ ] `cargo test` (在沙箱外执行)
- [ ] `pnpm test:i18n` 19/19
- [ ] `pnpm build` 通过
- [ ] commit: `feat(recognition): Parakeet engine integration (PR-39)`

## Task 3 (PR-40) 热词注入

### Step 3.1 — 设置项
- [ ] `preference.transcript.hotwords` 字段 (textarea, max 1000 chars)
- [ ] UI: 设置 → Transcript → "Hotwords / 专业术语" 字段
- [ ] 持久化: preferences.json (沿用现有机制)

### Step 3.2 — ASR 调用注入
- [ ] Whisper: `initial_prompt` 参数 = hotwords 字符串
- [ ] Faster-Whisper: 同上
- [ ] Parakeet: 用 hotwords 增强 lexicon（如果支持）或跳过
- [ ] Whisper.cpp: hotwords 文件 + `--hotwords` 参数

### Step 3.3 — UI 翻译 (6 locale)
- [ ] en-US: "Hotwords (comma-separated, improves recognition of proper nouns)"
- [ ] zh-CN: "热词（逗号分隔，提升专有名词识别）"
- [ ] zh-TW: "熱詞（逗號分隔，提升專有名詞辨識）"
- [ ] ja-JP: "ホットワード（カンマ区切り、固有名詞の認識向上）"
- [ ] ko-KR: "???(?? ??, ???? ?? ??)"

### Step 3.4 — 三道闸 + commit
- [ ] 单元测试: hotwords 注入逻辑
- [ ] 三道闸
- [ ] commit: `feat(recognition): hotwords injection for ASR (PR-40)`

## Task 4 (PR-41) 说话人分离

### Step 4.1 — 选型验证
- [ ] 评估 3 个方案:
  - pyannote.audio (Python, SOTA 但需 Python 依赖)
  - speechbrain ONNX (本地, 中等质量)
  - NeMo speaker diarization (NVIDIA, 需 NeMo)
- [ ] 选 1 个（推荐 speechbrain ONNX: 纯 Rust 可调用，无 Python 依赖）

### Step 4.2 — 集成
- [ ] 新文件: `src-tauri/src/asr/diarization.rs`
- [ ] 接口: `diarize(audio: &[f32]) -> Vec<{start: f32, end: f32, speaker: u8}>`
- [ ] 输出与 ASR segments 对齐（按时间戳）

### Step 4.3 — UI 显示
- [ ] 转写面板: 左侧色块标识说话人
- [ ] hover 说话人 → 显示 "Speaker 1" / "Speaker 2" 等
- [ ] 设置 → Transcript → "Speaker Diarization" 开关

### Step 4.4 — 三道闸 + commit
- [ ] 单元测试
- [ ] 三道闸
- [ ] commit: `feat(recognition): speaker diarization (PR-41)`

## Task 5 (PR-42) LLM 后处理纠错

### Step 5.1 — Prompt 设计
- [ ] 模板:
  ```
  你是会议转写纠错助手。修正以下转写中的同音错字、标点、段落。
  保持原意不变，不要添加新内容。

  [转写开始]
  {raw_transcript}
  [转写结束]

  输出格式：纯文本，无需额外说明。
  ```
- [ ] 语言自适应: 中文转写用中文 prompt，英文用英文 prompt

### Step 5.2 — 实现
- [ ] 新文件: `src-tauri/src/postprocess/transcript.rs`
- [ ] 复用现有 LLM client (沿用 PR-32 failover chain)
- [ ] 配置: 设置 → Transcript → "LLM Post-processing" 开关 (默认 on)
- [ ] 成本: 与摘要 prompt 合并, 不额外 API call

### Step 5.3 — UI 翻译 (6 locale)
- [ ] en-US: "LLM Post-processing (fix homophones, punctuation, paragraphs)"
- [ ] zh-CN: "LLM 后处理（纠同音错字、标点、段落）"
- [ ] zh-TW: "LLM 後處理（修正同音錯字、標點、段落）"
- [ ] ja-JP: "LLM 後処理（同音異義語、句読点、段落を修正）"
- [ ] ko-KR: "LLM ??? (?????, ???, ?? ??)"

### Step 5.4 — 三道闸 + commit
- [ ] 单元测试: prompt 模板渲染
- [ ] 三道闸
- [ ] commit: `feat(recognition): LLM post-processing for transcript (PR-42)`

## Task 6 — Wave 10 收尾

- [ ] 更新 `CHANGELOG.md` (Wave 10 段)
- [ ] 更新 `docs/asr.md` (新引擎说明)
- [ ] commit: `docs(recognition): Wave 10 spec + implementation plan (PR-38~42)`

## 总计

- **5 functional PR + 1 docs commit = 6 commit**
- 预计 4-6 周（PR-38/39 重，PR-40/41/42 轻）
- 关键依赖: Parakeet ONNX 模型下载 + Diarization 模型选型
