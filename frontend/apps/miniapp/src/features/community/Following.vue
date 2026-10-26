<script setup lang="ts">
import { shallowRef } from 'vue'
import { communityApi, type Follow } from './api'
import { schoolContext, requireSchool, scopeKey } from '../school-context/context'
import { usePage, confirm, navigate } from '../../shared/use-page'
import PageStatus from '../../shared/PageStatus.vue'
const rows = shallowRef<Follow[]>([])
const { busy, error, run } = usePage(() => {
  rows.value = []
})
function load() {
  return run(async () => {
    if (!(await requireSchool())) return
    const key = scopeKey(),
      result = await communityApi.follows(schoolContext.id)
    if (key === scopeKey()) rows.value = result
  })
}
function remove(item: Follow) {
  void run(async (current) => {
    if (await confirm(`取消关注 ${item.display_name}？`)) {
      await communityApi.follow(schoolContext.id, item.id, false)
      if (current()) rows.value = rows.value.filter((v) => v.id !== item.id)
    }
  })
}
defineExpose({ show: load })
</script>
<template>
  <view class="page">
    <text class="heading">我的关注</text>
    <button size="mini" @tap="navigate('/pages/index/hot')">查看关注动态</button>
    <view v-for="item in rows" :key="item.id" class="card row spread">
      <text>{{ item.display_name }}</text>
      <button size="mini" :disabled="busy" @tap="remove(item)">取消关注</button>
    </view>
    <PageStatus :busy="busy" :error="error" :empty="!rows.length" @retry="load" />
  </view>
</template>
