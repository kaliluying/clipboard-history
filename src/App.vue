<script setup>
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";

const DEFAULT_POLL_INTERVAL_MS = 800;

const page = ref("history");
const history = ref([]);
const filter = ref("all");
const keyword = ref("");
const isPolling = ref(false);
const isDraggingWindow = ref(false);
const dragPollCooldownUntil = ref(0);
const userInteractingUntil = ref(0);
const pollIntervalMs = ref(DEFAULT_POLL_INTERVAL_MS);
const notice = ref("");
const shortcut = ref("Alt+Shift+V");
const shortcutDraft = ref("Alt+Shift+V");
const isRecordingShortcut = ref(false);
const launchAtStartup = ref(false);
const alwaysOnTop = ref(false);
const storageDir = ref("");
const deviceName = ref("");
const historyLimit = ref(300);
const textRetentionDays = ref(30);
const imageRetentionDays = ref(7);
const maxStorageMb = ref(500);
const storageStats = ref({ historyCount: 0, textCount: 0, imageCount: 0, favoriteCount: 0, imageStorageBytes: 0 });
const imagePreviewMap = ref({});
const previewLoadingMap = ref({});
const expandedTextItem = ref(null);
const copiedItemId = ref("");
const copyBubble = ref({ visible: false, x: 0, y: 0, key: 0 });
const isClearHistoryConfirming = ref(false);
const appWindow = getCurrentWindow();

let timer = null;
let saveSettingsTimer = null;
let copiedItemTimer = null;
let copyBubbleTimer = null;
let clearHistoryConfirmTimer = null;
let isHydratingSettings = true;
let unlistenClipboardSynced = null;
let unlistenWsStatusChanged = null;
let unlistenWsReconnectNeeded = null;

// WebSocket 局域网共享
const wsMode = ref("disabled"); // "disabled" | "server" | "client"
const wsPort = ref(9521);
const wsUrl = ref("");
const wsRunning = ref(false);
const wsPeerCount = ref(0);
const wsAddress = ref("");
const wsLocalIps = ref([]);
const wsSelectedIp = ref("");
const wsLoading = ref(false);
const wsAutoReconnect = ref(true);

async function loadLocalIps() {
  try {
    wsLocalIps.value = await invoke("ws_get_local_ips");
    if (wsLocalIps.value.length > 0) {
      // 设置默认选中第一个 IP
      if (!wsSelectedIp.value) {
        wsSelectedIp.value = wsLocalIps.value[0];
      }
      if (!wsUrl.value) {
        wsUrl.value = `ws://${wsSelectedIp.value}:${wsPort.value}`;
      }
    }
  } catch (e) {
    console.error("ws_get_local_ips failed", e);
  }
}

// 监听选中的 IP 变化，更新客户端连接地址
watch(wsSelectedIp, (newIp) => {
  if (newIp) {
    if (wsMode.value === "client") {
      wsUrl.value = `ws://${newIp}:${wsPort.value}`;
    } else if (wsMode.value === "server" && wsRunning.value) {
      // 服务器运行时更新显示的地址
      wsAddress.value = `ws://${newIp}:${wsPort.value}`;
    }
  }
});

// 监听模式变化，确保 IP 选择正确
watch(wsMode, (newMode) => {
  if (newMode === "server" && wsSelectedIp.value && wsRunning.value) {
    // 切换回服务器模式时，确保显示正确的地址
    wsAddress.value = `ws://${wsSelectedIp.value}:${wsPort.value}`;
  }
});

async function loadWsStatus() {
  try {
    const status = await invoke("ws_get_status");
    wsMode.value = status.mode;
    wsRunning.value = status.running;
    wsPeerCount.value = status.peerCount;
    wsAddress.value = status.address || "";
    // 如果是客户端模式且有地址，同步到 wsUrl
    if (status.mode === "client" && status.address) {
      wsUrl.value = status.address;
    }
  } catch (e) {
    console.error("ws_get_status failed", e);
  }
}

async function wsStartServer() {
  wsLoading.value = true;
  try {
    const status = await invoke("ws_start_server", { port: wsPort.value });
    wsRunning.value = status.running;
    // 如果用户选择了 IP，使用用户选择的 IP；否则使用后端返回的地址
    if (wsSelectedIp.value) {
      wsAddress.value = `ws://${wsSelectedIp.value}:${wsPort.value}`;
    } else {
      wsAddress.value = status.address || "";
    }
    notice.value = `已启动，其它设备连接地址：${wsAddress.value}`;
  } catch (e) {
    notice.value = `启动失败：${e}`;
  } finally {
    wsLoading.value = false;
  }
}

async function wsStopServer() {
  wsLoading.value = true;
  try {
    await invoke("ws_stop_server");
    wsRunning.value = false;
    wsAddress.value = "";
    notice.value = "已停止 WebSocket 服务";
  } catch (e) {
    notice.value = `停止失败：${e}`;
  } finally {
    wsLoading.value = false;
  }
}

async function wsConnectClient() {
  if (!wsUrl.value.trim()) {
    notice.value = "请填写服务器地址";
    return;
  }
  wsLoading.value = true;
  try {
    const status = await invoke("ws_connect_client", { url: wsUrl.value.trim() });
    wsRunning.value = status.running;
    notice.value = `已连接到：${wsUrl.value}`;
  } catch (e) {
    notice.value = `连接失败：${e}`;
  } finally {
    wsLoading.value = false;
  }
}

async function wsDisconnectClient() {
  wsLoading.value = true;
  try {
    await invoke("ws_disconnect_client");
    wsRunning.value = false;
    notice.value = "已断开连接";
  } catch (e) {
    notice.value = `断开失败：${e}`;
  } finally {
    wsLoading.value = false;
  }
}

async function wsReconnect() {
  if (!wsUrl.value.trim()) {
    notice.value = "请填写服务器地址";
    return;
  }
  wsLoading.value = true;
  try {
    const status = await invoke("ws_connect_client", { url: wsUrl.value.trim() });
    wsRunning.value = status.running;
    notice.value = `正在重连...`;
  } catch (e) {
    notice.value = `重连失败：${e}`;
  } finally {
    wsLoading.value = false;
  }
}

function showCopyFeedback(itemId, mouseEvent) {
  copiedItemId.value = itemId;
  notice.value = "";

  const x = mouseEvent?.clientX ?? window.innerWidth / 2;
  const y = mouseEvent?.clientY ?? window.innerHeight / 2;
  copyBubble.value = {
    visible: true,
    x,
    y,
    key: copyBubble.value.key + 1,
  };

  if (copyBubbleTimer !== null) {
    window.clearTimeout(copyBubbleTimer);
  }
  copyBubbleTimer = window.setTimeout(() => {
    copyBubble.value.visible = false;
  }, 760);

  if (copiedItemTimer !== null) {
    window.clearTimeout(copiedItemTimer);
  }
  copiedItemTimer = window.setTimeout(() => {
    copiedItemId.value = "";
  }, 260);
}

function onUserInteraction() {
  userInteractingUntil.value = Date.now() + 1000;
}

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

function formatBytes(bytes) {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + " " + sizes[i];
}

function shortText(text) {
  const source = (text || "").replace(/\s+/g, " ").trim();
  if (source.length <= 120) return source;
  return `${source.slice(0, 120)}...`;
}

function isTextTruncated(text) {
  const source = (text || "").replace(/\s+/g, " ").trim();
  return source.length > 120;
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
  if (isRecordingShortcut.value) return;
  isRecordingShortcut.value = true;
  notice.value = "请按下新的快捷键组合";
}

function cancelRecordShortcut() {
  if (!isRecordingShortcut.value) return;
  isRecordingShortcut.value = false;
  notice.value = "已取消快捷键录制";
}

function onShortcutInputBlur() {
  if (!isRecordingShortcut.value) return;
  cancelRecordShortcut();
}

function onShortcutKeydown(event) {
  if (!isRecordingShortcut.value) return;
  if (event.key === "Escape") {
    event.preventDefault();
    cancelRecordShortcut();
    return;
  }
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
  if (settings && typeof settings.launchAtStartup === "boolean") {
    launchAtStartup.value = settings.launchAtStartup;
  }
  if (settings && typeof settings.alwaysOnTop === "boolean") {
    alwaysOnTop.value = settings.alwaysOnTop;
  }
  if (settings && typeof settings.storageDir === "string") {
    storageDir.value = settings.storageDir;
  }
  if (settings && typeof settings.deviceName === "string") {
    deviceName.value = settings.deviceName;
  }
  if (settings && typeof settings.historyLimit === "number") {
    historyLimit.value = settings.historyLimit;
  }
  if (settings && typeof settings.textRetentionDays === "number") {
    textRetentionDays.value = settings.textRetentionDays;
  }
  if (settings && typeof settings.imageRetentionDays === "number") {
    imageRetentionDays.value = settings.imageRetentionDays;
  }
  if (settings && typeof settings.maxStorageMb === "number") {
    maxStorageMb.value = settings.maxStorageMb;
  }

  // Load storage stats
  try {
    storageStats.value = await invoke("get_storage_stats");
  } catch (e) {
    console.error("load storage stats failed", e);
  }
}

async function selectStorageDir() {
  try {
    const selected = await open({
      directory: true,
      multiple: false,
      title: "选择存储目录",
    });

    if (!selected || Array.isArray(selected)) {
      return;
    }

    storageDir.value = selected;
    notice.value = "";
  } catch (error) {
    console.error("select storage directory failed", error);
    notice.value = "选择存储目录失败";
  }
}

async function openStorageDir() {
  try {
    await invoke("open_storage_dir");
    notice.value = "";
  } catch (error) {
    console.error("open storage directory failed", error);
    notice.value = "打开目录失败";
  }
}

async function hideWindow() {
  try {
    await appWindow.hide();
  } catch (error) {
    console.error("hide window failed", error);
    notice.value = "窗口隐藏失败，请检查权限配置";
  }
}

async function startWindowDrag(event) {
  if (event.button !== 0) return;
  const target = event.target;
  if (!(target instanceof Element)) return;

  const nonDragSelector = [
    ".no-drag",
    "button",
    "input",
    "textarea",
    "select",
    "option",
    "label",
    "a",
    "[role='button']",
    "[contenteditable='true']",
    ".history-item",
  ].join(",");

  if (target.closest(nonDragSelector)) {
    return;
  }
  isDraggingWindow.value = true;

  try {
    await invoke("suppress_auto_hide");
  } catch (error) {
    console.error("suppress auto hide failed", error);
  }

  try {
    await appWindow.startDragging();
  } catch (error) {
    console.error("start dragging failed", error);
    notice.value = "窗口拖动失败，请检查权限配置";
  } finally {
    isDraggingWindow.value = false;
    dragPollCooldownUntil.value = Date.now() + 350;
  }
}

async function loadHistory() {
  const data = await invoke("get_history");
  history.value = Array.isArray(data) ? data : [];
}

async function ensureImagePreview(item) {
  if (!item || item.type !== "image") return;
  if (imagePreviewMap.value[item.id]) return;
  if (previewLoadingMap.value[item.id]) return;

  previewLoadingMap.value[item.id] = true;
  try {
    const preview = await invoke("get_image_preview", { id: item.id });
    if (typeof preview === "string" && preview) {
      imagePreviewMap.value[item.id] = preview;
    }
  } catch (error) {
    console.error("get_image_preview failed", error);
  } finally {
    previewLoadingMap.value[item.id] = false;
  }
}

async function pollClipboard() {
  if (isDraggingWindow.value) return;
  if (Date.now() < dragPollCooldownUntil.value) return;
  if (Date.now() < userInteractingUntil.value) return;
  if (isPolling.value) return;
  isPolling.value = true;

  try {
    const item = await invoke("poll_clipboard");
    if (item) {
      upsertTop(item);
      notice.value = "";
    }
  } catch (error) {
    console.error("poll_clipboard failed", error);
    notice.value = "采集失败";
  } finally {
    isPolling.value = false;
  }
}

function getSelectedTextWithin(element) {
  const selection = window.getSelection();
  if (!selection || selection.isCollapsed) return "";

  const selected = selection.toString().trim();
  if (!selected) return "";

  const anchorNode = selection.anchorNode;
  const focusNode = selection.focusNode;
  if (!anchorNode || !focusNode) return "";
  if (!element.contains(anchorNode) || !element.contains(focusNode)) return "";

  return selected;
}

async function copyItem(item, event) {
  try {
    if (item.type === "text" && event?.currentTarget instanceof Element) {
      const selectedText = getSelectedTextWithin(event.currentTarget);
      if (selectedText) {
        await invoke("copy_text", { text: selectedText });
        showCopyFeedback(item.id, event);
        return;
      }
    }

    await invoke("copy_history_item", { id: item.id });
    showCopyFeedback(item.id, event);
  } catch (error) {
    console.error("copy_history_item failed", error);
    notice.value = "回填失败";
  }
}

function openTextPreview(item) {
  if (!item || item.type !== "text") return;
  expandedTextItem.value = item;
}

function closeTextPreview() {
  expandedTextItem.value = null;
}

async function copyExpandedText() {
  const text = expandedTextItem.value?.text || "";
  if (!text) return;
  try {
    await invoke("copy_text", { text });
    notice.value = "";
  } catch (error) {
    console.error("copy expanded text failed", error);
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
    notice.value = "";
  } catch (error) {
    console.error("toggle_favorite failed", error);
    notice.value = "收藏操作失败";
  }
}

async function deleteItem(item) {
  try {
    await invoke("delete_history_item", { id: item.id });
    history.value = history.value.filter((it) => it.id !== item.id);
    delete imagePreviewMap.value[item.id];
    delete previewLoadingMap.value[item.id];
    notice.value = "";
  } catch (error) {
    console.error("delete_history_item failed", error);
    notice.value = "删除失败";
  }
}

async function clearAllHistory() {
  if (clearHistoryConfirmTimer === null) {
    notice.value = "危险操作：请再次点击红色按钮确认删除全部历史";
    isClearHistoryConfirming.value = true;
    clearHistoryConfirmTimer = window.setTimeout(() => {
      isClearHistoryConfirming.value = false;
      clearHistoryConfirmTimer = null;
    }, 2500);
    return;
  }

  window.clearTimeout(clearHistoryConfirmTimer);
  clearHistoryConfirmTimer = null;
  isClearHistoryConfirming.value = false;

  try {
    await invoke("clear_history");
    history.value = [];
    imagePreviewMap.value = {};
    previewLoadingMap.value = {};
    notice.value = "已删除全部历史";
  } catch (error) {
    console.error("clear_history failed", error);
    notice.value = "清空失败";
  }
}

async function saveSettings() {
  const interval = Number(pollIntervalMs.value);
  if (!Number.isFinite(interval)) {
    return;
  }

  const newShortcut = shortcutDraft.value.trim();
  if (!newShortcut) {
    return;
  }

  const previousShortcut = shortcut.value;

  try {
    const settings = await invoke("update_settings", {
      payload: {
        pollIntervalMs: Math.max(300, Math.min(5000, interval)),
        globalShortcut: newShortcut,
        launchAtStartup: launchAtStartup.value,
        alwaysOnTop: alwaysOnTop.value,
        storageDir: storageDir.value.trim(),
        deviceName: deviceName.value.trim(),
        historyLimit: Number(historyLimit.value),
        textRetentionDays: Number(textRetentionDays.value),
        imageRetentionDays: Number(imageRetentionDays.value),
        maxStorageMb: Number(maxStorageMb.value),
      },
    });

    pollIntervalMs.value = settings.pollIntervalMs;
    shortcut.value = settings.globalShortcut;
    shortcutDraft.value = settings.globalShortcut;
    launchAtStartup.value = settings.launchAtStartup;
    alwaysOnTop.value = settings.alwaysOnTop;
    storageDir.value = settings.storageDir || "";
    deviceName.value = settings.deviceName || "";
    historyLimit.value = settings.historyLimit || 300;
    textRetentionDays.value = settings.textRetentionDays || 0;
    imageRetentionDays.value = settings.imageRetentionDays || 0;
    maxStorageMb.value = settings.maxStorageMb || 0;

    if (timer !== null) {
      window.clearInterval(timer);
    }
    timer = window.setInterval(() => {
      void pollClipboard();
    }, pollIntervalMs.value);

    if (settings.globalShortcut !== previousShortcut) {
      notice.value = `已保存快捷键：${settings.globalShortcut}`;
    } else {
      notice.value = "";
    }

    // Reload storage stats after save
    try {
      storageStats.value = await invoke("get_storage_stats");
    } catch (e) {
      console.error("reload storage stats failed", e);
    }
  } catch (error) {
    console.error("save settings failed", error);
    notice.value = "设置保存失败";
  }
}

function scheduleAutoSaveSettings() {
  if (isHydratingSettings) return;
  if (saveSettingsTimer !== null) {
    window.clearTimeout(saveSettingsTimer);
  }
  saveSettingsTimer = window.setTimeout(() => {
    void saveSettings();
  }, 300);
}

onMounted(async () => {
  try {
    await loadSettings();
    await loadHistory();
    await pollClipboard();
  } catch (error) {
    console.error("initialization failed", error);
    notice.value = "初始化失败";
  } finally {
    isHydratingSettings = false;
  }

  timer = window.setInterval(() => {
    void pollClipboard();
  }, pollIntervalMs.value);

  // 监听来自其它设备的剪切板事件
  unlistenClipboardSynced = await listen("clipboard-synced", async () => {
    await loadHistory();
  });

  // 监听 WebSocket 连接状态变化
  unlistenWsStatusChanged = await listen("ws-status-changed", async (event) => {
    const data = event.payload;
    if (data.mode === "client") {
      wsRunning.value = data.connected;
      if (data.connected) {
        notice.value = "已重新连接到服务器";
      } else {
        notice.value = "连接已断开";
      }
    }
  });

  // 监听需要重连的事件
  unlistenWsReconnectNeeded = await listen("ws-reconnect-needed", async () => {
    if (wsAutoReconnect.value && wsUrl.value) {
      notice.value = "连接断开，正在尝试重连...";
      // 自动重连
      setTimeout(async () => {
        await wsReconnect();
      }, 2000);
    }
  });

  // 加载本机 IP 和 WebSocket 状态
  await loadLocalIps();
  await loadWsStatus();
});

watch(
  visibleHistory,
  (items) => {
    for (const item of items) {
      if (item.type === "image") {
        void ensureImagePreview(item);
      }
    }
  },
  { immediate: true }
);

// 自动保存（不触发清理）
watch([pollIntervalMs, shortcutDraft, launchAtStartup, alwaysOnTop, storageDir, deviceName], () => {
  scheduleAutoSaveSettings();
});

onUnmounted(() => {
  if (timer !== null) {
    window.clearInterval(timer);
  }
  if (saveSettingsTimer !== null) {
    window.clearTimeout(saveSettingsTimer);
  }
  if (copyNoticeTimer !== null) {
    window.clearTimeout(copyNoticeTimer);
  }
  if (copiedItemTimer !== null) {
    window.clearTimeout(copiedItemTimer);
  }
  if (copyBubbleTimer !== null) {
    window.clearTimeout(copyBubbleTimer);
  }
  if (clearHistoryConfirmTimer !== null) {
    window.clearTimeout(clearHistoryConfirmTimer);
  }
  if (unlistenClipboardSynced) {
    unlistenClipboardSynced();
  }
  if (unlistenWsStatusChanged) {
    unlistenWsStatusChanged();
  }
  if (unlistenWsReconnectNeeded) {
    unlistenWsReconnectNeeded();
  }
});
</script>

<template>
  <main class="app-shell" @mousemove="onUserInteraction" @scroll.capture="onUserInteraction">
    <div class="ambient ambient-1" aria-hidden="true"></div>
    <div class="ambient ambient-2" aria-hidden="true"></div>
    <div
      v-if="copyBubble.visible"
      :key="copyBubble.key"
      class="copy-bubble"
      :style="{ left: `${copyBubble.x}px`, top: `${copyBubble.y}px` }"
    >
      已复制
    </div>

    <header class="titlebar panel" @mousedown="startWindowDrag">
      <div class="titlebar-name">Clipboard History</div>
      <button class="titlebar-btn no-drag" @mousedown.stop @click.stop="hideWindow">×</button>
    </header>

    <section class="panel controls" @mousedown="startWindowDrag">
      <template v-if="page === 'history'">
        <div class="filters">
          <button :class="['chip', { active: filter === 'all' }]" @click="filter = 'all'">全部</button>
          <button :class="['chip', { active: filter === 'text' }]" @click="filter = 'text'">文本</button>
          <button :class="['chip', { active: filter === 'image' }]" @click="filter = 'image'">图片</button>
          <button :class="['chip', { active: filter === 'favorite' }]" @click="filter = 'favorite'">收藏</button>
          <button class="chip settings-entry" @click="page = 'settings'">设置</button>
        </div>

        <div class="actions-row">
          <input v-model="keyword" class="search" placeholder="搜索文本内容" />
        </div>
      </template>

      <template v-else>
        <div class="settings-compact">
          <div class="setting-actions top-setting-actions">
            <button class="chip" @click="page = 'history'">返回历史</button>
          </div>

          <div class="setting-row setting-inline">
            <label>全局快捷键</label>
            <div class="setting-actions">
              <input
                class="search compact-input"
                :value="shortcutDraft"
                readonly
                @keydown="onShortcutKeydown"
                @click="startRecordShortcut"
                @blur="onShortcutInputBlur"
                placeholder="点击输入框后按下组合键"
              />
            </div>
          </div>

          <div class="setting-row setting-inline">
            <label>轮询间隔(ms)</label>
            <input v-model.number="pollIntervalMs" class="search compact-input" type="number" min="300" max="5000" />
          </div>

          <!-- 历史记录限制 -->
          <div class="setting-section">
            <h3 class="setting-section-title">📊 历史记录限制</h3>

            <div class="storage-stats-row">
              <span class="storage-stat">总计: {{ storageStats.historyCount || 0 }}</span>
              <span class="storage-stat">文本: {{ storageStats.textCount || 0 }}</span>
              <span class="storage-stat">图片: {{ storageStats.imageCount || 0 }}</span>
              <span class="storage-stat">收藏: {{ storageStats.favoriteCount || 0 }}</span>
              <span class="storage-stat">图片占用: {{ formatBytes(storageStats.imageStorageBytes || 0) }}</span>
            </div>

            <div class="setting-row setting-inline">
              <label>历史记录数量</label>
              <input v-model.number="historyLimit" class="search compact-input" type="number" min="50" max="5000" style="width:80px" />
            </div>

            <div class="setting-row setting-inline">
              <label>文本保留天数</label>
              <input v-model.number="textRetentionDays" class="search compact-input" type="number" min="0" max="365" style="width:80px" />
              <span class="setting-hint">0=不限制</span>
            </div>

            <div class="setting-row setting-inline">
              <label>图片保留天数</label>
              <input v-model.number="imageRetentionDays" class="search compact-input" type="number" min="0" max="365" style="width:80px" />
              <span class="setting-hint">0=不限制</span>
            </div>

            <div class="setting-row setting-inline">
              <label>存储空间上限(MB)</label>
              <input v-model.number="maxStorageMb" class="search compact-input" type="number" min="0" max="10000" style="width:80px" />
              <span class="setting-hint">0=不限制</span>
            </div>

            <div class="setting-actions">
              <button class="chip" @click="saveSettings">保存并清理</button>
            </div>
          </div>

          <div class="setting-pair">
            <div class="setting-row setting-inline">
              <label>开机自启</label>
              <label class="switch-row">
                <input v-model="launchAtStartup" type="checkbox" />
                <span>{{ launchAtStartup ? "已启用" : "未启用" }}</span>
              </label>
            </div>

            <div class="setting-row setting-inline">
              <label>窗口置顶</label>
              <label class="switch-row">
                <input v-model="alwaysOnTop" type="checkbox" />
                <span>{{ alwaysOnTop ? "已启用" : "未启用" }}</span>
              </label>
            </div>
          </div>

          <div class="setting-row">
            <label>存储目录</label>
            <div class="storage-dir-row">
              <div class="storage-dir-display" :title="storageDir || '默认应用数据目录'">
                <input
                  class="search compact-input storage-dir-input"
                  :value="storageDir || '默认应用数据目录'"
                  readonly
                  @click="selectStorageDir"
                  title="点击选择保存位置"
                />
              </div>
              <div class="setting-actions storage-dir-actions">
                <button class="chip" @click="openStorageDir">打开目录</button>
              </div>
            </div>
          </div>

          <div class="setting-row setting-inline">
            <label>设备名称</label>
            <input
              v-model="deviceName"
              class="search compact-input"
              placeholder="用于局域网识别"
              style="min-width:120px"
            />
          </div>

          <div class="setting-actions bottom-setting-actions">
            <button class="chip danger" :class="{ 'danger-confirm': isClearHistoryConfirming }" @click="clearAllHistory">
              {{ isClearHistoryConfirming ? "再次点击确认删除" : "删除全部历史" }}
            </button>
          </div>

          <!-- 局域网剪切板共享 -->
          <div class="setting-section">
            <h3 class="setting-section-title">📶 局域网共享</h3>

            <div class="setting-row setting-inline">
              <label>模式</label>
              <div class="ws-mode-btns">
                <button :class="['chip', { active: wsMode === 'disabled' }]" @click="wsMode = 'disabled'">禁用</button>
                <button :class="['chip', { active: wsMode === 'server' }]" @click="wsMode = 'server'">作为主机</button>
                <button :class="['chip', { active: wsMode === 'client' }]" @click="wsMode = 'client'">加入主机</button>
              </div>
            </div>

            <template v-if="wsMode === 'server'">
              <div class="setting-row setting-inline">
                <label>端口</label>
                <input v-model.number="wsPort" class="search compact-input" type="number" min="1024" max="65535" style="width:90px" />
              </div>
              <div v-if="wsLocalIps.length > 1" class="setting-row setting-inline">
                <label>绑定IP</label>
                <select v-model="wsSelectedIp" class="search compact-input" style="min-width:150px">
                  <option v-for="ip in wsLocalIps" :key="ip" :value="ip">{{ ip }}</option>
                </select>
              </div>
              <div v-else-if="wsLocalIps.length === 1" class="setting-row setting-inline">
                <label>绑定IP</label>
                <code class="ws-addr" style="margin-left:4px">{{ wsLocalIps[0] }}</code>
              </div>
              <div class="setting-row">
                <div class="setting-actions">
                  <button v-if="!wsRunning" class="chip" :disabled="wsLoading" @click="wsStartServer">
                    {{ wsLoading ? '启动中...' : '启动' }}
                  </button>
                  <button v-else class="chip danger" :disabled="wsLoading" @click="wsStopServer">
                    {{ wsLoading ? '停止中...' : '停止服务' }}
                  </button>
                </div>
              </div>
              <div v-if="wsRunning && wsAddress" class="ws-address-box">
                <span class="ws-label">其它设备输入：</span>
                <code class="ws-addr">{{ wsAddress }}</code>
              </div>
            </template>

            <template v-if="wsMode === 'client'">
              <div class="setting-row setting-inline">
                <label>服务器地址</label>
                <input
                  v-model="wsUrl"
                  class="search compact-input"
                  placeholder="ws://192.168.1.x:9521"
                  style="min-width:180px"
                />
              </div>
              <div v-if="wsLocalIps.length" class="ws-ips">
                <span class="ws-label">本机 IP：</span>
                <code v-for="ip in wsLocalIps" :key="ip" class="ws-addr" style="margin-right:6px">{{ ip }}</code>
              </div>
              <div class="setting-row">
                <div class="setting-actions">
                  <button v-if="!wsRunning" class="chip" :disabled="wsLoading" @click="wsConnectClient">
                    {{ wsLoading ? '连接中...' : '连接' }}
                  </button>
                  <button v-else class="chip danger" :disabled="wsLoading" @click="wsDisconnectClient">
                    {{ wsLoading ? '断开中...' : '断开' }}
                  </button>
                </div>
              </div>
              <div class="ws-status-row">
                <span class="ws-dot" :class="{ connected: wsRunning }"></span>
                <span>{{ wsRunning ? '已连接' : '未连接' }}</span>
              </div>
            </template>
          </div>
        </div>
      </template>

      <p v-if="notice" class="notice">{{ notice }}</p>
    </section>

    <section v-if="page === 'history'" class="history-list">
      <article
        v-for="item in visibleHistory"
        :key="item.id"
        :class="['panel', 'history-item', { copied: copiedItemId === item.id }]"
        @click="copyItem(item, $event)"
      >
        <header>
          <span class="tag" :class="item.type">{{ item.type === "text" ? "文本" : "图片" }}</span>
          <time>{{ formatTime(item.updatedAt) }}</time>
        </header>

        <template v-if="item.type === 'text'">
          <p class="text-preview" :title="item.text || ''">{{ shortText(item.text) }}</p>
        </template>

        <div v-else class="image-preview-wrap">
          <img
            v-if="imagePreviewMap[item.id] || item.imagePreviewDataUrl"
            :src="imagePreviewMap[item.id] || item.imagePreviewDataUrl"
            alt="clipboard image"
            class="image-preview"
          />
          <div v-else class="image-preview-placeholder">加载中...</div>
        </div>

        <div class="history-actions">
          <button
            v-if="item.type === 'text' && isTextTruncated(item.text)"
            class="text-expand-btn"
            @click.stop="openTextPreview(item)"
          >
            展开全文
          </button>
          <button
            class="favorite-toggle"
            :class="{ active: item.isFavorite }"
            @click.stop="toggleFavorite(item)"
          >
            {{ item.isFavorite ? "已收藏" : "收藏" }}
          </button>
          <button class="history-delete-btn" @click.stop="deleteItem(item)">删除</button>
        </div>
      </article>

      <article v-if="visibleHistory.length === 0" class="panel empty">
        <p>当前没有可展示的历史项，复制任意文本或图片后会自动出现。</p>
      </article>
    </section>

    <section v-if="expandedTextItem" class="text-modal-mask" @click="closeTextPreview">
      <article class="panel text-modal" @mousedown.stop @click.stop>
        <header class="text-modal-header">
          <strong>全文预览</strong>
          <button class="text-modal-close" @click="closeTextPreview">关闭</button>
        </header>
        <pre class="text-modal-content">{{ expandedTextItem.text || "" }}</pre>
        <footer class="text-modal-footer">
          <button class="chip" @click="copyExpandedText">复制全文</button>
        </footer>
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
  overflow-x: hidden;
}

html,
#app {
  overflow-x: hidden;
}

* {
  scrollbar-width: thin;
  scrollbar-color: rgba(34, 211, 238, 0.55) rgba(15, 23, 42, 0.35);
}

*::-webkit-scrollbar {
  width: 8px;
  height: 8px;
}

*::-webkit-scrollbar-track {
  background: rgba(15, 23, 42, 0.35);
}

*::-webkit-scrollbar-thumb {
  background: linear-gradient(180deg, rgba(34, 211, 238, 0.75), rgba(14, 165, 233, 0.75));
  border-radius: 999px;
}

.app-shell {
  position: relative;
  min-height: 100vh;
  padding: 8px 8px 10px;
  max-width: 450px;
  margin: 0 auto;
  overflow-x: hidden;
}

.copy-bubble {
  position: fixed;
  z-index: 40;
  pointer-events: none;
  transform: translate(-50%, -115%);
  padding: 6px 12px;
  border-radius: 999px;
  border: 1px solid rgba(255, 255, 255, 0.28);
  background: rgba(14, 165, 233, 0.92);
  color: #0b1020;
  font-size: 12px;
  font-weight: 700;
  box-shadow: 0 8px 22px rgba(14, 165, 233, 0.4);
  animation: copy-bubble-float 0.75s ease forwards;
}

@keyframes copy-bubble-float {
  0% {
    opacity: 0;
    transform: translate(-50%, -95%) scale(0.92);
  }
  20% {
    opacity: 1;
    transform: translate(-50%, -115%) scale(1);
  }
  100% {
    opacity: 0;
    transform: translate(-50%, -155%) scale(0.98);
  }
}

.titlebar {
  height: 34px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding: 0 8px 0 10px;
  margin-bottom: 8px;
  cursor: move;
  user-select: none;
}

.titlebar-name {
  font-size: 12px;
  color: #cbd5e1;
  letter-spacing: 0.3px;
}

.titlebar-btn {
  width: 22px;
  height: 22px;
  border: 1px solid rgba(255, 255, 255, 0.25);
  border-radius: 999px;
  background: rgba(15, 23, 42, 0.25);
  color: #e2e8f0;
  cursor: pointer;
  line-height: 1;
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
  padding: 10px;
  display: grid;
  gap: 8px;
}

.filters,
.actions-row,
.setting-actions {
  display: flex;
  gap: 8px;
  flex-wrap: wrap;
  align-items: center;
}

.filters {
  flex-wrap: nowrap;
  overflow-x: auto;
  padding-bottom: 2px;
}

.filters .chip {
  flex: 0 0 auto;
  white-space: nowrap;
}

.filters {
  gap: 6px;
}

.filters .chip {
  padding: 6px 11px;
  font-size: 12px;
}

.settings-entry {
  margin-left: auto;
}

.top-setting-actions {
  justify-content: flex-end;
}

.bottom-setting-actions {
  justify-content: space-between;
}

.actions-row {
  flex-wrap: nowrap;
}

.actions-row .search {
  flex: 1;
  min-width: 0;
}

.actions-row .chip {
  flex: 0 0 auto;
}

.setting-row {
  display: grid;
  gap: 4px;
}

.settings-compact {
  display: grid;
  gap: 6px;
}

.setting-inline {
  grid-template-columns: 96px minmax(0, 1fr);
  align-items: center;
  column-gap: 8px;
}

.setting-inline .setting-actions {
  flex-wrap: nowrap;
}

.setting-inline .search {
  width: auto;
  flex: 1;
  min-width: 0;
}

.setting-inline .chip {
  flex: 0 0 auto;
}

.setting-pair {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 8px;
}

.setting-row label {
  font-size: 12px;
  color: #cbd5e1;
}

.switch-row {
  display: inline-flex;
  gap: 8px;
  align-items: center;
}

.storage-dir-display {
  width: 100%;
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}

.storage-dir-input {
  cursor: pointer;
}

.storage-dir-row {
  display: flex;
  align-items: center;
  gap: 8px;
}

.storage-dir-row .storage-dir-display {
  flex: 1;
  min-width: 0;
  width: auto;
}

.storage-dir-actions {
  flex-direction: row;
  flex-wrap: nowrap;
}

.storage-dir-actions .chip {
  flex: 0 0 auto;
}

.chip {
  border: 1px solid rgba(255, 255, 255, 0.25);
  background: rgba(15, 23, 42, 0.28);
  color: var(--text-main);
  border-radius: 999px;
  padding: 7px 12px;
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

.chip.danger-confirm {
  background: rgba(220, 38, 38, 0.9);
  border-color: rgba(254, 202, 202, 0.9);
  color: #fff1f2;
  font-weight: 700;
  box-shadow: 0 0 0 2px rgba(220, 38, 38, 0.35);
  animation: danger-pulse 0.9s ease-in-out infinite;
}

@keyframes danger-pulse {
  0% {
    transform: scale(1);
  }
  50% {
    transform: scale(1.02);
  }
  100% {
    transform: scale(1);
  }
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
  padding: 9px 11px;
}

.compact-input {
  padding: 7px 10px;
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
  margin-top: 8px;
  display: grid;
  gap: 8px;
  max-height: calc(100vh - 170px);
  overflow: auto;
  padding-right: 2px;
}

.history-item {
  padding: 10px;
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

.history-item.copied {
  animation: copied-pop 0.26s ease;
  border-color: rgba(34, 211, 238, 0.62);
}

@keyframes copied-pop {
  0% {
    transform: scale(1);
  }
  40% {
    transform: scale(0.985);
  }
  100% {
    transform: scale(1);
  }
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
  user-select: text;
  overflow-wrap: anywhere;
  word-break: break-word;
}

.text-expand-btn {
  margin-top: 0;
  border: 1px solid rgba(255, 255, 255, 0.28);
  border-radius: 999px;
  background: rgba(15, 23, 42, 0.3);
  color: #cbd5e1;
  padding: 5px 10px;
  font-size: 12px;
  cursor: pointer;
}

.text-modal-mask {
  position: fixed;
  inset: 0;
  background: rgba(2, 6, 23, 0.45);
  display: grid;
  place-items: center;
  padding: 12px;
  z-index: 20;
}

.text-modal {
  width: min(100%, 680px);
  max-width: 100%;
  max-height: calc(100vh - 24px);
  padding: 12px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}

.text-modal-header,
.text-modal-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.text-modal-close {
  border: 1px solid rgba(255, 255, 255, 0.25);
  background: rgba(15, 23, 42, 0.3);
  color: #cbd5e1;
  border-radius: 999px;
  padding: 6px 10px;
  cursor: pointer;
}

.text-modal-content {
  margin: 0;
  white-space: pre-wrap;
  word-break: break-word;
  line-height: 1.6;
  color: #e2e8f0;
  background: rgba(15, 23, 42, 0.35);
  border: 1px solid rgba(255, 255, 255, 0.2);
  border-radius: 12px;
  padding: 12px;
  max-height: calc(100vh - 180px);
  overflow: auto;
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

.image-preview-placeholder {
  min-height: 120px;
  display: grid;
  place-items: center;
  color: #94a3b8;
  font-size: 12px;
}

.history-item footer {
  margin-top: 10px;
  font-size: 12px;
  color: #94a3b8;
}

.history-actions {
  margin-top: 10px;
  display: flex;
  align-items: center;
  justify-content: space-between;
  gap: 8px;
}

.favorite-toggle {
  margin-top: 0;
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

.history-delete-btn {
  margin-top: 0;
  border: 1px solid rgba(244, 114, 182, 0.45);
  border-radius: 999px;
  background: rgba(190, 24, 93, 0.25);
  color: #fbcfe8;
  padding: 6px 12px;
  cursor: pointer;
}

.empty {
  padding: 20px;
  text-align: center;
  color: var(--text-soft);
}

@media (max-width: 640px) {
  .app-shell {
    padding: 6px 6px 8px;
  }

  .setting-actions {
    flex-direction: column;
  }

  .setting-inline .setting-actions {
    flex-direction: row;
    flex-wrap: nowrap;
    align-items: center;
  }

  .top-setting-actions,
  .bottom-setting-actions {
    flex-direction: row;
    justify-content: flex-end;
  }

  .bottom-setting-actions {
    justify-content: space-between;
  }

  .setting-inline {
    grid-template-columns: 1fr;
    gap: 4px;
  }

  .setting-pair {
    grid-template-columns: 1fr;
    gap: 6px;
  }

  .storage-dir-row {
    flex-direction: row;
    align-items: center;
  }

  .storage-dir-actions {
    flex-direction: row;
    flex-wrap: nowrap;
    justify-content: flex-end;
  }
}
</style>

<style>
/* WebSocket 局域网共享样式 */
.setting-section {
  margin-top: 12px;
  padding-top: 10px;
  border-top: 1px solid rgba(255, 255, 255, 0.10);
}

.setting-section-title {
  font-size: 12px;
  font-weight: 600;
  color: var(--text-soft);
  margin: 0 0 8px;
  letter-spacing: 0.03em;
}

.storage-stats-row {
  display: flex;
  flex-wrap: wrap;
  gap: 12px;
  font-size: 12px;
  color: var(--text-soft);
  margin-bottom: 12px;
  padding: 8px;
  background: var(--bg-elevated);
  border-radius: 6px;
}

.storage-stat {
  white-space: nowrap;
}

.setting-hint {
  font-size: 11px;
  color: var(--text-soft);
  margin-left: 6px;
}

.ws-mode-btns {
  display: flex;
  gap: 6px;
  flex-wrap: wrap;
}

.ws-address-box,
.ws-ips {
  margin-top: 6px;
  padding: 6px 10px;
  background: rgba(255,255,255,0.05);
  border-radius: 8px;
  font-size: 12px;
  display: flex;
  align-items: center;
  gap: 6px;
  flex-wrap: wrap;
}

.ws-label {
  color: var(--text-soft);
  flex-shrink: 0;
}

.ws-addr {
  color: var(--accent);
  font-family: monospace;
  font-size: 12px;
  word-break: break-all;
}

.ws-status-row {
  display: flex;
  align-items: center;
  gap: 6px;
  margin-top: 6px;
  font-size: 12px;
  color: var(--text-soft);
}

.ws-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  background: rgba(255,255,255,0.25);
  flex-shrink: 0;
}

.ws-dot.connected {
  background: #22d3ee;
  box-shadow: 0 0 6px rgba(34,211,238,0.7);
}
</style>
