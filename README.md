# 剪贴板历史

基于 Tauri 2 + Vue 3 的桌面剪贴板历史工具，自动采集剪贴板内容并持久化，支持按类型筛选、搜索、收藏与一键回填。

当前版本：`0.2.0`

## 功能

### 核心功能

- 自动采集剪贴板内容（文本、图片）
- 历史列表展示与筛选（全部 / 文本 / 图片 / 收藏）
- 点击历史项回填复制
- 收藏 / 取消收藏
- 本地持久化（历史、设置、图片目录）

### 增强功能

- 关键字搜索（文本）
- 智能去重合并（文本精确匹配、图片哈希）
- 清空历史
- 全局快捷键唤起窗口
- 托盘常驻与显示 / 隐藏
- 自定义存储目录与数据迁移

### 局域网共享

- 作为主机：启动 WebSocket 服务器，其他设备可连接同步
- 加入主机：连接其他设备的剪贴板历史
- **仅同步剪切板内容**，不同步设置
- 支持多设备实时同步文本和图片

## 界面预览

主要界面包括：
- 剪贴板历史列表（支持滚动加载）
- 设置面板（快捷键、存储、局域网共享等）

## 设置说明

### 全局快捷键
- 点击输入框进入录制状态，按下组合键后自动录入并保存
- 录制状态下点击输入框外区域，自动取消本次录制

### 存储目录
- 点击目录输入框选择路径
- 点击"打开目录"可直接打开当前目录

### 局域网共享

**作为主机：**
1. 模式选择"作为主机"
2. 如有多个局域网 IP，可选择绑定 IP
3. 点击启动，生成连接地址
4. 其他设备使用该地址连接

**加入主机：**
1. 模式选择"加入主机"
2. 输入主机地址（如 `ws://192.168.1.100:9521`）
3. 点击连接

**防火墙：**
- 主机需要开放 9521 端口（TCP）
- 可在 Windows 防火墙中添加入站规则

## 技术栈

- **前端**：Vue 3（`<script setup>`）、Vite
- **桌面**：Tauri 2
- **后端**：Rust（剪贴板、存储、托盘、全局快捷键、WebSocket）

## 项目结构

```
clipboard-history/
├── src/                 # Vue 前端
│   ├── App.vue          # 主应用组件
│   └── main.js          # 入口文件
├── src-tauri/           # Tauri Rust 后端
│   ├── src/
│   │   ├── lib.rs       # 命令与业务逻辑
│   │   ├── ws_server.rs # WebSocket 服务器
│   │   ├── ws_client.rs # WebSocket 客户端
│   │   └── main.rs      # 入口文件
│   ├── capabilities/    # 权限配置
│   └── tauri.conf.json  # Tauri 配置
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

仅启动前端（不启动 Tauri）：`npm run dev`

## 开发环境

推荐 [VS Code](https://code.visualstudio.com/) 并安装：

- [Vue - Official](https://marketplace.visualstudio.com/items?itemName=Vue.volar)
- [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode)
- [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer)
