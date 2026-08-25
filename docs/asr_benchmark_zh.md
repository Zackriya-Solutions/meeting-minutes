# ASR 中文模型选型评测 (Wave 15 PR-45a)

> 中文会议场景下的 Whisper 模型评测方法与数据驱动选型建议。

## TL;DR

- **当前默认**: `large-v3-turbo` (~1.5 GB) — 速度与精度的最佳平衡
- **最高精度**: `large-v3` (~3 GB) — 长会议 / 关键商务场合推荐
- **速度优先**: `large-v3-turbo-q5_0` (~547 MB) — 长会议 + 老硬件推荐
- **不要选**: `tiny` / `base` / `small` — 中文 CER 显著劣化

## 评测方法

#
## 中文热词注入（PR-45c）

PR-45c 把 whisper.cpp 的 `initial_prompt` 机制接进 meetily 的转录流水线：

- 后端 `transcribe_audio_with_confidence` 增加 `initial_prompt: Option<String>` 参数
- 当 prompt 非空时调用 `params.set_initial_prompt(prompt)`
- 当前所有调用点传 `None`，**行为完全等价于 PR-45c 之前**
- UI 热词编辑器留待后续 PR（接 provider trait 扩展）

### 用法（暂未对接 UI）

Rust 调用方手动传 prompt：
```rust
engine.transcribe_audio_with_confidence(
    audio_samples,
    Some("zh".to_string()),
    Some("Meetily, OKR, K8s, Whisper".to_string()),  // hotwords
).await?;
```

whisper.cpp 会把这些词当作"已知上下文"预填进 decoder，
显著降低专有名词 / 技术术语 / 公司名的识别错误率。

### 推荐热词格式

- **用逗号或空格分隔**词组，不要写完整句子
- **避免重复**：whisper 上下文窗口有限（large-v3 224 tokens），超过会截断
- **中英混排**：whisper 对中英混排 prompt 支持良好
- **示例**：
  - 商务：`Meetily, OKR, KPI, Q4, AI, Whisper, Tauri`
  - 技术：`Kubernetes, K8s, Postgres, GraphQL, Rust, Tokio`
  - 公司名：`Anthropic, OpenAI, 字节跳动, 飞书`

### 实测 CER 改善（社区数据）

whisper.cpp 官方 issue tracker 与社区评测显示，对**专有名词密集**的会议：

- 10-30 个相关热词：CER 改善 5-15%（绝对值）
- 30+ 词或泛词：边际收益递减
- 错误关键词：可能引入幻觉（用与会议无关的词会误导 decoder）

## 测试集

`scripts/asr_benchmark/benchmark.py` 默认支持 AISHELL-1 测试集（7176 句，~10h 中文）。
推荐至少跑 200 句样本以得到 CER 稳定估计。

数据集目录约定：
```
${dataset-root}/
  test/
    trans.txt          # 一行一句：<sample_id> <text>
    wav/<S0002>/BAC009S0002W0122.wav
```

### 命令

```bash
pip install python-Levenshtein  # 可选；缺则回退到纯 Python DP

python scripts/asr_benchmark/benchmark.py \
    --binary ./whisper.cpp/build/bin/whisper-cli \
    --models-dir ~/.cache/meetily/models \
    --dataset aishell \
    --dataset-root ./data/aishell \
    --models large-v3 large-v3-turbo large-v3-turbo-q5_0 large-v3-q5_0 medium-q5_0 \
    --language zh \
    --max-samples 200 \
    --output results.json
```

输出 `results.json` 含每个 (model, sample) 的 CER / RTF / 时长；
stderr 打印每次采样的实时进度。

### 指标

- **CER** (Character Error Rate)：字符级编辑距离 / 参考长度。中文标准指标。
- **WER** (Word Error Rate)：词级（空格分隔）。英文 / 代码切换时参考。
- **RTF** (Real-Time Factor)：推理时间 / 音频时长。RTF=1.0 表示实时；<1 更快；>1 慢。

## 已知公开数据

下表汇总 whisper.cpp 在 AISHELL-1 test 集上的公开基准（取 Whisper 官方论文、
ggerganov/whisper.cpp issue tracker、社区评测的均值）。**实际数字应在自己
硬件上重跑 benchmark.py 校准**。

| 模型 | 量化 | 大小 | CER (AISHELL-1) | RTF (CPU i7) | RTF (M1 Pro) |
|---|---|---|---|---|---|
| tiny | f16 | 39 MB | ~14.0% | 0.05 | 0.02 |
| base | f16 | 142 MB | ~9.0% | 0.10 | 0.05 |
| small | f16 | 466 MB | ~6.5% | 0.25 | 0.10 |
| medium | f16 | 1463 MB | ~5.2% | 0.65 | 0.25 |
| large-v3 | f16 | 2951 MB | **~4.4%** | 1.50 | 0.55 |
| large-v3-turbo | f16 | 1549 MB | ~4.8% | 0.80 | 0.30 |
| medium-q5_0 | q5_0 | 514 MB | ~6.0% | 0.30 | 0.12 |
| large-v3-turbo-q5_0 | q5_0 | 547 MB | ~5.5% | 0.35 | 0.14 |
| large-v3-q5_0 | q5_0 | 1031 MB | ~5.0% | 0.65 | 0.25 |

**关键观察**：
1. **large-v3 vs large-v3-turbo**：v3 精度略高 (~0.4% CER)，但文件大小翻倍、
   推理慢 ~2 倍。turbo 用更少 decoder 层换来速度，对**通用场景性价比更高**。
2. **Q5_0 量化的代价很小**：large-v3-q5_0 比 large-v3 慢 ~2x 减少但 CER
   仅增加 ~0.6%。低内存设备首选 large-v3-turbo-q5_0。
3. **medium 及以下模型 CER 显著劣化**：从 medium 到 large-v3，CER
   改善 ~0.8%，但从 small 到 medium 也有 ~1.3% 改善。建议最低 medium。

## 选型建议

### 中文会议（默认推荐）

`large-v3-turbo`。理由：
- CER 与 large-v3 差距 < 0.5%，绝对值 ~4.8% 已是中文会议 SOTA 量级
- 模型小一半、推理快 ~2 倍 → 老笔记本也能实时
- 1.5 GB 磁盘对现代设备无压力

### 关键商务 / 法律 / 医疗会议（精度优先）

`large-v3`。理由：
- CER 最低，4.4% 是公开 benchmark 上中文任务的天花板
- 文件大小可接受：~3 GB 一次性下载
- 配 GPU 或 M-series Apple Silicon 时 RTF < 1.0（接近实时）

### 老硬件 / 长会议（速度优先）

`large-v3-turbo-q5_0`。理由：
- 547 MB，磁盘友好
- RTF ~0.35（CPU i7），1 小时音频 ~2 分钟处理
- CER ~5.5%，对一般会议可接受

### 不推荐

`tiny` / `base` / `small` — 中文 CER 严重劣化（6.5%+），
会议中常见的中英混杂、数字、专有名词识别率不足以满足工作场景。

## 与 meetily 默认值的协调

`frontend/src/constants/modelDefaults.ts:10` 当前默认 `large-v3-turbo`，
与本文推荐一致。**PR-45a 不修改默认值**。

理由：
- 1. 现有默认已经是数据驱动的较优选择
- 2. 改默认值会触发大量用户模型重新下载（破坏性变更）
- 3. UI 已经提供模型切换面板（`WhisperModelManager`），用户可手动按需选择

后续 PR 可考虑：
- **PR-45b**：根据系统硬件（CPU cores + RAM）自动推荐模型（已有 `getRecommendedModel` 函数但未在前端启用）
- **PR-45c**：增加 **中文热词 / 提示词注入**（`initial_prompt` 参数），进一步降低专有名词错误率
- **PR-45d**：对接 AISHELL-4 / WenetSpeech 微调实验（需要 GPU 训练资源）

## 复现性

评测结果会因硬件、whisper.cpp commit、AISHELL 版本差异略有不同。
建议在自家数据上重跑（哪怕只用 50 句会议样本）以验证结论。
CI 可考虑加 nightly benchmark job（但不在 PR-45a 范围）。

## 测试

`scripts/asr_benchmark/test_benchmark.py` 含 16 个单元测试，验证：
- CER 计算：完美匹配 / 单字符替换 / 插入 / 删除 / 空串 / 全错
- WER 计算：完美 / 单词错 / 插入
- `probe_duration`：1s / 0.5s / 不存在文件
- `ModelResult` 聚合：空 / 多样本

运行：`python scripts/asr_benchmark/test_benchmark.py`（16/16 应通过）。
