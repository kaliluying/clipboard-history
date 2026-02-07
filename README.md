# 剪贴板历史

基于 Tauri 2 + Vue 3 + Vite 的桌面剪贴板历史工具，自动采集剪贴板内容并持久化，支持按类型筛选、收藏与一键回填。

## 功能

### 必做（MVP）

- 自动采集剪贴板内容（文本、图片）
- 历史列表展示与筛选（全部 / 文本 / 图片 / 收藏）
- 点击历史项回填复制
- 收藏 / 取消收藏
- 本地持久化（历史、设置、图片目录）

### 增强（V1）

- 智能去重合并（文本精确、图片哈希）
- 关键字搜索（文本）
- 清空历史
- 全局快捷键唤起窗口
- 托盘常驻与显示 / 隐藏
- 自定义存储目录与数据迁移

## 技术栈

- **前端**：Vue 3（`<script setup>`）、Vite
- **桌面**：Tauri 2
- **后端能力**：Rust Command（剪贴板、存储、托盘、全局快捷键）

## 项目结构

```
clipboard-history/
├── src/                 # Vue 前端
│   ├── App.vue
│   └── main.js
├── src-tauri/           # Tauri Rust 后端
│   ├── src/
│   │   ├── lib.rs       # 命令与业务逻辑
│   │   └── main.rs
│   ├── capabilities/    # 权限配置
│   └── tauri.conf.json
├── 功能开发文档.md      # 功能与实现方案
└── package.json
```

## 数据存储

默认使用 Tauri `app_data_dir`，可切换为用户自定义目录：

- 历史：`clipboard-history.json`
- 设置：`settings.json`
- 图片：`clipboard-images/`

## 开发与构建

```bash
# 安装依赖
npm install

# 开发模式（前端 + Tauri 窗口）
npm run tauri dev

# 构建生产包
npm run tauri build
```

仅启动前端（不启动 Tauri）：`npm run dev`。

## 开发环境

推荐 [VS Code](https://code.visualstudio.com/) 并安装：

- [Vue - Official](https://marketplace.visualstudio.com/items?itemName=Vue.volar)
- [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode)
- [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)

详细设计、接口与分期计划见 `功能开发文档.md`。
