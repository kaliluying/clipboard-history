<script setup>
import { ref, watch, nextTick } from 'vue';

const props = defineProps({
  items: { type: Array, required: true },
  copiedItemId: { type: String, default: "" },
  imagePreviewMap: { type: Object, default: () => ({}) },
  selectedIndex: { type: Number, default: -1 }
});

const emit = defineEmits(["copy-item", "toggle-favorite", "delete-item", "open-text-preview"]);

const renderLimit = ref(30);
const itemRefs = ref([]);

watch(() => props.items, () => {
  renderLimit.value = 30; // reset on search or array replace
});

watch(() => props.selectedIndex, async (newVal) => {
  if (newVal >= 0 && newVal < props.items.length) {
    if (newVal >= renderLimit.value) {
      renderLimit.value = newVal + 10;
    }
    await nextTick();
    const el = itemRefs.value[newVal];
    if (el && el.scrollIntoView) {
      el.scrollIntoView({ block: 'nearest', behavior: 'smooth' });
    }
  }
});

function onScroll(e) {
  const { scrollTop, clientHeight, scrollHeight } = e.target;
  if (scrollTop + clientHeight >= scrollHeight - 200) {
    if (renderLimit.value < props.items.length) {
      renderLimit.value += 30;
    }
  }
}

function shortText(text) {
  const source = (text || "").replace(/\s+/g, " ").trim();
  if (source.length <= 120) return source;
  return `${source.slice(0, 120)}...`;
}

function isTextTruncated(text) {
  return (text || "").replace(/\s+/g, " ").trim().length > 120;
}

function formatTime(ms) {
  return new Date(ms).toLocaleString();
}
</script>

<template>
  <section class="history-list" @scroll.passive="onScroll">
    <article
      v-for="(item, index) in items.slice(0, renderLimit)"
      :key="item.id"
      :ref="el => { if(el) itemRefs[index] = el.$el || el }"
      :class="['panel', 'history-item', { copied: copiedItemId === item.id, focused: index === props.selectedIndex }]"
      @click="emit('copy-item', item, $event)"
    >
      <div class="hover-copy-hint">点击复制 {{ index < 9 ? `(Alt+${index + 1})` : '' }}</div>
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
          @click.stop="emit('open-text-preview', item)"
        >
          展开全文
        </button>
        <button
          class="favorite-toggle"
          :class="{ active: item.isFavorite }"
          @click.stop="emit('toggle-favorite', item)"
        >
          {{ item.isFavorite ? "已收藏" : "收藏" }}
        </button>
        <button class="history-delete-btn" @click.stop="emit('delete-item', item)">删除</button>
      </div>
    </article>

    <article v-if="items.length === 0" class="panel empty">
      <svg class="empty-icon" xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round">
        <path d="M16 4h2a2 2 0 0 1 2 2v14a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2h2"></path>
        <rect x="8" y="2" width="8" height="4" rx="1" ry="1"></rect>
        <path d="M9 14h6"></path>
        <path d="M9 10h6"></path>
        <path d="M9 18h4"></path>
      </svg>
      <p>暂无历史记录<br/><span style="font-size: 0.85em; opacity: 0.7">复制任意文本或图片后即刻出现</span></p>
    </article>
  </section>
</template>

<style scoped>
.hover-copy-hint {
  position: absolute;
  top: 12px;
  right: 16px;
  background: rgba(14, 165, 233, 0.9);
  color: white;
  padding: 4px 8px;
  font-size: 11px;
  border-radius: 4px;
  opacity: 0;
  transform: translateY(-5px);
  transition: all 0.2s ease;
  pointer-events: none;
  z-index: 10;
}

.history-item:hover .hover-copy-hint {
  opacity: 1;
  transform: translateY(0);
}

.focused {
  border-color: rgba(34, 211, 238, 0.8) !important;
  box-shadow: 0 0 0 1px rgba(34, 211, 238, 0.3), 0 4px 12px rgba(0, 0, 0, 0.2) !important;
  transform: translateY(-1px);
}

.empty-icon {
  width: 48px;
  height: 48px;
  margin: 0 auto 16px;
  color: rgba(14, 165, 233, 0.5);
  display: block;
}

.panel.empty {
  text-align: center;
  padding: 40px 20px;
  display: flex;
  flex-direction: column;
  justify-content: center;
  align-items: center;
}
</style>
