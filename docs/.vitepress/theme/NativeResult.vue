<script setup lang="ts">
defineProps<{
  src: string
  alt: string
  caption?: string
}>()
</script>

<template>
  <figure class="native-result">
    <div class="native-result__bar">
      <span>實際原生輸出</span>
    </div>
    <img :src="src" :alt="alt" loading="lazy" />
    <figcaption v-if="caption">{{ caption }}</figcaption>
  </figure>
</template>

<style scoped>
.native-result {
  margin: 18px 0 24px;
  overflow: hidden;
  border: 1px solid var(--vp-c-divider);
  border-radius: 12px;
  background: #020617;
  box-shadow: 0 12px 32px rgb(0 0 0 / 8%);
}

.native-result__bar {
  display: flex;
  align-items: center;
  gap: 12px;
  min-height: 35px;
  padding: 0 12px;
  border-bottom: 1px solid rgb(148 163 184 / 20%);
  color: #94a3b8;
  font-size: 12px;
  font-weight: 600;
  letter-spacing: 0.02em;
}

.native-result img {
  display: block;
  width: 100%;
  height: auto;
  margin: 0;
  border-radius: 0;
}

.native-result figcaption {
  margin: 0;
  padding: 10px 13px 12px;
  border-top: 1px solid rgb(148 163 184 / 16%);
  color: #94a3b8;
  font-size: 12px;
  line-height: 1.6;
}
</style>
