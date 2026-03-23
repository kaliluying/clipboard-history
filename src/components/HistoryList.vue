<script setup>
defineProps({
  items: { type: Array, required: true },
  copiedItemId: { type: String, default: "" },
  imagePreviewMap: { type: Object, default: () => ({}) },
});

const emit = defineEmits(["copy-item", "toggle-favorite", "delete-item", "open-text-preview"]);

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
  <section class="history-list">
    <article
      v-for="item in items"
      :key="item.id"
      :class="['panel', 'history-item', { copied: copiedItemId === item.id }]"
      @click="emit('copy-item', item, $event)"
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
      <p>当前没有可展示的历史项，复制任意文本或图片后会自动出现。</p>
    </article>
  </section>
</template>
