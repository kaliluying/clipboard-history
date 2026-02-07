# AGENTS.md

本文件面向在 `clipboard-history` 仓库中工作的自动化编码代理。
目标：快速对齐命令入口、工程约束与代码风格，减少无效试错。

## 1) 项目基线

- 技术栈：Tauri 2 + Vue 3 + Vite。
- 前端语言：JavaScript（非 TypeScript）。
- 后端语言：Rust（Tauri command + 本地存储 + 托盘 + 快捷键）。
- 包管理器：npm（依据 `package-lock.json`）。
- 前端入口：`src/main.js`。
- 主界面：`src/App.vue`。
- 后端入口：`src-tauri/src/main.rs`。
- 后端核心：`src-tauri/src/lib.rs`。
- Tauri 配置：`src-tauri/tauri.conf.json`。

## 2) 命令清单（先确认再执行）

在仓库根目录执行：

```bash
npm install
```

### 2.1 开发/构建

```bash
# 前端开发
npm run dev

# 前端构建
npm run build

# 前端预览
npm run preview

# Tauri CLI 入口（可透传子命令）
npm run tauri -- <subcommand>

# 桌面联调（前端 + Tauri）
npm run tauri dev

# 桌面产物构建
npm run tauri build
```

### 2.2 lint / test / typecheck 现状

- 未配置 ESLint/Prettier 命令。
- 未配置 Vitest/Jest 命令。
- 未配置 `tsc` 类型检查。
- 未提供可直接运行的 Rust 测试模块。

### 2.3 单测运行（重点）

当前仓库没有单测入口，因此不存在“单文件单测”或“按用例名运行”命令。

如后续新增测试，可参考（示例，非当前可用）：

```bash
# Vitest 示例
npx vitest path/to/file.test.js
npx vitest -t "test name"

# Rust 示例
cargo test test_name --manifest-path src-tauri/Cargo.toml
```

## 3) 通用编码原则

1. 修复问题优先最小改动，不做无关重构。
2. 不臆造脚本/配置，先核实仓库是否存在。
3. 变更前后端联动时，先对齐 command 名称与参数结构。
4. 前端对外数据字段保持 camelCase（Rust 侧用 serde rename 保证）。
5. 禁止静默失败，错误必须可观测。

## 4) 前端规范（Vue + JS）

### 4.1 组织方式

- 使用 Vue 3 Composition API，优先 `<script setup>`。
- 状态使用 `ref` / `computed`，生命周期使用 `onMounted` / `onUnmounted`。
- 与后端交互统一使用 `invoke("command")`。
- 异步逻辑使用 `async/await`，避免回调嵌套。

### 4.2 Import 约定

- 使用 ES Module import。
- 第三方依赖在前，本地模块在后。
- 同源导入尽量合并一条语句。

### 4.3 命名与格式

- 变量/函数：`camelCase`。
- 常量：`UPPER_SNAKE_CASE`（如 `DEFAULT_POLL_INTERVAL_MS`）。
- 组件文件：`PascalCase.vue`。
- CSS 类名：`kebab-case`。
- 延续现状：双引号、分号、2 空格缩进。

### 4.4 错误处理

- `invoke` 相关异步流程必须 `try/catch`。
- `catch` 至少包含：
  - `console.error` 输出上下文；
  - 用户可见反馈（如 `notice`）。
- 禁止空 catch。

## 5) 后端规范（Rust + Tauri）

### 5.1 Command 设计

- 对外接口使用 `#[tauri::command]`。
- 返回类型优先 `Result<T, String>`。
- 错误信息需包含上下文，便于前端展示与排查。

### 5.2 命名与序列化

- Rust 内部标识符使用 `snake_case`。
- 前端消费字段使用 `camelCase`：
  - `#[serde(rename_all = "camelCase")]` 或字段级 `#[serde(rename = "...")]`。
- 结构体/枚举类型名使用 `PascalCase`。

### 5.3 状态与并发

- 共享状态通过 `State<AppState>` 管理。
- 并发访问通过 `Mutex` 保护关键区。
- 锁失败时返回明确错误，不吞掉异常。

### 5.4 存储与迁移

- 先确保目录结构存在（参考 `ensure_storage_layout`）。
- JSON 写入保持可读性（当前使用 `to_string_pretty`）。
- 迁移/删除文件出错必须显式返回错误。

## 6) 业务约束（从当前实现提炼）

- 历史项按 `updated_at` 倒序。
- 去重键：`item_type + content_hash`。
- 文本入库前做标准化（换行归一 + trim）。
- 设置值范围：
  - `poll_interval_ms`：300 ~ 5000
  - `history_limit`：50 ~ 5000
- 快捷键需清洗（如 `Meta` -> `Super`）。

## 7) Cursor/Copilot 规则检查

以下规则文件当前未找到：

- `.cursor/rules/`
- `.cursorrules`
- `.github/copilot-instructions.md`

若后续新增这些文件，以其内容为更高优先级约束，并同步更新本节。

## 8) 代理执行建议（仓库定制）

1. 先读 `README.md` 与 `功能开发文档.md`，确认目标是 MVP 还是 V1。
2. 动手前先确认命令存在；不存在就不要执行伪命令。
3. 涉及持久化结构修改时，评估历史数据兼容与迁移风险。
4. 涉及前后端协议变更时，前端 `invoke` 参数和 Rust 结构体同时调整。
5. 提交前至少做一次运行验证：
   - `npm run dev` 或 `npm run tauri dev`
   - 改到构建链路时再跑 `npm run build` 或 `npm run tauri build`

## 9) 维护要求

- 新增 lint/test/typecheck/CI 后，第一时间更新第 2 节。
- 引入 TS 或 lint 规则后，更新第 3~5 节并写清优先级。
- 新增 Cursor/Copilot 规则后，更新第 7 节。
- 保持文档“可执行、可验证、可追溯”，避免空泛描述。
