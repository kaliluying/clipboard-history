<script setup>
import { computed, onMounted, onUnmounted, ref } from "vue";
import { invoke } from "@tauri-apps/api/core";

const DEFAULT_POLL_INTERVAL_MS = 800;

const page = ref("history");
const history = ref([]);
const filter = ref("all");
const keyword = ref("");
const isPolling = ref(false);
const pollIntervalMs = ref(DEFAULT_POLL_INTERVAL_MS);
const notice = ref("");
const shortcut = ref("Alt+Shift+V");
const shortcutDraft = ref("Alt+Shift+V");
const isRecordingShortcut = ref(false);

let timer = null;

const visibleHistory = computed(() => {
  const q = keyword.value.trim().toLowerCase();

  return history.value.filter((item) => {
    if (filter.value === "favorite") {
      if (!item.isFavorite) return false;
    } else if (filter.value !== "all" && item.type !== filter.value) {
      return false;
    }

    if (!q) return true;
    if (item.type === "text") return (item.text || "").toLowerCase().includes(q);
    return false;
  });
});

function upsertTop(item) {
  const idx = history.value.findIndex((it) => it.id === item.id);
  if (idx >= 0) history.value.splice(idx, 1);
  history.value.unshift(item);
}

function formatTime(ms) {
  return new Date(ms).toLocaleString();
}

function shortText(text) {
  const source = (text || "").replace(/\s+/g, " ").trim();
  if (source.length <= 120) return source;
  return `${source.slice(0, 120)}...`;
}

function keyToAccelerator(event) {
  const parts = [];
  if (event.ctrlKey) parts.push("Ctrl");
  if (event.altKey) parts.push("Alt");
  if (event.shiftKey) parts.push("Shift");
  if (event.metaKey) parts.push("Super");

  const ignored = ["Control", "Shift", "Alt", "Meta"];
  if (ignored.includes(event.key)) {
    return "";
  }

  let key = event.key;
  if (key.length === 1) key = key.toUpperCase();
  if (key === " ") key = "Space";
  parts.push(key);
  return parts.join("+");
}

function startRecordShortcut() {
  isRecordingShortcut.value = true;
  notice.value = "请按下新的快捷键组合";
}

function onShortcutKeydown(event) {
  if (!isRecordingShortcut.value) return;
  event.preventDefault();
  const accelerator = keyToAccelerator(event);
  if (!accelerator) return;
  shortcutDraft.value = accelerator;
  isRecordingShortcut.value = false;
  notice.value = `已录入：${accelerator}`;
}

async function loadSettings() {
  const settings = await invoke("get_settings");
  if (settings && typeof settings.pollIntervalMs === "number") {
    pollIntervalMs.value = Math.max(300, Math.min(5000, settings.pollIntervalMs));
  }
  if (settings && typeof settings.globalShortcut === "string" && settings.globalShortcut.trim()) {
    shortcut.value = settings.globalShortcut.trim();
    shortcutDraft.value = shortcut.value;
  }
}

async function loadHistory() {
  const data = await invoke("get_history");
  history.value = Array.isArray(data) ? data : [];
}

async function pollClipboard() {
  if (isPolling.value) return;
  isPolling.value = true;

  try {
    const item = await invoke("poll_clipboard");
    if (item) {
      upsertTop(item);
      notice.value = "已采集新内容";
    }
  } catch (error) {
    console.error("poll_clipboard failed", error);
    notice.value = "采集失败";
  } finally {
    isPolling.value = false;
  }
}

async function copyItem(item) {
  try {
    await invoke("copy_history_item", { id: item.id });
    notice.value = "已回填到剪贴板";
  } catch (error) {
    console.error("copy_history_item failed", error);
    notice.value = "回填失败";
  }
}

async function toggleFavorite(item) {
  try {
    const updated = await invoke("toggle_favorite", { id: item.id });
    if (!updated) return;
    const idx = history.value.findIndex((it) => it.id === item.id);
    if (idx >= 0) history.value.splice(idx, 1, updated);
    history.value.sort((a, b) => b.updatedAt - a.updatedAt);
    notice.value = updated.isFavorite ? "已加入收藏" : "已取消收藏";
  } catch (error) {
    console.error("toggle_favorite failed", error);
    notice.value = "收藏操作失败";
  }
}

async function clearAllHistory() {
  try {
    await invoke("clear_history");
    history.value = [];
    notice.value = "历史已清空";
  } catch (error) {
    console.error("clear_history failed", error);
    notice.value = "清空失败";
  }
}

async function saveSettings() {
  const interval = Number(pollIntervalMs.value);
  if (!Number.isFinite(interval)) {
    notice.value = "轮询间隔格式错误";
    return;
  }

  const newShortcut = shortcutDraft.value.trim();
  if (!newShortcut) {
    notice.value = "快捷键不能为空";
    return;
  }

  try {
    const settings = await invoke("update_settings", {
      payload: {
        pollIntervalMs: Math.max(300, Math.min(5000, interval)),
        globalShortcut: newShortcut,
      },
    });

    pollIntervalMs.value = settings.pollIntervalMs;
    shortcut.value = settings.globalShortcut;
    shortcutDraft.value = settings.globalShortcut;

    if (timer !== null) {
      window.clearInterval(timer);
    }
    timer = window.setInterval(() => {
      void pollClipboard();
    }, pollIntervalMs.value);

    notice.value = "设置已保存";
  } catch (error) {
    console.error("save settings failed", error);
    notice.value = "设置保存失败";
  }
}

onMounted(async () => {
  try {
    await loadSettings();
    await loadHistory();
    await pollClipboard();
  } catch (error) {
    console.error("initialization failed", error);
    notice.value = "初始化失败";
  }

  timer = window.setInterval(() => {
    void pollClipboard();
  }, pollIntervalMs.value);
});

onUnmounted(async () => {
  if (timer !== null) {
    window.clearInterval(timer);
  }
});
</script>

<template>
  <main class="app-shell">
    <div class="ambient ambient-1" aria-hidden="true"></div>
    <div class="ambient ambient-2" aria-hidden="true"></div>

    <section class="panel controls">
      <div class="page-tabs">
        <button :class="['chip', { active: page === 'history' }]" @click="page = 'history'">历史</button>
        <button :class="['chip', { active: page === 'settings' }]" @click="page = 'settings'">设置</button>
      </div>

      <template v-if="page === 'history'">
        <div class="filters">
          <button :class="['chip', { active: filter === 'all' }]" @click="filter = 'all'">全部</button>
          <button :class="['chip', { active: filter === 'text' }]" @click="filter = 'text'">文本</button>
          <button :class="['chip', { active: filter === 'image' }]" @click="filter = 'image'">图片</button>
          <button :class="['chip', { active: filter === 'favorite' }]" @click="filter = 'favorite'">收藏</button>
        </div>

        <div class="actions-row">
          <input v-model="keyword" class="search" placeholder="搜索文本内容" />
          <button class="chip danger" @click="clearAllHistory">清空历史</button>
        </div>
      </template>

      <template v-else>
        <div class="setting-row">
          <label>全局快捷键</label>
          <div class="setting-actions">
            <input
              class="search"
              :value="shortcutDraft"
              readonly
              @keydown="onShortcutKeydown"
              @focus="startRecordShortcut"
              placeholder="点击后按下组合键"
            />
            <button class="chip" @click="startRecordShortcut">
              {{ isRecordingShortcut ? "录制中..." : "录入快捷键" }}
            </button>
          </div>
        </div>

        <div class="setting-row">
          <label>轮询间隔(ms)</label>
          <input v-model.number="pollIntervalMs" class="search" type="number" min="300" max="5000" />
        </div>

        <button class="chip save-btn" @click="saveSettings">保存设置</button>
      </template>

      <p v-if="notice" class="notice">{{ notice }}</p>
    </section>

    <section v-if="page === 'history'" class="history-list">
      <article
        v-for="item in visibleHistory"
        :key="item.id"
        class="panel history-item"
        @click="copyItem(item)"
      >
        <header>
          <span class="tag" :class="item.type">{{ item.type === "text" ? "文本" : "图片" }}</span>
          <time>{{ formatTime(item.updatedAt) }}</time>
        </header>

        <p v-if="item.type === 'text'" class="text-preview">{{ shortText(item.text) }}</p>

        <div v-else class="image-preview-wrap">
          <img :src="item.imagePreviewDataUrl" alt="clipboard image" class="image-preview" />
        </div>

        <footer>点击回填到系统剪贴板</footer>
        <button
          class="favorite-toggle"
          :class="{ active: item.isFavorite }"
          @click.stop="toggleFavorite(item)"
        >
          {{ item.isFavorite ? "已收藏" : "收藏" }}
        </button>
      </article>

      <article v-if="visibleHistory.length === 0" class="panel empty">
        <p>当前没有可展示的历史项，复制任意文本或图片后会自动出现。</p>
      </article>
    </section>
  </main>
</template>

<style>
:root {
  --bg-top: #0f172a;
  --bg-bottom: #1e293b;
  --glass: rgba(255, 255, 255, 0.12);
  --glass-border: rgba(255, 255, 255, 0.25);
  --text-main: #eff6ff;
  --text-soft: #cbd5e1;
  --accent: #22d3ee;
  --accent-strong: #0ea5e9;
}

* {
  box-sizing: border-box;
}

body {
  margin: 0;
  font-family: "Segoe UI", "PingFang SC", "Microsoft YaHei", sans-serif;
  color: var(--text-main);
  background: linear-gradient(145deg, var(--bg-top), var(--bg-bottom));
  min-height: 100vh;
}

.app-shell {
  position: relative;
  min-height: 100vh;
  padding: 28px 20px 36px;
  max-width: 980px;
  margin: 0 auto;
}

.ambient {
  position: fixed;
  border-radius: 999px;
  filter: blur(46px);
  opacity: 0.22;
  pointer-events: none;
}

.ambient-1 {
  width: 300px;
  height: 300px;
  background: #22d3ee;
  top: -80px;
  right: -100px;
}

.ambient-2 {
  width: 280px;
  height: 280px;
  background: #38bdf8;
  bottom: -100px;
  left: -80px;
}

.panel {
  position: relative;
  background: var(--glass);
  border: 1px solid var(--glass-border);
  backdrop-filter: blur(18px);
  -webkit-backdrop-filter: blur(18px);
  border-radius: 18px;
  box-shadow: 0 10px 35px rgba(2, 6, 23, 0.35);
}

.controls {
  padding: 14px;
  display: grid;
  gap: 12px;
}

.page-tabs,
.filters,
.actions-row,
.setting-actions {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
  align-items: center;
}

.setting-row {
  display: grid;
  gap: 6px;
}

.setting-row label {
  font-size: 12px;
  color: #cbd5e1;
}

.chip {
  border: 1px solid rgba(255, 255, 255, 0.25);
  background: rgba(15, 23, 42, 0.28);
  color: var(--text-main);
  border-radius: 999px;
  padding: 8px 14px;
  cursor: pointer;
}

.chip.active {
  background: linear-gradient(120deg, var(--accent), var(--accent-strong));
  color: #0b1020;
  border-color: transparent;
  font-weight: 600;
}

.chip.danger {
  background: rgba(190, 24, 93, 0.25);
  border-color: rgba(244, 114, 182, 0.45);
  color: #fbcfe8;
  white-space: nowrap;
}

.save-btn {
  justify-self: start;
}

.search {
  width: 100%;
  border: 1px solid rgba(255, 255, 255, 0.3);
  background: rgba(15, 23, 42, 0.35);
  border-radius: 12px;
  color: var(--text-main);
  padding: 10px 12px;
}

.search::placeholder {
  color: #94a3b8;
}

.notice {
  margin: 0;
  font-size: 12px;
  color: #cbd5e1;
}

.history-list {
  margin-top: 14px;
  display: grid;
  gap: 12px;
}

.history-item {
  padding: 14px;
  cursor: pointer;
  transition: transform 0.12s ease, border-color 0.12s ease;
  background: rgba(15, 23, 42, 0.35);
  backdrop-filter: none;
  -webkit-backdrop-filter: none;
  box-shadow: 0 6px 18px rgba(2, 6, 23, 0.2);
}

.history-item:hover {
  transform: translateY(-1px);
  border-color: rgba(34, 211, 238, 0.45);
}

.history-item header {
  display: flex;
  justify-content: space-between;
  align-items: center;
  margin-bottom: 10px;
}

.tag {
  display: inline-flex;
  border-radius: 999px;
  font-size: 12px;
  padding: 4px 10px;
  font-weight: 600;
}

.tag.text {
  background: rgba(14, 165, 233, 0.2);
  color: #7dd3fc;
}

.tag.image {
  background: rgba(16, 185, 129, 0.2);
  color: #6ee7b7;
}

time {
  font-size: 12px;
  color: #94a3b8;
}

.text-preview {
  margin: 0;
  line-height: 1.5;
  color: #e2e8f0;
}

.image-preview-wrap {
  width: 100%;
  overflow: hidden;
  border-radius: 10px;
  border: 1px solid rgba(255, 255, 255, 0.2);
}

.image-preview {
  display: block;
  max-width: 100%;
  max-height: 240px;
  object-fit: contain;
  margin: 0 auto;
  background: rgba(15, 23, 42, 0.45);
}

.history-item footer {
  margin-top: 10px;
  font-size: 12px;
  color: #94a3b8;
}

.favorite-toggle {
  margin-top: 10px;
  border: 1px solid rgba(255, 255, 255, 0.28);
  border-radius: 999px;
  background: rgba(15, 23, 42, 0.3);
  color: #e2e8f0;
  padding: 6px 12px;
  cursor: pointer;
}

.favorite-toggle.active {
  background: rgba(251, 191, 36, 0.2);
  border-color: rgba(251, 191, 36, 0.5);
  color: #fde68a;
}

.empty {
  padding: 20px;
  text-align: center;
  color: var(--text-soft);
}

@media (max-width: 640px) {
  .app-shell {
    padding: 16px 12px 28px;
  }

  .actions-row,
  .setting-actions {
    flex-direction: column;
  }
}
</style>
