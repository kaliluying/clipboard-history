# AGENTS.md
本文件面向在 `clipboard-history` 仓库工作的自动化编码代理。
目标：先核实再修改，保证改动可运行、可验证、可维护。

## 1) 仓库基线（先读）
- 技术栈：Tauri 2 + Vue 3 + Vite。
- 前端：JavaScript + Vue SFC（`<script setup>`），无 TypeScript。
- 后端：Rust（Tauri command、托盘、快捷键、剪贴板、持久化）。
- 包管理器：npm（存在 `package-lock.json`）。
- 前端入口：`src/main.js`；主界面：`src/App.vue`。
- 后端入口：`src-tauri/src/main.rs`；核心逻辑：`src-tauri/src/lib.rs`。
- 关键配置：`package.json`、`vite.config.js`、`src-tauri/tauri.conf.json`、`src-tauri/Cargo.toml`。

## 2) 命令清单（已核实）
默认在仓库根目录执行。

```bash
# 依赖安装
npm install

# 前端
npm run dev
npm run build
npm run preview

# Tauri CLI 透传
npm run tauri -- <subcommand>

# 桌面联调 / 打包
npm run tauri dev
npm run tauri build
```

事实说明：`package.json` 当前仅有 `dev` / `build` / `preview` / `tauri` 四个脚本。

## 3) lint / test / typecheck 现状（2026-02-10）
- 未发现 ESLint 配置与 `lint` 脚本。
- 未发现 Prettier 配置与 `format` 脚本。
- 未发现 Vitest / Jest 配置与 `test` 脚本。
- 未发现 `tsconfig*.json`，无需 `tsc` 检查（项目是 JS）。
- Rust 侧当前未定义 `#[test]` 用例。

结论：不要假设 `npm test`、`npm run lint`、`npm run typecheck` 存在。

## 4) 单测命令规则（重点）
### 4.1 当前仓库
- 当前没有可直接跑单测的现成命令。
- 仅在引入测试框架后，才使用下面命令。

### 4.2 若后续新增测试框架，优先写法

```bash
# Vitest：按文件 / 名称
npx vitest run path/to/file.test.js
npx vitest run -t "case name"

# Jest：按文件 / 名称
npx jest path/to/file.test.js
npx jest -t "case name"

# Cargo：按测试名过滤（指定 manifest）
cargo test case_name --manifest-path src-tauri/Cargo.toml

# Cargo：精确匹配（可选）
cargo test --manifest-path src-tauri/Cargo.toml -- --exact case_name
```

参考文档：
- Vitest CLI: https://vitest.dev/guide/cli.html
- Jest CLI: https://jestjs.io/docs/cli
- Cargo test: https://doc.rust-lang.org/cargo/commands/cargo-test.html

## 5) 通用改动原则
- 最小改动优先，禁止夹带无关重构。
- 先确认命令、文件、配置真实存在，再动手。
- 前后端协议改动需同步：`invoke(...)` 参数与 Rust 结构体字段一致。
- 禁止静默失败，错误必须可观测（日志或用户可见提示）。
- 涉及存储结构改动时，先设计迁移与回滚路径。

## 6) 前端规范（Vue + JavaScript）
### 6.1 导入与组织
- 使用 ES Module。
- 导入顺序：第三方依赖在前，本地模块在后。
- 同来源导入合并，避免重复导入。

### 6.2 格式与命名
- 保持现状：双引号、分号、2 空格缩进。
- 变量 / 函数：`camelCase`。
- 常量：`UPPER_SNAKE_CASE`（如 `DEFAULT_POLL_INTERVAL_MS`）。
- 组件文件名：`PascalCase.vue`。
- CSS 类名：`kebab-case`。

### 6.3 状态与异步
- 使用组合式 API：`ref`、`computed`、`watch`、`onMounted`、`onUnmounted`。
- 与后端交互统一使用 `invoke("command")`。
- 异步流程使用 `async/await`，避免层层 `.then()`。

### 6.4 错误处理
- 关键异步流程统一 `try/catch`。
- `catch` 至少包含：
  - `console.error("context", error)`；
  - 用户可见提示（例如更新 `notice`）。
- 禁止空 `catch`，禁止吞错。

## 7) 后端规范（Rust + Tauri）
### 7.1 Command 与返回
- 对外接口使用 `#[tauri::command]`。
- 返回类型优先 `Result<T, String>`。
- 错误文案必须有上下文信息（例如“读取设置失败: {e}”）。

### 7.2 命名与序列化
- Rust 标识符：`snake_case`；类型名：`PascalCase`。
- 前后端字段采用 `camelCase`：优先 `#[serde(rename_all = "camelCase")]`。
- 字段差异使用 `#[serde(rename = "...")]` 显式映射（如 `type` / `imagePath`）。

### 7.3 并发与状态
- 全局状态通过 `State<AppState>` 管理。
- 共享数据使用 `Mutex`，并处理加锁失败分支。
- 不要 `panic!` 代替业务错误返回。

### 7.4 存储与文件
- 任何读写前先确保目录布局存在（`ensure_storage_layout`）。
- JSON 持久化使用 `serde_json::to_string_pretty`。
- 迁移、删除、读写失败均需显式 `Err(String)`。

## 8) 业务约束（按当前实现）
- 历史记录按 `updated_at` 倒序展示。
- 去重键：`item_type + content_hash`。
- 文本入库前标准化：统一换行 + `trim`。
- 配置边界：`poll_interval_ms` 在 300~5000，`history_limit` 在 50~5000。
- 快捷键会清洗（例如 `Meta -> Super`，`CmdOrCtrl -> CommandOrControl`）。

## 9) Cursor / Copilot 规则探测
当前未发现以下文件或目录：
- `.cursor/rules/`
- `.cursorrules`
- `.github/copilot-instructions.md`

若后续新增这些规则：
- 其优先级高于本文件的通用建议。
- 需在本文件补充“冲突处理策略”和生效范围。

## 10) 代理执行清单
开始前：
- 阅读 `README.md` 与 `功能开发文档.md`。
- 确认要执行的脚本/命令在仓库中真实存在。

进行中：
- 仅修改与目标直接相关的文件。
- 协议改动时前后端同步修改并自检字段映射。
- 对风险操作（删除、迁移、覆盖写）先做失败路径处理。

完成后：
- 至少验证一条运行链路：`npm run dev` 或 `npm run tauri dev`。
- 涉及打包链路时补跑：`npm run build` 或 `npm run tauri build`。
- 若新增 lint/test/typecheck 能力，同步更新本 AGENTS.md 的命令与现状章节。
