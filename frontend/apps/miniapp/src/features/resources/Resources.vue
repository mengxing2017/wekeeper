<script setup lang="ts">
import { computed, shallowRef } from 'vue'
import { resourcesApi, type SchoolResource } from './api'
import { requireSchool, schoolContext, scopeKey } from '../school-context/context'
import { usePage } from '../../shared/use-page'
import PageStatus from '../../shared/PageStatus.vue'
import PrivateImage from '../media/PrivateImage.vue'
const props = defineProps<{ kind?: 'legal' | 'tag' }>()
const rows = shallowRef<SchoolResource[]>([])
const visible = computed(() =>
  rows.value.filter((r) =>
    props.kind ? r.kind === props.kind : ['banner', 'qrcode', 'service', 'legal'].includes(r.kind),
  ),
)
const { busy, error, run } = usePage(() => {
  rows.value = []
})
function load() {
  return run(async () => {
    if (!(await requireSchool())) return
    const key = scopeKey(),
      result = await resourcesApi.list(schoolContext.id)
    if (key === scopeKey()) rows.value = result
  })
}
function copy(item: SchoolResource) {
  void run(async () => {
    if (!/^https:\/\//i.test(item.url)) throw new Error('暂不支持该链接')
    if (item.kind === 'banner') await resourcesApi.click(schoolContext.id, item.id)
    uni.setClipboardData({ data: item.url })
  })
}
defineExpose({ show: load })
</script>
<template>
  <view class="page">
    <text class="heading">
      {{ props.kind === 'legal' ? '学校说明' : props.kind === 'tag' ? '校园标签' : '校园服务' }}
    </text>
    <view v-for="item in visible" :key="item.id" class="card">
      <text class="title">{{ item.title }}</text>
      <text class="body">{{ item.content }}</text>
      <PrivateImage
        v-if="item.kind === 'qrcode' && item.url"
        :school="schoolContext.id"
        :source="item.url"
      />
      <button v-else-if="item.url" size="mini" :disabled="busy" @tap="copy(item)">复制链接</button>
    </view>
    <PageStatus :busy="busy" :error="error" :empty="!visible.length" @retry="load" />
  </view>
</template>
