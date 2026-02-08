# AGENTS.md

本文件给在 `clipboard-history` 仓库工作的自动化编码代理使用。
目标：先核实、再修改；确保改动可运行、可验证、可维护。

## 1) 项目基线

- 技术栈：Tauri 2 + Vue 3 + Vite。
- 前端：JavaScript + Vue SFC（`<script setup>`），无 TypeScript。
- 后端：Rust（Tauri commands、剪贴板、托盘、快捷键、持久化）。
- 包管理器：npm（有 `package-lock.json`）。
- 前端入口：`src/main.js`。
- 前端主界面：`src/App.vue`。
- 后端入口：`src-tauri/src/main.rs`。
- 后端核心：`src-tauri/src/lib.rs`。
- Tauri 配置：`src-tauri/tauri.conf.json`。

## 2) 已验证命令

默认在仓库根目录执行。

```bash
# 安装依赖
npm install

# 前端开发 / 构建 / 预览
npm run dev
npm run build
npm run preview

# Tauri CLI 入口（透传）
npm run tauri -- <subcommand>

# 桌面联调 / 打包
npm run tauri dev
npm run tauri build
```

说明：`package.json` 当前仅有 `dev/build/preview/tauri` 四个脚本。

## 3) lint / test / typecheck 现状（2026-02-08）

- 未配置 ESLint/Prettier 命令。
- 未配置 Vitest/Jest 命令。
- 未配置 `tsc` 检查（项目为 JavaScript）。
- Rust 无现成 `#[test]` 用例入口。

## 4) 单测执行规则（重点）

### 4.1 当前结论

当前仓库没有可直接执行的单测命令，不要假设存在 `npm test`。

### 4.2 后续若新增测试框架，优先使用

```bash
# Vitest
npx vitest run path/to/file.test.js
npx vitest -t "case name"

# Jest
npx jest path/to/file.test.js
npx jest -t "case name"

# Rust
cargo test case_name --manifest-path src-tauri/Cargo.toml
```

官方参考：
- Vitest CLI: https://vitest.dev/guide/cli.html
- Jest CLI: https://jestjs.io/docs/cli
- Cargo test: https://doc.rust-lang.org/cargo/commands/cargo-test.html

### 4.3 同步要求

若仓库新增测试能力，必须同步更新：
- 第 3 节的现状说明；
- 本节可执行命令；
- 示例路径与脚本名。

## 5) 通用改动原则

- 优先最小改动修复，不夹带无关重构。
- 修改前先确认命令、文件、配置真实存在。
- 前后端协议改动时，`invoke` 参数与 Rust 结构体同步调整。
- 禁止静默失败，错误必须可观测（日志或用户提示）。
- 涉及存储结构改动时，先考虑迁移与回滚。

## 6) 前端规范（Vue + JS）

### 6.1 导入与组织

- 使用 ES Module。
- 第三方依赖在前，本地模块在后。
- 同来源导入尽量合并，按语义分组。

### 6.2 命名与格式

- 变量/函数：`camelCase`。
- 常量：`UPPER_SNAKE_CASE`。
- 组件文件：`PascalCase.vue`。
- CSS 类名：`kebab-case`。
- 保持现状：双引号、分号、2 空格缩进。

### 6.3 状态与异步

- 组合式 API：`ref`、`computed`、`watch`、`onMounted`、`onUnmounted`。
- 与后端交互统一 `invoke("command")`。
- 异步统一 `async/await`。

### 6.4 错误处理

- 关键异步流程使用 `try/catch`。
- `catch` 里至少包含：
  1) `console.error("context", error)`；
  2) 用户可见提示（如 `notice`）。
- 禁止空 `catch`。

## 7) 后端规范（Rust + Tauri）

### 7.1 Command 与返回

- 对外接口使用 `#[tauri::command]`。
- 返回类型优先 `Result<T, String>`。
- 错误信息必须包含上下文。

### 7.2 命名与序列化

- Rust 内部标识符：`snake_case`。
- 类型名：`PascalCase`。
- 前端字段：`camelCase`，优先 `#[serde(rename_all = "camelCase")]`。
- 必要时使用字段级 `#[serde(rename = "...")]`。

### 7.3 并发与状态

- 全局状态通过 `State<AppState>` 管理。
- 关键共享数据使用 `Mutex`。
- 锁失败返回明确错误，不 panic、不吞错。

### 7.4 存储与文件操作

- 先确保目录结构存在（参考 `ensure_storage_layout`）。
- JSON 写入保持可读（`serde_json::to_string_pretty`）。
- 迁移/删除失败要显式返回错误。

## 8) 业务约束（来自当前实现）

- 历史列表按 `updated_at` 倒序。
- 去重键：`item_type + content_hash`。
- 文本入库前标准化（换行统一 + trim）。
- 设置边界：`poll_interval_ms` 300~5000，`history_limit` 50~5000。
- 快捷键字符串会清洗（如 `Meta -> Super`）。

## 9) Cursor / Copilot 规则

当前未发现以下文件：
- `.cursor/rules/`
- `.cursorrules`
- `.github/copilot-instructions.md`

若后续新增这些规则文件：
- 其约束优先级高于本文件的通用建议；
- 需同步更新本节并写明冲突处理策略。

## 10) 代理执行清单

开始前：
- 先读 `README.md` 与 `功能开发文档.md`。
- 先确认将使用的脚本/命令真实存在。

进行中：
- 只改与目标直接相关的文件。
- 协议变更时同时更新前后端。

完成后：
- 至少验证一条运行链路：`npm run dev` 或 `npm run tauri dev`。
- 涉及构建链路时补跑：`npm run build` 或 `npm run tauri build`。
