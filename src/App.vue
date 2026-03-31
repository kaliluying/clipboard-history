<script setup>
import { computed, onMounted, onUnmounted, ref, watch } from "vue";
import { invoke } from "@tauri-apps/api/core";
import { open } from "@tauri-apps/plugin-dialog";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { listen } from "@tauri-apps/api/event";
import { hide } from "@tauri-apps/api/app";
import HistoryList from "./components/HistoryList.vue";
import SettingsPanel from "./components/SettingsPanel.vue";
import TextPreviewModal from "./components/TextPreviewModal.vue";

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
const searchInput = ref(null);

let timer = null;
let saveSettingsTimer = null;
let copiedItemTimer = null;
let copyBubbleTimer = null;
let clearHistoryConfirmTimer = null;
let isHydratingSettings = true;
let unlistenClipboardSynced = null;
let unlistenWsStatusChanged = null;
let unlistenWsReconnectNeeded = null;
let unlistenFocus = null;

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

const selectedIndex = ref(-1);

watch(keyword, () => {
  selectedIndex.value = -1;
});

let noticeTimer = null;
watch(notice, (newVal) => {
  if (newVal) {
    if (noticeTimer) clearTimeout(noticeTimer);
    noticeTimer = setTimeout(() => {
      notice.value = "";
    }, 3500);
  }
});

function handleGlobalKeydown(e) {
  if (page.value !== "history") return;
  if (isClearHistoryConfirming.value || expandedTextItem.value) return;

  const len = visibleHistory.value.length;
  if (len === 0) return;

  if (e.key === "ArrowDown") {
    e.preventDefault();
    selectedIndex.value = selectedIndex.value < len - 1 ? selectedIndex.value + 1 : len - 1;
  } else if (e.key === "ArrowUp") {
    e.preventDefault();
    selectedIndex.value = selectedIndex.value > 0 ? selectedIndex.value - 1 : 0;
  } else if (e.key === "Enter") {
    if (selectedIndex.value >= 0 && selectedIndex.value < len) {
      e.preventDefault();
      void copyItem(visibleHistory.value[selectedIndex.value]);
    }
  } else if (e.altKey && e.key >= "1" && e.key <= "9") {
    e.preventDefault();
    const index = parseInt(e.key) - 1;
    if (index < len) {
      void copyItem(visibleHistory.value[index]);
    }
  }
}

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
    notice.value = `已重新连接到：${wsUrl.value}`;
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
  const prevImageCount = storageStats.value.imageCount;
  const idx = history.value.findIndex((it) => it.id === item.id);
  if (idx >= 0) history.value.splice(idx, 1);
  history.value.unshift(item);
  // Update storage stats
  updateStorageStats();
  // Update image storage size from backend when new image is added
  if (item.type === 'image' && storageStats.value.imageCount > prevImageCount) {
    loadStorageStats();
  }
}

async function loadStorageStats() {
  try {
    storageStats.value = await invoke("get_storage_stats");
  } catch (e) {
    console.error("load storage stats failed", e);
  }
}

function updateStorageStats() {
  const items = history.value;
  storageStats.value = {
    historyCount: items.length,
    textCount: items.filter(i => i.type === 'text').length,
    imageCount: items.filter(i => i.type === 'image').length,
    favoriteCount: items.filter(i => i.isFavorite).length,
    imageStorageBytes: storageStats.value.imageStorageBytes || 0
  };
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
  // 读取持久化的 WebSocket 配置
  if (settings && typeof settings.wsMode === "string") {
    wsMode.value = settings.wsMode;
  }
  if (settings && typeof settings.wsServerPort === "number") {
    wsPort.value = settings.wsServerPort;
  }
  if (settings && typeof settings.wsClientUrl === "string" && settings.wsClientUrl.trim()) {
    wsUrl.value = settings.wsClientUrl;
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
    await hide();
  } catch (e) {
    try {
      await appWindow.hide();
    } catch (error) {
      console.error("hide window failed", error);
    }
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
  loadStorageStats();
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
        await hideWindow();
        setTimeout(() => { invoke("auto_paste"); }, 50);
        return;
      }
    }

    await invoke("copy_history_item", { id: item.id });
    showCopyFeedback(item.id, event);
    await hideWindow();
    setTimeout(() => { invoke("auto_paste"); }, 50);
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
    closeTextPreview();
    await hideWindow();
    setTimeout(() => { invoke("auto_paste"); }, 50);
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
    const wasImage = item.type === 'image';
    await invoke("delete_history_item", { id: item.id });
    history.value = history.value.filter((it) => it.id !== item.id);
    delete imagePreviewMap.value[item.id];
    delete previewLoadingMap.value[item.id];
    updateStorageStats();
    if (wasImage) {
      loadStorageStats();
    }
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
    updateStorageStats();
    loadStorageStats();
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
        wsMode: wsMode.value,
        wsServerPort: wsPort.value,
        wsClientUrl: wsUrl.value.trim(),
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

  // 监听窗口激活，自动聚焦搜索并重置状态
  unlistenFocus = await appWindow.onFocusChanged(({ payload: focused }) => {
    if (focused && page.value === "history") {
      keyword.value = "";
      filter.value = "all";
      selectedIndex.value = -1;
      setTimeout(() => {
        searchInput.value?.focus();
      }, 50);
    }
  });

  // 加载本机 IP 和 WebSocket 状态
  await loadLocalIps();
  await loadWsStatus();

  window.addEventListener("keydown", handleGlobalKeydown);
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
  window.removeEventListener("keydown", handleGlobalKeydown);
  if (timer !== null) {
    window.clearInterval(timer);
  }
  if (saveSettingsTimer !== null) {
    window.clearTimeout(saveSettingsTimer);
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
  if (unlistenFocus) {
    unlistenFocus();
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
          <div class="search-wrapper">
            <input ref="searchInput" v-model="keyword" class="search" placeholder="搜索文本内容" />
            <button v-show="keyword" class="search-clear-btn" @click="keyword = ''">×</button>
          </div>
        </div>
      </template>

      <template v-else>
        <SettingsPanel
          :shortcutDraft="shortcutDraft"
          :pollIntervalMs="pollIntervalMs"
          :launchAtStartup="launchAtStartup"
          :alwaysOnTop="alwaysOnTop"
          :storageDir="storageDir"
          :deviceName="deviceName"
          :historyLimit="historyLimit"
          :textRetentionDays="textRetentionDays"
          :imageRetentionDays="imageRetentionDays"
          :maxStorageMb="maxStorageMb"
          :storageStats="storageStats"
          :isClearHistoryConfirming="isClearHistoryConfirming"
          :wsMode="wsMode"
          :wsPort="wsPort"
          :wsUrl="wsUrl"
          :wsRunning="wsRunning"
          :wsLoading="wsLoading"
          :wsAddress="wsAddress"
          :wsLocalIps="wsLocalIps"
          :wsSelectedIp="wsSelectedIp"
          @go-history="page = 'history'"
          @update:pollIntervalMs="pollIntervalMs = $event"
          @update:launchAtStartup="launchAtStartup = $event"
          @update:alwaysOnTop="alwaysOnTop = $event"
          @update:deviceName="deviceName = $event"
          @update:historyLimit="historyLimit = $event"
          @update:textRetentionDays="textRetentionDays = $event"
          @update:imageRetentionDays="imageRetentionDays = $event"
          @update:maxStorageMb="maxStorageMb = $event"
          @shortcut-keydown="onShortcutKeydown"
          @shortcut-click="startRecordShortcut"
          @shortcut-blur="onShortcutInputBlur"
          @save-settings="saveSettings"
          @select-storage-dir="selectStorageDir"
          @open-storage-dir="openStorageDir"
          @clear-history="clearAllHistory"
          @update:wsMode="wsMode = $event"
          @update:wsPort="wsPort = $event"
          @update:wsUrl="wsUrl = $event"
          @update:wsSelectedIp="wsSelectedIp = $event"
          @ws-start-server="wsStartServer"
          @ws-stop-server="wsStopServer"
          @ws-connect-client="wsConnectClient"
          @ws-disconnect-client="wsDisconnectClient"
        />
      </template>
    </section>

    <Transition name="toast">
      <div v-if="notice" class="toast-notification">
        {{ notice }}
      </div>
    </Transition>

    <HistoryList
      v-if="page === 'history'"
      :items="visibleHistory"
      :keyword="keyword"
      :totalCount="history.length"
      :copiedItemId="copiedItemId"
      :imagePreviewMap="imagePreviewMap"
      :selectedIndex="selectedIndex"
      @copy-item="copyItem"
      @toggle-favorite="toggleFavorite"
      @delete-item="deleteItem"
      @open-text-preview="openTextPreview"
    />

    <TextPreviewModal
      v-if="expandedTextItem"
      :item="expandedTextItem"
      @close="closeTextPreview"
      @copy="copyExpandedText"
    />
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

html,
body,
#app {
  margin: 0;
  padding: 0;
  background: transparent !important;
  height: 100vh;
  overflow: hidden;
}

body {
  font-family: "Inter", "Segoe UI", "PingFang SC", "Microsoft YaHei", sans-serif;
  color: var(--text-main);
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
  height: 100vh;
  padding: 8px 8px 10px;
  max-width: 450px;
  margin: 0 auto;
  background: linear-gradient(145deg, var(--bg-top), var(--bg-bottom));
  overflow: hidden;
  border-radius: 14px;
  /* Ensure no border clipping artifacts */
  box-sizing: border-box;
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
  border: 1px solid rgba(255, 255, 255, 0.15);
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.05);
  color: #e2e8f0;
  cursor: pointer;
  line-height: 1;
  transition: all 0.2s ease;
  display: grid;
  place-items: center;
}

.titlebar-btn:hover {
  background: rgba(239, 68, 68, 0.8);
  border-color: rgba(248, 113, 113, 1);
  color: white;
  transform: scale(1.05);
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
  background: rgba(255, 255, 255, 0.08); /* slightly softer */
  border: 1px solid rgba(255, 255, 255, 0.15);
  backdrop-filter: blur(24px);
  -webkit-backdrop-filter: blur(24px);
  border-radius: 16px;
  box-shadow: 0 10px 30px rgba(0, 0, 0, 0.2);
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

/* Modern Toggle Switch */
.switch-row input[type="checkbox"] {
  appearance: none;
  width: 38px;
  height: 22px;
  background: rgba(255, 255, 255, 0.15);
  border-radius: 999px;
  position: relative;
  cursor: pointer;
  outline: none;
  transition: background 0.3s ease;
  border: 1px solid rgba(255, 255, 255, 0.1);
}

.switch-row input[type="checkbox"]::after {
  content: '';
  position: absolute;
  top: 2px;
  left: 2px;
  width: 16px;
  height: 16px;
  background: #fff;
  border-radius: 50%;
  transition: transform 0.3s cubic-bezier(0.34, 1.56, 0.64, 1);
  box-shadow: 0 2px 5px rgba(0, 0, 0, 0.2);
}

.switch-row input[type="checkbox"]:checked {
  background: var(--accent-strong);
}

.switch-row input[type="checkbox"]:checked::after {
  transform: translateX(16px);
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
  border: 1px solid rgba(255, 255, 255, 0.15);
  background: rgba(255, 255, 255, 0.07);
  color: var(--text-main);
  border-radius: 999px;
  padding: 7px 14px;
  cursor: pointer;
  transition: all 0.2s ease;
}

.chip:hover {
  background: rgba(255, 255, 255, 0.12);
  transform: translateY(-1px);
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
  border: 1px solid rgba(255, 255, 255, 0.15);
  background: rgba(0, 0, 0, 0.2);
  border-radius: 12px;
  color: var(--text-main);
  padding: 10px 14px;
  transition: all 0.2s ease;
  outline: none;
}

.search:focus {
  border-color: rgba(34, 211, 238, 0.5);
  background: rgba(0, 0, 0, 0.3);
  box-shadow: 0 0 0 3px rgba(34, 211, 238, 0.15);
}

.compact-input {
  padding: 8px 12px;
}

.search::placeholder {
  color: #94a3b8;
}

.search-wrapper {
  position: relative;
  flex: 1;
  min-width: 0;
  display: flex;
}

.search-wrapper .search {
  padding-right: 36px;
}

.search-clear-btn {
  position: absolute;
  right: 10px;
  top: 50%;
  transform: translateY(-50%);
  background: rgba(255, 255, 255, 0.1);
  border: none;
  border-radius: 999px;
  width: 18px;
  height: 18px;
  color: #fff;
  font-size: 14px;
  line-height: 1;
  cursor: pointer;
  display: grid;
  place-items: center;
  padding: 0;
  transition: background 0.2s ease;
}

.search-clear-btn:hover {
  background: rgba(255, 255, 255, 0.25);
}

.toast-notification {
  position: absolute;
  top: 48px;
  left: 50%;
  transform: translateX(-50%);
  background: rgba(15, 23, 42, 0.75);
  backdrop-filter: blur(16px);
  -webkit-backdrop-filter: blur(16px);
  border: 1px solid rgba(255, 255, 255, 0.15);
  color: #e2e8f0;
  padding: 8px 16px;
  border-radius: 999px;
  font-size: 12px;
  z-index: 100;
  box-shadow: 0 4px 20px rgba(0, 0, 0, 0.3);
  white-space: nowrap;
  pointer-events: none;
}

.toast-enter-active,
.toast-leave-active {
  transition: all 0.3s cubic-bezier(0.34, 1.56, 0.64, 1);
}

.toast-enter-from,
.toast-leave-to {
  opacity: 0;
  transform: translate(-50%, -15px) scale(0.95);
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
  padding: 12px;
  cursor: pointer;
  transition: transform 0.2s cubic-bezier(0.34, 1.56, 0.64, 1), border-color 0.2s ease, box-shadow 0.2s ease;
  background: rgba(15, 23, 42, 0.4);
  border-radius: 14px;
  border: 1px solid rgba(255, 255, 255, 0.05);
  box-shadow: 0 4px 15px rgba(2, 6, 23, 0.15);
}

.history-item:hover {
  transform: translateY(-2px) scale(1.005);
  border-color: rgba(34, 211, 238, 0.45);
  box-shadow: 0 8px 25px rgba(2, 6, 23, 0.3);
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
  font-size: 11px;
  padding: 4px 10px;
  font-weight: 600;
  letter-spacing: 0.05em;
  text-transform: uppercase;
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
  transition: opacity 0.2s ease;
}

.history-item:hover time {
  opacity: 0;
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
  border: 1px solid rgba(255, 255, 255, 0.15);
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.05);
  color: #cbd5e1;
  padding: 5px 12px;
  font-size: 11px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s ease;
}

.text-expand-btn:hover {
  background: rgba(255, 255, 255, 0.1);
  color: #fff;
  transform: translateY(-1px);
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
  padding: 16px;
  display: flex;
  flex-direction: column;
  gap: 12px;
  border-radius: 20px;
}

.text-modal-header,
.text-modal-footer {
  display: flex;
  align-items: center;
  justify-content: space-between;
}

.text-modal-close {
  border: 1px solid rgba(255, 255, 255, 0.15);
  background: rgba(255, 255, 255, 0.05);
  color: #cbd5e1;
  border-radius: 999px;
  padding: 6px 14px;
  font-size: 12px;
  font-weight: 500;
  cursor: pointer;
  transition: all 0.2s ease;
}

.text-modal-close:hover {
  background: rgba(255, 255, 255, 0.1);
  color: #fff;
  transform: translateY(-1px);
}

.text-modal-content {
  margin: 0;
  white-space: pre-wrap;
  word-break: break-word;
  line-height: 1.6;
  color: #e2e8f0;
  background: rgba(0, 0, 0, 0.2);
  border: 1px solid rgba(255, 255, 255, 0.1);
  border-radius: 16px;
  padding: 16px;
  max-height: calc(100vh - 180px);
  overflow: auto;
}

.image-preview-wrap {
  width: 100%;
  overflow: hidden;
  border-radius: 12px;
  border: 1px solid rgba(255, 255, 255, 0.15);
  background: rgba(0, 0, 0, 0.15);
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
  border: 1px solid rgba(255, 255, 255, 0.15);
  border-radius: 999px;
  background: rgba(255, 255, 255, 0.05);
  color: #e2e8f0;
  padding: 6px 14px;
  cursor: pointer;
  transition: all 0.2s ease;
}

.favorite-toggle:hover {
  background: rgba(255, 255, 255, 0.1);
}

.favorite-toggle.active {
  background: rgba(251, 191, 36, 0.2);
  border-color: rgba(251, 191, 36, 0.5);
  color: #fde68a;
  box-shadow: 0 2px 8px rgba(251, 191, 36, 0.2);
}

.history-delete-btn {
  margin-top: 0;
  border: 1px solid rgba(244, 114, 182, 0.3);
  border-radius: 999px;
  background: rgba(190, 24, 93, 0.15);
  color: #fbcfe8;
  padding: 6px 14px;
  cursor: pointer;
  transition: all 0.2s ease;
}

.history-delete-btn:hover {
  background: rgba(190, 24, 93, 0.3);
  transform: scale(1.05);
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
  padding: 8px 12px;
  background: rgba(255,255,255,0.05);
  border-radius: 12px;
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
  font-family: "JetBrains Mono", monospace;
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
