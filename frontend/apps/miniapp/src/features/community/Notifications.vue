<script setup lang="ts">
import { shallowRef, ref } from 'vue'
import { communityApi, type Notification } from './api'
import { schoolContext, requireSchool, scopeKey } from '../school-context/context'
import { usePage } from '../../shared/use-page'
import PageStatus from '../../shared/PageStatus.vue'
const rows = shallowRef<Notification[]>([]),
  page = ref(1),
  more = ref(false)
const labels: Record<string, string> = {
  like: '收到点赞',
  comment: '收到评论',
  follow: '有人关注了你',
}
const { busy, error, run } = usePage(() => {
  rows.value = []
  page.value = 1
  more.value = false
})
function load(reset = true) {
  return run(async () => {
    if (!(await requireSchool())) return
    const key = scopeKey(),
      p = reset ? 1 : page.value + 1
    const result = await communityApi.notifications(schoolContext.id, p)
    if (key !== scopeKey()) return
    rows.value = reset ? result.items : [...rows.value, ...result.items]
    page.value = p
    more.value = !!result.page?.has_more
  })
}
function read(item: Notification) {
  void run(async (current) => {
    await communityApi.read(schoolContext.id, item.id)
    if (current())
      rows.value = rows.value.map((row) =>
        row.id === item.id ? { ...row, read_at: new Date().toISOString() } : row,
      )
  })
}
defineExpose({
  show: load,
  loadMore: () => {
    if (more.value) return load(false)
  },
})
</script>
<template>
  <view class="page">
    <text class="heading">消息</text>
    <view v-for="item in rows" :key="item.id" class="card">
      <text class="title">{{ labels[item.kind] || '校园消息' }}</text>
      <text class="muted">{{ item.created_at }}</text>
      <button size="mini" :disabled="busy || !!item.read_at" @tap="read(item)">
        {{ item.read_at ? '已读' : '标记已读' }}
      </button>
    </view>
    <PageStatus :busy="busy" :error="error" :empty="!rows.length" @retry="load()" />
    <button v-if="more" :disabled="busy" @tap="load(false)">更多消息</button>
  </view>
</template>
