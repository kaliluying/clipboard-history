<script setup>
defineProps({
  item: { type: Object, required: true },
});

const emit = defineEmits(["close", "copy"]);
</script>

<template>
  <section class="text-modal-mask" @click="emit('close')">
    <article class="panel text-modal" @mousedown.stop @click.stop>
      <header class="text-modal-header">
        <strong>全文预览</strong>
        <button class="text-modal-close" @click="emit('close')">关闭</button>
      </header>
      <pre class="text-modal-content">{{ item.text || "" }}</pre>
      <footer class="text-modal-footer">
        <button class="chip" @click="emit('copy')">复制全文</button>
      </footer>
    </article>
  </section>
</template>
