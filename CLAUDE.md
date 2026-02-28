# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

A clipboard history desktop application built with Tauri 2 + Vue 3. It automatically captures clipboard content (text, images), persists locally, supports filtering, search, favorites, and LAN synchronization via WebSocket.

## Commands

```bash
# Install dependencies
npm install

# Frontend only (dev server without Tauri)
npm run dev
npm run build

# Tauri desktop development
npm run tauri dev

# Build production executable
npm run tauri build
```

**Note**: No lint/test/typecheck scripts configured. Frontend uses JavaScript (not TypeScript). Rust side has no `#[test]` cases.

## Architecture

### Frontend (Vue 3 + Vite)
- Entry: `src/main.js`
- Main component: `src/App.vue`
- Uses `<script setup>` composition API
- Communicates with backend via `invoke("command_name")`

### Backend (Rust + Tauri 2)
- Entry: `src-tauri/src/main.rs`
- Core logic: `src-tauri/src/lib.rs` (Tauri commands, clipboard monitoring, storage)
- WebSocket server: `src-tauri/src/ws_server.rs` (host mode, broadcasts to clients)
- WebSocket client: `src-tauri/src/ws_client.rs` (join mode, receives from host)

### Key Configuration Files
- `src-tauri/tauri.conf.json` - App ID, window config, permissions, build settings
- `src-tauri/Cargo.toml` - Rust dependencies (arboard, tokio, image, sha2, etc.)

### Key Dependencies
- `arboard` - Clipboard access
- `tokio` + `tokio-tungstenite` - Async runtime + WebSocket
- `image` - Image processing
- `sha2` - Content hashing for deduplication

### Data Storage
Default location: Tauri `app_data_dir` (configurable)
- `clipboard-history.json` - History records
- `settings.json` - User settings
- `clipboard-images/` - Saved images

### Window & System Integration
- Frameless window with custom title bar (drag region)
- System tray with menu (show/hide, quit)
- Global shortcut to toggle window visibility
- Default size: 450x500, hidden by default

## Development Notes

- Frontend uses JavaScript (not TypeScript)
- Backend state managed via `State<AppState>` with `Mutex`
- WebSocket server runs on port 9521 by default
- LAN sync broadcasts clipboard content to all connected clients
- Deduplication: text by exact match (normalized), images by SHA256 hash
