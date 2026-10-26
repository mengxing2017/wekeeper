<script setup lang="ts">
import { ref, watch, onUnmounted } from 'vue'
import { imagePath } from './files'
import { onScopeChange, clearScope } from '../school-context/context'
const props = defineProps<{ school: string; source: string }>()
const local = ref(''),
  error = ref(false)
let generation = 0
const stop = onScopeChange(() => {
  generation++
  local.value = ''
  error.value = false
})
onUnmounted(() => {
  generation++
  stop()
})
async function load() {
  const turn = ++generation
  local.value = ''
  error.value = false
  try {
    const path = await imagePath(props.school, props.source)
    if (turn === generation) local.value = path
  } catch (failure) {
    if (turn === generation) {
      if (failure && typeof failure === 'object' && 'status' in failure && failure.status === 403)
        clearScope()
      error.value = true
    }
  }
}
watch(
  () => [props.school, props.source],
  () => void load(),
  { immediate: true },
)
function preview() {
  if (local.value) uni.previewImage({ urls: [local.value] })
}
</script>
<template>
  <image v-if="local" class="post-image" :src="local" mode="widthFix" @tap="preview" />
  <view v-else class="image-placeholder" @tap="load">
    {{ error ? '图片加载失败，点击重试' : '图片加载中…' }}
  </view>
</template>
