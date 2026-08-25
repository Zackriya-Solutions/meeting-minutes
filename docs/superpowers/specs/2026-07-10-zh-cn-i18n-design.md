# Meetily 简体中文 UI 迭代 — Design Spec

**状态**: Draft (待用户审阅)
**日期**: 2026-07-10
**作者**: Codex (brainstorming session with user)
**范围**: 9 个 PR (PR-11 ~ PR-19)
**估时**: 6-8 周
**目标版本**: Meetily v0.5.0+

---

## §1 背景与目标

### 1.1 背景

Meetily 是隐私优先的 AI 会议助手（22k+ stars，MIT 协议），当前 UI 全英文。仓库已有部分国际化基础：

- `whatlang` 库自动检测转写内容语言
- `lib.rs::LANGUAGE_PREFERENCE` 默认 `auto-translate`
- 但仅服务"转写内容"翻译，不服务"UI 文本"

仓库 22k+ stars 中中文用户占比可观（GitHub 2025 数据显示中文是仅次于英文的活跃语言）。Meetily 桌面应用场景下，中文 UI 直接打开 C 端（自雇咨询、远程团队）和 B 端（中文企业自托管）两个市场。

### 1.2 目标

- 提供完整简体中文 UI 体验
- 即时切换语言不中断录音/转写
- 构建时严格校验翻译完整性，源头堵漏
- 为未来多语言扩展预留接口

### 1.3 成功标准（高层）

9 个 PR 全部合并后：

- 所有用户可见 UI 文本已翻译
- 语言切换无需刷新页面
- 数据库持久化用户语言选择
- OS locale 自动检测生效
- CI 严格校验翻译完整性
- 中文母语开发者走查通过

---

## §2 范围与非目标

### 2.1 In scope

- 9 个 PR 全部完成（PR-11 ~ PR-19）
- 简体中文（zh-CN）翻译
- 所有用户可见 UI 文本
- 翻译质量：专业译员交付
- i18n 基础设施：next-intl 集成、命名空间拆分、Provider 状态管理
- 数据库持久化：ui_language 字段
- 系统语言检测：OS locale 自动识别
- CI 校验：缺失 key 失败
- 术语表：`docs/i18n/glossary.md`

### 2.2 Out of scope

- 繁体中文（zh-TW/HK）、日语、韩语、其他语言（接口预留，但不实现）
- markdown 文档翻译：README.md、CONTRIBUTING.md、CLAUDE.md、PRIVACY_POLICY.md、BLUETOOTH_PLAYBACK_NOTICE.md
- Rust 端日志翻译（保持英文技术日志）
- 翻译管理平台（Crowdin、POEditor 等）
- 后端数据库 i18n（用户生成的会议名等不翻译）

### 2.3 后续 spec 候选（不在本 spec 范围）

- 多语言扩展（繁体、日韩、英文之外的语种）
- 文档翻译
- RTL 语言支持

---

## §3 术语表规范

### 3.1 文件位置

`docs/i18n/glossary.md`，与现有 `docs/architecture.md`、`docs/BUILDING.md` 同级。

### 3.2 格式（Markdown 表格）

| 字段 | 类型 | 说明 |
|---|---|---|
| English | 必填 | 英文原词（与代码中的 key 对应） |
| 中文（简体） | 必填 | 标准译法 |
| 定义 | 必填 | 专业释义，澄清歧义 |
| 适用范围 | 必填 | 哪些模块/页面 |
| 备注 | 可选 | 特殊情况、用户偏好、避免误译 |
| 状态 | 必填 | proposed / approved / deprecated |

### 3.3 初始术语表

PR-11 提交时附 20 条核心术语（详见下表）。PR-13~18 回填过程中会自然产生新术语条目（每加一个非高频词都需要术语表条目），完成时累计 ≥ 30 个 `approved` 术语。

| English | 中文（简体） | 定义 | 适用范围 | 备注 | 状态 |
|---|---|---|---|---|---|
| Recording | 录音 | 录制音频的过程 | 录音模块 | | approved |
| Transcription | 转写 | 语音转文字的过程与产物 | 转写模块 | 动词"to transcribe"用"转写" | approved |
| Summary | 总结 | 会议内容摘要 | 总结模块 | 不用"摘要"避免歧义 | approved |
| Speaker | 说话人 | 会议参与者 | 转写视图 | 与"发言人"区别：发言偏正式 | approved |
| Diarization | 说话人分离 | 区分不同说话人的技术 | 高级设置 | 专业术语可附英文 | proposed |
| Parakeet | Parakeet | NVIDIA 模型名 | 模型选择 | 保留英文专名 | approved |
| Whisper | Whisper | OpenAI 模型名 | 模型选择 | 保留英文专名 | approved |
| Meeting | 会议 | 多人讨论的会话 | 全局 | 不用"会面"避免歧义 | approved |
| Live | 实时 | 同步无延迟 | 描述功能 | 不用"现场" | approved |
| Microphone | 麦克风 | 拾音设备 | 设备选择 | | approved |
| System Audio | 系统音频 | 操作系统输出的声音 | 设备选择 | | approved |
| Device | 设备 | 硬件外设 | 设备管理 | | approved |
| Model | 模型 | AI 模型 | 模型管理 | | approved |
| Download | 下载 | 获取资源 | 模型管理 | | approved |
| Settings | 设置 | 偏好配置 | 设置中心 | 不用"配置"避免与"config"混淆 | approved |
| Profile | 个人资料 | 用户信息 | 设置中心 | | proposed |
| Notification | 通知 | 系统提示 | 通知设置 | | approved |
| Permission | 权限 | 系统授权 | 权限请求 | | approved |
| Update | 更新 | 升级到新版本 | 自动更新 | | approved |
| Cancel | 取消 | 终止操作 | 全局 | | approved |

### 3.4 维护流程

- 译员或开发者提 PR 修改/新增/弃用术语
- 至少 1 名中文母语 reviewer 批准
- 状态变更为 `approved` 后，i18n JSON 才允许使用该译法
- `deprecated` 状态的术语需在下一个 PR 周期内从所有 JSON 中替换

---

## §4 i18n 架构

### 4.1 技术选型

| 选型 | 理由 |
|---|---|
| next-intl | Next.js 14 App Router 一等公民；TypeScript 友好；静态校验；JSON 直接对接术语表 |
| 命名空间拆分 | 与 PR-13~18 按域回填一一对应；每个 PR 只动一个 JSON 文件 |
| NextIntlClientProvider | 客户端 i18n（桌面应用无 SSR）；状态提升实现即时切换 |

### 4.2 文件组织

```
frontend/
└── locales/
    ├── en-US/
    │   ├── common.json
    │   ├── recording.json
    │   ├── transcript.json
    │   ├── summary.json
    │   ├── settings.json
    │   └── errors.json
    └── zh-CN/
        ├── common.json
        ├── recording.json
        ├── transcript.json
        ├── summary.json
        ├── settings.json
        └── errors.json
```

每个 JSON 文件预计 50-150 个 key，总计 300-500 个 key。

### 4.3 Key 命名规范

格式：`{namespace}.{action}.{object}` 三段式，全小写 + 数字 + 下划线。

示例：

- `recording.start_button` — 录音模块的"开始"按钮
- `transcript.view_empty_state` — 转写视图的空状态
- `summary.regenerate_action` — 总结模块的"重新生成"操作
- `settings.tab_general_title` — 设置中心"通用"标签页标题
- `errors.permission_microphone_denied` — 麦克风权限被拒的错误消息

反例：

- `start`（缺命名空间）
- `recordingStart`（驼峰非下划线）
- `transcript.view.empty`（嵌套过深）

### 4.4 Provider 骨架

```typescript
// frontend/src/app/layout.tsx
import { NextIntlClientProvider } from 'next-intl';
import { getLocale } from '@/i18n/request';

export default async function RootLayout({ children }: { children: React.ReactNode }) {
  const locale = await getLocale();
  return (
    <html lang={locale}>
      <body>
        <NextIntlClientProvider locale={locale}>
          {children}
        </NextIntlClientProvider>
      </body>
    </html>
  );
}
```

```typescript
// frontend/src/i18n/request.ts
import { getRequestConfig } from 'next-intl/server';
import { invoke } from '@tauri-apps/api/core';

export const LOCALES = ['en-US', 'zh-CN'] as const;
export type Locale = (typeof LOCALES)[number];
export const DEFAULT_LOCALE: Locale = 'en-US';

export async function getLocale(): Promise<Locale> {
  try {
    const stored = await invoke<string>('get_ui_language');
    if (LOCALES.includes(stored as Locale)) return stored as Locale;
  } catch {
    // Fallback: read OS locale via plugin-os (PR-19)
  }
  return DEFAULT_LOCALE;
}

async function loadMessages(locale: Locale) {
  const [common, recording, transcript, summary, settings, errors] = await Promise.all([
    import(`../locales/${locale}/common.json`),
    import(`../locales/${locale}/recording.json`),
    import(`../locales/${locale}/transcript.json`),
    import(`../locales/${locale}/summary.json`),
    import(`../locales/${locale}/settings.json`),
    import(`../locales/${locale}/errors.json`),
  ]);
  return {
    common: common.default,
    recording: recording.default,
    transcript: transcript.default,
    summary: summary.default,
    settings: settings.default,
    errors: errors.default,
  };
}

export default getRequestConfig(async ({ locale }) => ({
  messages: await loadMessages(locale as Locale),
}));
```

### 4.5 客户端使用

```typescript
// frontend/src/components/Sidebar/Sidebar.tsx
import { useTranslations } from 'next-intl';

export function Sidebar() {
  const t = useTranslations('common');
  return <nav aria-label={t('nav_label')}>...</nav>;
}
```

### 4.6 即时切换

```typescript
// frontend/src/hooks/useLocale.ts
import { useState, useCallback, createContext, useContext } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { Locale, DEFAULT_LOCALE } from '@/i18n/request';

const LocaleContext = createContext<{
  locale: Locale;
  setLocale: (l: Locale) => Promise<void>;
}>({ locale: DEFAULT_LOCALE, setLocale: async () => {} });

export function LocaleProvider({ initial, children }: { initial: Locale; children: React.ReactNode }) {
  const [locale, setLocaleState] = useState<Locale>(initial);
  const setLocale = useCallback(async (newLocale: Locale) => {
    await invoke('set_ui_language', { language: newLocale });
    setLocaleState(newLocale);
  }, []);
  return <LocaleContext.Provider value={{ locale, setLocale }}>{children}</LocaleContext.Provider>;
}

export const useLocale = () => useContext(LocaleContext);
```

---

## §5 数据库迁移 (PR-12)

### 5.1 迁移文件

`frontend/src-tauri/migrations/20260810000000_add_ui_language.sql`

```sql
-- Add UI language preference column to settings table
ALTER TABLE settings ADD COLUMN ui_language TEXT NOT NULL DEFAULT 'en-US';
```

### 5.2 Tauri Commands

注：项目已有 `SettingsRepository::get_model_config` 用 `SELECT * FROM settings LIMIT 1` 模式访问 settings 表（单行表），`save_model_config` 用 `id='1'` + `ON CONFLICT(id) DO UPDATE` 模式做 upsert。新增的 `get_ui_language` / `set_ui_language` 沿用同样的约定。

```rust
// frontend/src-tauri/src/database/repositories/setting.rs

impl SettingsRepository {
    pub async fn get_ui_language(pool: &SqlitePool) -> Result<String, sqlx::Error> {
        // Follow project pattern: SELECT * FROM settings LIMIT 1
        // (settings table is single-row, accessed by LIMIT 1, not by fixed id)
        let row = sqlx::query("SELECT ui_language FROM settings LIMIT 1")
            .fetch_optional(pool)
            .await?;
        Ok(row
            .map(|r| r.get::<String, _>("ui_language"))
            .unwrap_or_else(|| "en-US".to_string()))
    }

    pub async fn set_ui_language(pool: &SqlitePool, language: &str) -> Result<(), sqlx::Error> {
        // Upsert with id='1' (matches save_model_config convention)
        // If settings table is empty, INSERT a new row with empty defaults
        // If row exists with id='1', UPDATE only the ui_language field
        sqlx::query(
            r#"
            INSERT INTO settings (id, provider, model, whisperModel, ui_language)
            VALUES ('1', '', '', '', ?)
            ON CONFLICT(id) DO UPDATE SET ui_language = excluded.ui_language
            "#,
        )
        .bind(language)
        .execute(pool)
        .await?;
        Ok(())
    }
}
```

```rust
// frontend/src-tauri/src/database/commands.rs

#[tauri::command]
pub async fn get_ui_language(state: tauri::State<'_, AppState>) -> Result<String, String> {
    state.setting_repo.get_ui_language().await.map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_ui_language(language: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    state.setting_repo.set_ui_language(&language).await.map_err(|e| e.to_string())
}
```

### 5.3 关键点

- `settings` 表是单行表（沿用 `LIMIT 1` 访问模式 + `id='1'` 写入约定）
- 字段加 `NOT NULL DEFAULT` 保证向后兼容（现有数据自动填充 `en-US`）
- 不需要单独的 `user_preferences` 表
- 旧字段（`groqApiKey` 等）继续保留（PR 不动其他字段）

---

## §6 PR 拆分

### 6.1 总览

| PR | Wave | 范围 | 估时 | 依赖 |
|---|---|---|---|---|
| PR-11 | 1 | i18n 基建 | 1 周 | — |
| PR-12 | 2 | ui_language 字段 | 0.5 周 | PR-11 |
| PR-13 | 2 | common + 导航 | 1 周 | PR-11 |
| PR-14 | 3 | 录音模块 | 1 周 | PR-11, PR-13 |
| PR-15 | 3 | 转写模块 | 1 周 | PR-11, PR-13 |
| PR-16 | 3 | 总结模块 | 1 周 | PR-11, PR-13 |
| PR-17 | 3 | 设置中心 | 1.5 周 | PR-11, PR-13 |
| PR-18 | 3 | 错误/toast | 1 周 | PR-11, PR-13 |
| PR-19 | 4 | 系统语言检测 | 0.5 周 | PR-12 |

### 6.2 详细展开

#### PR-11 i18n 基础设施

**新建文件**：

- `frontend/locales/{en-US,zh-CN}/{common,recording,transcript,summary,settings,errors}.json`（12 个文件）
- `frontend/src/i18n/config.ts`
- `frontend/src/i18n/request.ts`
- `frontend/src/hooks/useLocale.ts`
- `frontend/scripts/check-i18n.ts`
- `docs/i18n/glossary.md`
- `docs/i18n/README.md`

**修改文件**：

- `frontend/package.json`（加 next-intl 依赖 + `check:i18n` script）
- `frontend/next.config.js`（加 next-intl plugin）
- `frontend/src/app/layout.tsx`（包 NextIntlClientProvider + LocaleProvider）
- `.github/workflows/ci.yml`（加 `pnpm run check:i18n` step）
- `frontend/src/app/page.tsx`（改 5-10 个硬编码字符串做样例）

**估时**：1 周
**验收**：

- `pnpm tauri:dev` 启动，能切换 en-US ↔ zh-CN，5-10 个样例字符串即时刷新
- `pnpm run check:i18n` 通过
- 术语表至少 20 个 `approved` 术语
- 6 个 JSON 文件 baseline（en-US 满，zh-CN 译 5-10 个样例）

#### PR-12 ui_language 数据库字段

**新建**：`frontend/src-tauri/migrations/20260810000000_add_ui_language.sql`
**修改**：

- `frontend/src-tauri/src/database/repositories/setting.rs`（加 `get_ui_language` / `set_ui_language`）
- `frontend/src-tauri/src/database/commands.rs`（加 Tauri commands）
- `frontend/src-tauri/src/lib.rs`（注册 commands）
- `frontend/src/i18n/request.ts`（从 settings 表读默认 locale）

**估时**：0.5 周
**验收**：

- 全新安装：`settings.ui_language = 'en-US'`
- 旧版本升级：迁移后旧数据自动填充 `'en-US'`
- `get_ui_language` / `set_ui_language` commands 可调用

#### PR-13 公共 + 导航

**修改**：

- `frontend/locales/zh-CN/common.json`（译 ~80 个 key）
- `frontend/src/components/Sidebar/Sidebar.tsx`
- `frontend/src/components/Sidebar/SidebarProvider.tsx`
- `frontend/src/components/MainNav/MainNav.tsx`
- `frontend/src/components/Logo.tsx`
- 新建 `frontend/src/components/LanguagePickerPopover/LanguagePickerPopover.tsx`（语言切换器）

**估时**：1 周
**验收**：

- 侧边栏、主导航、Logo 全部中文
- `LanguagePickerPopover` 出现在 `MainNav`，可切换并持久化

#### PR-14 录音模块

**修改**：

- `frontend/locales/zh-CN/recording.json`（译 ~80 个 key）
- `frontend/src/components/RecordingControls/RecordingControls.tsx`
- `frontend/src/components/RecordingStatusBar/RecordingStatusBar.tsx`
- `frontend/src/components/DeviceSelection/DeviceSelection.tsx`
- `frontend/src/components/RecordingSettings/RecordingSettings.tsx`

**估时**：1 周
**验收**：录音相关所有 UI 元素中文，按钮、提示、错误消息全覆盖

#### PR-15 转写模块

**修改**：

- `frontend/locales/zh-CN/transcript.json`（译 ~100 个 key）
- `frontend/src/components/TranscriptView/TranscriptView.tsx`
- `frontend/src/components/TranscriptView/VirtualizedTranscriptView.tsx`
- `frontend/src/components/TranscriptSettings/TranscriptSettings.tsx`
- `frontend/src/components/TranscriptRecovery/TranscriptRecovery.tsx`

**估时**：1 周
**验收**：转写视图、设置、恢复 UI 全中文

#### PR-16 总结模块

**修改**：

- `frontend/locales/zh-CN/summary.json`（译 ~80 个 key）
- `frontend/src/components/AISummary/AISummary.tsx`
- `frontend/src/components/EmptyStateSummary/EmptyStateSummary.tsx`
- `frontend/src/components/SummaryModelSettings/SummaryModelSettings.tsx`
- `frontend/src/components/SummaryLanguageSettings/SummaryLanguageSettings.tsx`

**估时**：1 周
**验收**：总结生成、模板选择、模型设置全中文

#### PR-17 设置中心其余

**修改**：

- `frontend/locales/zh-CN/settings.json`（译 ~120 个 key）
- `frontend/src/components/SettingTabs/SettingTabs.tsx`
- `frontend/src/components/BetaSettings/BetaSettings.tsx`
- `frontend/src/components/PreferenceSettings/PreferenceSettings.tsx`
- `frontend/src/components/ModelDownloadProgress/ModelDownloadProgress.tsx`
- `frontend/src/components/WhisperModelManager/WhisperModelManager.tsx`
- `frontend/src/components/ParakeetModelManager/ParakeetModelManager.tsx`
- `frontend/src/components/BuiltInModelManager/BuiltInModelManager.tsx`
- `frontend/src/components/AnalyticsConsentSwitch/AnalyticsConsentSwitch.tsx`

**估时**：1.5 周
**验收**：设置中心全中文，包括模型管理、数据同意等子页面

#### PR-18 错误/toast

**修改**：

- `frontend/locales/zh-CN/errors.json`（译 ~60 个 key）
- `frontend/src/components/MessageToast/MessageToast.tsx`
- `frontend/src/components/ConfirmationModel/ConfirmationModel.tsx`
- `frontend/src/components/UpdateDialog/UpdateDialog.tsx`
- `frontend/src/components/UpdateNotification/UpdateNotification.tsx`
- `frontend/src/components/ConsoleToggle/ConsoleToggle.tsx`
- `frontend/src/components/BluetoothPlaybackWarning/BluetoothPlaybackWarning.tsx`

**估时**：1 周
**验收**：所有 toast、确认对话框、更新提示、蓝牙警告全中文

#### PR-19 系统语言检测

**新建**：`frontend/src-tauri/src/lib/ui_language.rs`（封装 OS locale 读取）
**修改**：

- `frontend/src-tauri/src/lib.rs`（启动时检测 OS locale）
- `frontend/src-tauri/src/database/commands.rs`（首次启动写入 settings）
- `frontend/src/components/onboarding/Onboarding.tsx`（首启默认语言）

**估时**：0.5 周
**验收**：

- 中文 OS 首次启动 → 默认 `zh-CN`
- 其他 OS 首次启动 → 默认 `en-US`
- 用户手动切换后，OS locale 变化不影响

---

## §7 CI 校验

### 7.1 脚本 `frontend/scripts/check-i18n.ts`

```typescript
#!/usr/bin/env tsx
import { readFileSync, readdirSync, statSync } from 'fs';
import { join, relative } from 'path';

const LOCALES = ['en-US', 'zh-CN'];
const LOCALES_DIR = join(__dirname, '..', 'locales');
const SRC_DIRS = [join(__dirname, '..', 'src'), join(__dirname, '..', 'app')];

function loadLocaleKeys(locale: string): Set<string> {
  const dir = join(LOCALES_DIR, locale);
  const keys = new Set<string>();
  for (const file of readdirSync(dir)) {
    if (!file.endsWith('.json')) continue;
    const ns = file.replace('.json', '');
    const data = JSON.parse(readFileSync(join(dir, file), 'utf-8'));
    flatten(data, ns, keys);
  }
  return keys;
}

function flatten(obj: any, prefix: string, out: Set<string>): void {
  for (const [k, v] of Object.entries(obj)) {
    const key = `${prefix}.${k}`;
    if (typeof v === 'object' && v !== null) flatten(v, key, out);
    else out.add(key);
  }
}

function extractKeys(file: string): Set<string> {
  const src = readFileSync(file, 'utf-8');
  const keys = new Set<string>();
  const patterns = [
    /useTranslations\(['"`]([^'"`]+)['"`]\)/g,
    /t\(['"`]([a-z][a-z0-9_]*\.[a-z0-9_.]+)['"`]/g,
  ];
  for (const pat of patterns) {
    let m;
    while ((m = pat.exec(src)) !== null) {
      const key = m[1];
      if (key.includes('.')) keys.add(key);
    }
  }
  return keys;
}

function walk(dir: string, out: string[] = []): string[] {
  for (const entry of readdirSync(dir)) {
    const p = join(dir, entry);
    if (statSync(p).isDirectory()) walk(p, out);
    else if (/\.(ts|tsx)$/.test(entry)) out.push(p);
  }
  return out;
}

let errors = 0;
const allUsed = new Set<string>();
for (const dir of SRC_DIRS) {
  for (const file of walk(dir)) {
    for (const k of extractKeys(file)) allUsed.add(k);
  }
}

const enKeys = loadLocaleKeys('en-US');
for (const k of allUsed) {
  if (!enKeys.has(k)) {
    console.error(`ERROR: Key "${k}" used in code but missing from en-US JSON`);
    errors++;
  }
}

for (const locale of LOCALES) {
  const keys = loadLocaleKeys(locale);
  for (const k of enKeys) {
    if (!keys.has(k)) {
      console.error(`ERROR: Locale "${locale}" missing key "${k}"`);
      errors++;
    }
  }
}

if (errors > 0) {
  console.error(`\n${errors} i18n issue(s) found`);
  process.exit(1);
}
console.log('i18n check passed');
```

### 7.2 package.json script

```json
{
  "scripts": {
    "check:i18n": "tsx scripts/check-i18n.ts"
  }
}
```

### 7.3 GitHub Actions

在现有 `.github/workflows/ci.yml` 加一步：

```yaml
- name: Check i18n completeness
  run: pnpm run check:i18n
  working-directory: frontend
```

---

## §8 测试策略

### 8.1 单元测试

- `tests/i18n/glossary.test.ts`：术语表 schema 校验（必填字段、合法状态值）
- `tests/i18n/locale_json.test.ts`：所有 locale JSON 格式校验（无空字符串、无重复 key）
- `tests/i18n/key_consistency.test.ts`：en-US 包含的 key 必须在 zh-CN 也包含

### 8.2 E2E 测试

`tests/e2e/i18n.spec.ts`（Playwright）：

```typescript
test('switch to zh-CN shows Chinese UI', async ({ page }) => {
  await page.goto('/');
  await page.getByTestId('language-picker').click();
  await page.getByText('简体中文').click();
  await expect(page.getByTestId('sidebar')).toContainText('设置');
  await expect(page.getByTestId('sidebar')).not.toContainText('Settings');
});

test('language switch is instant (no reload)', async ({ page }) => {
  await page.goto('/');
  const start = Date.now();
  await page.getByTestId('language-picker').click();
  await page.getByText('简体中文').click();
  await expect(page.getByTestId('sidebar')).toContainText('设置');
  expect(Date.now() - start).toBeLessThan(500);
});
```

### 8.3 手动验收

每 Wave 结束：

- 中文母语开发者全 UI 走查
- 对照术语表 `docs/i18n/glossary.md` 检查专业术语
- 检查长字符串溢出（用 Chrome DevTools 设置 200% 缩放）

---

## §9 风险与缓解

| 风险 | 影响 | 概率 | 缓解 |
|---|---|---|---|
| 译员离职 / 不可用 | PR-13~18 卡壳 | 中 | 至少 2 名译员；术语表在仓库，承接方便 |
| i18n key 命名不一致 | 维护困难，CI 误报 | 高 | PR-11 制定 key 规范文档；CI 验证命名格式 |
| 字符串持续新增，PR 节奏破坏 | 漏翻、CI 红 | 高 | 强制每个 PR 改组件必须同步 JSON；code review 必查 |
| 桌面应用 locale 与 OS locale 冲突 | 用户困惑 | 中 | 优先级：用户主动选择 > OS locale > en-US 默认 |
| 长字符串导致 UI 溢出 | 排版问题 | 中 | next-intl ICU plural 支持；CSS 用 `min-width` 而非固定值 |
| 旧组件 hardcoded 英文散落 | 漏翻 | 高 | `check-i18n.ts` 包含 grep 检测 `>[A-Z][a-z]+<` 模式 |
| CONTRIBUTING.md 说 devtest 分支存在，实际不存在 | 新人迷茫 | 中 | 本 spec 假设基于 main；提交时附注 devtest 状态 |
| next-intl 与 Tauri build 兼容性 | 编译失败 | 低 | PR-11 阶段先做小实验验证；如失败回退到 react-i18next |
| 翻译内容里包含 `{variable}`，译员误删 | 运行时错误 | 中 | CI 检查 JSON 模板字符串完整性；术语表示例明确"占位符" |
| zh-CN 字符在 macOS/Windows/Linux 字体差异 | 显示问题 | 低 | CSS 字体回退 `PingFang SC, Microsoft YaHei, sans-serif` |

---

## §10 时间线

### 10.1 里程碑

| 节点 | 周次 | 可演示物 |
|---|---|---|
| Wave 1 结束 | W1 | 切换 en-US ↔ zh-CN 看到 5-10 个样本字符串；术语表 20 条；CI 红绿 |
| Wave 2 结束 | W3 | 数据库持久化；侧边栏/导航/Logo 中文；语言切换器 |
| Wave 3 结束 | W6 | 各域全中文（录音/转写/总结/设置/错误） |
| Wave 4 结束 | W7 | OS locale 自动检测；首启默认中文 |

### 10.2 关键依赖

- **译员可用性**：必须 Wave 1 之前确认译员到位，否则 Wave 2 之后无法推进
- **PR-11 决策代价大**：next-intl 选型一旦合入，后续 8 个 PR 全部基于此。技术上可回退到 react-i18next（API 类似），但需重写 §4.4 §4.6 的 Provider/LocaleProvider；故 PR-11 实施前应做最小验证（先在一个简单组件上做 spike，确认 next-intl + Tauri build 链路通）

### 10.3 总时长估算

- 乐观：6 周（1 译员稳定 + 1 开发）
- 保守：8 周（译员中途更换 / 术语表评审慢）

---

## §11 验收标准

### 11.1 每个 PR 验收（共同）

- CI 通过（包括 `pnpm run check:i18n`）
- 至少 1 名 maintainer 批准
- 至少 1 名中文母语 reviewer 批准（针对翻译内容）
- 对应 JSON 文件 key 数量符合估时
- 手动测试：核心场景无回归

### 11.2 9 个 PR 全部完成验收

- [ ] 全新安装 → 默认 en-US，OS 是中文 → 启动后默认 zh-CN
- [ ] 用户在 `LanguagePickerPopover` 切换 → 立即生效，UI 全刷新
- [ ] 切换后关掉应用再开 → 选择保留
- [ ] `pnpm run check:i18n` 通过，无漏翻
- [ ] 中文母语开发者走查通过
- [ ] 术语表 ≥ 30 个 approved 术语
- [ ] 录音/转写/总结过程中切换语言不中断
- [ ] 设置、错误、toast 全中文
- [ ] 字体在 macOS/Windows/Linux 都不显示方块

### 11.3 发布准备

- 在 README 顶部加"支持语言"徽章：`![i18n](https://img.shields.io/badge/i18n-en--US%20%7C%20zh--CN-blue)`
- 在 PRIVACY_POLICY 增加"语言偏好存储"说明
- 写 release notes 段落

---

## 附录 A: 关键文件清单

### A.1 新建文件

```
docs/i18n/glossary.md
docs/i18n/README.md
frontend/locales/en-US/common.json
frontend/locales/en-US/recording.json
frontend/locales/en-US/transcript.json
frontend/locales/en-US/summary.json
frontend/locales/en-US/settings.json
frontend/locales/en-US/errors.json
frontend/locales/zh-CN/common.json
frontend/locales/zh-CN/recording.json
frontend/locales/zh-CN/transcript.json
frontend/locales/zh-CN/summary.json
frontend/locales/zh-CN/settings.json
frontend/locales/zh-CN/errors.json
frontend/src/i18n/config.ts
frontend/src/i18n/request.ts
frontend/src/hooks/useLocale.ts
frontend/src/components/LanguagePickerPopover/LanguagePickerPopover.tsx
frontend/src-tauri/src/lib/ui_language.rs
frontend/src-tauri/migrations/20260810000000_add_ui_language.sql
frontend/scripts/check-i18n.ts
frontend/tests/i18n/glossary.test.ts
frontend/tests/i18n/locale_json.test.ts
frontend/tests/i18n/key_consistency.test.ts
frontend/tests/e2e/i18n.spec.ts
```

### A.2 修改文件

```
frontend/package.json
frontend/next.config.js
frontend/src/app/layout.tsx
frontend/src/app/page.tsx
frontend/src/components/Sidebar/Sidebar.tsx
frontend/src/components/Sidebar/SidebarProvider.tsx
frontend/src/components/MainNav/MainNav.tsx
frontend/src/components/Logo.tsx
frontend/src/components/RecordingControls/RecordingControls.tsx
frontend/src/components/RecordingStatusBar/RecordingStatusBar.tsx
frontend/src/components/DeviceSelection/DeviceSelection.tsx
frontend/src/components/RecordingSettings/RecordingSettings.tsx
frontend/src/components/TranscriptView/TranscriptView.tsx
frontend/src/components/TranscriptView/VirtualizedTranscriptView.tsx
frontend/src/components/TranscriptSettings/TranscriptSettings.tsx
frontend/src/components/TranscriptRecovery/TranscriptRecovery.tsx
frontend/src/components/AISummary/AISummary.tsx
frontend/src/components/EmptyStateSummary/EmptyStateSummary.tsx
frontend/src/components/SummaryModelSettings/SummaryModelSettings.tsx
frontend/src/components/SummaryLanguageSettings/SummaryLanguageSettings.tsx
frontend/src/components/SettingTabs/SettingTabs.tsx
frontend/src/components/BetaSettings/BetaSettings.tsx
frontend/src/components/PreferenceSettings/PreferenceSettings.tsx
frontend/src/components/ModelDownloadProgress/ModelDownloadProgress.tsx
frontend/src/components/WhisperModelManager/WhisperModelManager.tsx
frontend/src/components/ParakeetModelManager/ParakeetModelManager.tsx
frontend/src/components/BuiltInModelManager/BuiltInModelManager.tsx
frontend/src/components/AnalyticsConsentSwitch/AnalyticsConsentSwitch.tsx
frontend/src/components/MessageToast/MessageToast.tsx
frontend/src/components/ConfirmationModel/ConfirmationModel.tsx
frontend/src/components/UpdateDialog/UpdateDialog.tsx
frontend/src/components/UpdateNotification/UpdateNotification.tsx
frontend/src/components/ConsoleToggle/ConsoleToggle.tsx
frontend/src/components/BluetoothPlaybackWarning/BluetoothPlaybackWarning.tsx
frontend/src/components/onboarding/Onboarding.tsx
frontend/src-tauri/src/database/repositories/setting.rs
frontend/src-tauri/src/database/commands.rs
frontend/src-tauri/src/lib.rs
.github/workflows/ci.yml
README.md
PRIVACY_POLICY.md
```

---

## 附录 B: 决策日志

| 决策 | 选项 | 选定 | 理由 |
|---|---|---|---|
| spec 切分粒度 | 三份独立 / 总路线图 / 仅 Wave 1 | 三份独立 | 范围聚焦、便于维护者 review、按月推进 |
| 第一份 spec | 稳定性 / 识别率 / i18n | i18n | 用户基数大；与 Rust 核心解耦，可快速出成果 |
| 翻译资源 | 专业译员 / 母语 AI / 社区 / 暂用 AI | 专业译员 + 术语表 | 最高质量；术语表先于 JSON 落地 |
| i18n 库 | next-intl / i18next / Lingui / 自研 | next-intl | Next.js 14 App Router 一等公民；JSON 直接对接；TS 友好 |
| 文件组织 | 单文件 / 命名空间 / 按页面 / 共享+功能 | 命名空间 | 与按域回填 PR 一一对应 |
| 切换生效 | 即时 / 路由 / 重启 | 即时 | 桌面应用 UX 必须顺滑 |
| 缺失策略 | fallback 英文 / 显示 key / PostHog / CI | 构建时严格 + CI 失败 | 源头堵漏，PR 合入前必须补全 |
| 术语表存储 | docs/ / locales/ / 独立仓库 / 翻译平台 | docs/i18n/glossary.md | git 跟踪、PR 流程与代码一致 |
| 整体方案 | 完整 9 PR / MVP 优先 / 分两阶段 | 完整 9 PR（Approach A） | 一次设计、长期不变 |

---

## 附录 C: 参考资料

- next-intl 官方文档：https://next-intl-docs.vercel.app/
- Tauri 2 国际化实践：https://tauri.app/v1/guides/features/i18n
- ICU MessageFormat 规范：https://unicode-org.github.io/icu/userguide/format_parse/messages/
- 项目已有 `frontend/src-tauri/migrations/`（10 个迁移文件）
- `docs/architecture.md` 项目架构
- `frontend/src-tauri/src/database/repositories/setting.rs` 实际访问模式（`LIMIT 1` + `id='1'`）