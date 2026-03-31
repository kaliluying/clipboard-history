<script setup>
import { ref, computed } from 'vue';

const isMac = computed(() => typeof navigator !== 'undefined' && /Mac|iPhone|iPod|iPad/.test(navigator.platform));

const props = defineProps({
  // 基本设置
  shortcutDraft: { type: String, required: true },
  pollIntervalMs: { type: Number, required: true },
  launchAtStartup: { type: Boolean, required: true },
  alwaysOnTop: { type: Boolean, required: true },
  storageDir: { type: String, default: "" },
  deviceName: { type: String, default: "" },
  historyLimit: { type: Number, required: true },
  textRetentionDays: { type: Number, required: true },
  imageRetentionDays: { type: Number, required: true },
  maxStorageMb: { type: Number, required: true },
  storageStats: { type: Object, required: true },
  isClearHistoryConfirming: { type: Boolean, required: true },
  // WebSocket
  wsMode: { type: String, required: true },
  wsPort: { type: Number, required: true },
  wsUrl: { type: String, default: "" },
  wsRunning: { type: Boolean, required: true },
  wsLoading: { type: Boolean, required: true },
  wsAddress: { type: String, default: "" },
  wsLocalIps: { type: Array, default: () => [] },
  wsSelectedIp: { type: String, default: "" },
});

const emit = defineEmits([
  // 导航
  "go-history",
  // 设置字段变更
  "update:pollIntervalMs",
  "update:launchAtStartup",
  "update:alwaysOnTop",
  "update:deviceName",
  "update:historyLimit",
  "update:textRetentionDays",
  "update:imageRetentionDays",
  "update:maxStorageMb",
  // 操作
  "shortcut-keydown",
  "shortcut-click",
  "shortcut-blur",
  "save-settings",
  "select-storage-dir",
  "open-storage-dir",
  "clear-history",
  // WebSocket
  "update:wsMode",
  "update:wsPort",
  "update:wsUrl",
  "update:wsSelectedIp",
  "ws-start-server",
  "ws-stop-server",
  "ws-connect-client",
  "ws-disconnect-client",
]);

const activeTab = ref('general');

function formatBytes(bytes) {
  if (bytes === 0) return "0 B";
  const k = 1024;
  const sizes = ["B", "KB", "MB", "GB"];
  const i = Math.floor(Math.log(bytes) / Math.log(k));
  return parseFloat((bytes / Math.pow(k, i)).toFixed(1)) + " " + sizes[i];
}

const displayShortcut = computed(() => {
  if (!props.shortcutDraft) return "";
  let res = props.shortcutDraft;
  if (isMac.value) {
    res = res.replace(/Super/g, "⌘ Command")
             .replace(/Alt/g, "⌥ Option")
             .replace(/Shift/g, "⇧ Shift")
             .replace(/Ctrl/g, "⌃ Control");
  } else {
    res = res.replace(/Super/g, "Win");
  }
  return res;
});
</script>

<template>
  <div class="settings-compact">
    <div class="setting-actions top-setting-actions" style="margin-bottom: 16px; display: flex; align-items: center;">
      <button class="chip" @click="emit('go-history')">⟵ 返回</button>
      <div class="tab-controls" style="display: flex; gap: 6px; margin-left: 12px; background: rgba(0,0,0,0.25); padding: 4px; border-radius: 8px;">
        <button class="chip" :class="{ active: activeTab === 'general' }" @click="activeTab = 'general'">常规</button>
        <button class="chip" :class="{ active: activeTab === 'storage' }" @click="activeTab = 'storage'">回收策略</button>
        <button class="chip" :class="{ active: activeTab === 'network' }" @click="activeTab = 'network'">局域网共享</button>
      </div>
    </div>

    <!-- 常规设置 Tab -->
    <div v-show="activeTab === 'general'" class="settings-tab-content">
      <div class="setting-row setting-inline">
        <label>全局快捷键</label>
        <div class="setting-actions">
          <input
            class="search compact-input"
            :value="displayShortcut"
            readonly
            @keydown="emit('shortcut-keydown', $event)"
            @click="emit('shortcut-click')"
            @blur="emit('shortcut-blur')"
            placeholder="点击输入框后按下组合键"
          />
        </div>
      </div>

      <div class="setting-row setting-inline">
        <label>轮询间隔(ms)</label>
        <input
          :value="pollIntervalMs"
          class="search compact-input"
          type="number"
          min="300"
          max="5000"
          @input="emit('update:pollIntervalMs', Number($event.target.value))"
        />
      </div>

      <div class="setting-pair">
        <div class="setting-row setting-inline">
          <label>开机自启</label>
          <label class="switch-row">
            <input
              :checked="launchAtStartup"
              type="checkbox"
              @change="emit('update:launchAtStartup', $event.target.checked)"
            />
            <span>{{ launchAtStartup ? "已启用" : "未启用" }}</span>
          </label>
        </div>

        <div class="setting-row setting-inline">
          <label>窗口置顶</label>
          <label class="switch-row">
            <input
              :checked="alwaysOnTop"
              type="checkbox"
              @change="emit('update:alwaysOnTop', $event.target.checked)"
            />
            <span>{{ alwaysOnTop ? "已启用" : "未启用" }}</span>
          </label>
        </div>
      </div>

      <div class="setting-row">
        <label>数据存储目录</label>
        <div class="storage-dir-row">
          <div class="storage-dir-display" :title="storageDir || '默认应用数据目录'">
            <input
              class="search compact-input storage-dir-input"
              :value="storageDir || '默认应用数据目录'"
              readonly
              @click="emit('select-storage-dir')"
              title="点击选择保存位置"
            />
          </div>
          <div class="setting-actions storage-dir-actions">
            <button class="chip" @click="emit('open-storage-dir')">打开目录</button>
          </div>
        </div>
      </div>
    </div>

    <!-- 回收策略 Tab -->
    <div v-show="activeTab === 'storage'" class="settings-tab-content">
      <div class="setting-section">
        <h3 class="setting-section-title">📊 实时存储统计</h3>
        <div class="storage-stats-row">
          <span class="storage-stat">总计: {{ storageStats.historyCount || 0 }}</span>
          <span class="storage-stat">文本: {{ storageStats.textCount || 0 }}</span>
          <span class="storage-stat">图片: {{ storageStats.imageCount || 0 }}</span>
          <span class="storage-stat">收藏: {{ storageStats.favoriteCount || 0 }}</span>
          <span class="storage-stat">图片体积: {{ formatBytes(storageStats.imageStorageBytes || 0) }}</span>
        </div>

        <br>
        <h3 class="setting-section-title">⚙️ 自动清理限制</h3>
        <div class="setting-row setting-inline">
          <label>历史记录总量</label>
          <input
            :value="historyLimit"
            class="search compact-input"
            type="number"
            min="50"
            max="5000"
            style="width:80px"
            @input="emit('update:historyLimit', Number($event.target.value))"
          />
        </div>

        <div class="setting-row setting-inline">
          <label>文本保留天数</label>
          <input
            :value="textRetentionDays"
            class="search compact-input"
            type="number"
            min="0"
            max="365"
            style="width:80px"
            @input="emit('update:textRetentionDays', Number($event.target.value))"
          />
          <span class="setting-hint">0=不限制</span>
        </div>

        <div class="setting-row setting-inline">
          <label>图片保留天数</label>
          <input
            :value="imageRetentionDays"
            class="search compact-input"
            type="number"
            min="0"
            max="365"
            style="width:80px"
            @input="emit('update:imageRetentionDays', Number($event.target.value))"
          />
          <span class="setting-hint">0=不限制</span>
        </div>

        <div class="setting-row setting-inline">
          <label>图片存储上限(MB)</label>
          <input
            :value="maxStorageMb"
            class="search compact-input"
            type="number"
            min="0"
            max="10000"
            style="width:80px"
            @input="emit('update:maxStorageMb', Number($event.target.value))"
          />
          <span class="setting-hint">0=不限制</span>
        </div>

        <div class="setting-actions bottom-setting-actions" style="margin-top: 24px;">
          <button
            class="chip danger"
            :class="{ 'danger-confirm': isClearHistoryConfirming }"
            @click="emit('clear-history')"
          >
            {{ isClearHistoryConfirming ? "再次点击确认删除" : "清空全部" }}
          </button>
          <button class="chip" @click="emit('save-settings')">立即生效并清理</button>
        </div>
      </div>
    </div>

    <!-- 局域网同步 Tab -->
    <div v-show="activeTab === 'network'" class="settings-tab-content">
      <div class="setting-section">
        <h3 class="setting-section-title">📶 WebSocket 设备互通</h3>

        <div class="setting-row setting-inline">
          <label>识别名称</label>
          <input
            :value="deviceName"
            class="search compact-input"
            placeholder="例如: MacBook Pro"
            style="min-width:120px"
            @input="emit('update:deviceName', $event.target.value)"
          />
        </div>
        <hr style="border:0; border-top: 1px solid rgba(255,255,255,0.05); margin: 16px 0;" />

        <div class="setting-row setting-inline">
          <label>运行模式</label>
          <div class="ws-mode-btns">
            <button :class="['chip', { active: wsMode === 'disabled' }]" @click="emit('update:wsMode', 'disabled')">未开启</button>
            <button :class="['chip', { active: wsMode === 'server' }]" @click="emit('update:wsMode', 'server')">作为总机</button>
            <button :class="['chip', { active: wsMode === 'client' }]" @click="emit('update:wsMode', 'client')">连接总机</button>
          </div>
        </div>

        <template v-if="wsMode === 'server'">
          <div class="setting-row setting-inline">
            <label>通讯端口</label>
            <input
              :value="wsPort"
              class="search compact-input"
              type="number"
              min="1024"
              max="65535"
              style="width:90px"
              @input="emit('update:wsPort', Number($event.target.value))"
            />
          </div>
          <div v-if="wsLocalIps.length > 1" class="setting-row setting-inline">
            <label>内网 IP</label>
            <select
              :value="wsSelectedIp"
              class="search compact-input"
              style="min-width:150px"
              @change="emit('update:wsSelectedIp', $event.target.value)"
            >
              <option v-for="ip in wsLocalIps" :key="ip" :value="ip">{{ ip }}</option>
            </select>
          </div>
          <div v-else-if="wsLocalIps.length === 1" class="setting-row setting-inline">
            <label>内网 IP</label>
            <code class="ws-addr" style="margin-left:4px">{{ wsLocalIps[0] }}</code>
          </div>
          <div class="setting-row">
            <div class="setting-actions">
              <button v-if="!wsRunning" class="chip" :disabled="wsLoading" @click="emit('ws-start-server')">
                {{ wsLoading ? '正在启动...' : '开启广播服务' }}
              </button>
              <button v-else class="chip danger" :disabled="wsLoading" @click="emit('ws-stop-server')">
                {{ wsLoading ? '正在关闭...' : '关闭广播服务' }}
              </button>
            </div>
          </div>
          <div v-if="wsRunning && wsAddress" class="ws-address-box">
            <span class="ws-label">分机配对地址：</span>
            <code class="ws-addr">{{ wsAddress }}</code>
          </div>
        </template>

        <template v-if="wsMode === 'client'">
          <div class="setting-row setting-inline">
            <label>总机地址</label>
            <input
              :value="wsUrl"
              class="search compact-input"
              placeholder="ws://192.168.1.x:9521"
              style="min-width:180px"
              @input="emit('update:wsUrl', $event.target.value)"
            />
          </div>
          <div v-if="wsLocalIps.length" class="ws-ips" style="margin-bottom: 12px; opacity: 0.7;">
            <span class="ws-label">本机 IP：</span>
            <code v-for="ip in wsLocalIps" :key="ip" class="ws-addr" style="margin-right:6px">{{ ip }}</code>
          </div>
          <div class="setting-row">
            <div class="setting-actions" style="margin-top: 8px;">
              <button v-if="!wsRunning" class="chip" :disabled="wsLoading" @click="emit('ws-connect-client')">
                {{ wsLoading ? '连接中...' : '发起连接' }}
              </button>
              <button v-else class="chip danger" :disabled="wsLoading" @click="emit('ws-disconnect-client')">
                {{ wsLoading ? '断开中...' : '主动断开' }}
              </button>
            </div>
          </div>
          <div class="ws-status-row">
            <span class="ws-dot" :class="{ connected: wsRunning }"></span>
            <span>{{ wsRunning ? '已接入服务' : '休眠中 / 离线' }}</span>
          </div>
        </template>
      </div>
    </div>
  </div>
</template>
