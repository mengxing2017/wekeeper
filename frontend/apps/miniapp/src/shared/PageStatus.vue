<script setup lang="ts">
defineProps<{ busy?: boolean; error?: string; empty?: boolean }>()
defineEmits<{ retry: [] }>()
</script>
<template>
  <view v-if="busy" class="status">正在加载…</view>
  <view v-else-if="error" class="status error">
    <text>{{ error }}</text>
    <button size="mini" @tap="$emit('retry')">重试</button>
  </view>
  <view v-else-if="empty" class="status">暂无内容</view>
</template>
