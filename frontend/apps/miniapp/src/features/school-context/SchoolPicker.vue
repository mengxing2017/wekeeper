<script setup lang="ts">
import { shallowRef, ref, computed } from 'vue'
import { bindSchool, loadSchools, schoolContext, selectSchool } from './context'
import { schoolsApi } from './api'
import { session, restoreSession } from '../identity/session'
import { usePage, confirm } from '../../shared/use-page'
import PageStatus from '../../shared/PageStatus.vue'
const directory = shallowRef<Awaited<ReturnType<typeof schoolsApi.directory>>>([]),
  search = ref('')
const { busy, error, run } = usePage(() => {
  directory.value = []
})
const choices = computed(() =>
  (session.user?.bound_school_id ? schoolContext.schools : directory.value).filter((s) =>
    s.name.includes(search.value),
  ),
)
function load() {
  return run(async (current) => {
    await restoreSession()
    if (!session.user) {
      uni.redirectTo({ url: '/pages/user/login' })
      return
    }
    await loadSchools()
    const list = await schoolsApi.directory()
    if (current()) directory.value = list
  })
}
function choose(id: string, name: string) {
  void run(async () => {
    if (session.user?.bound_school_id) selectSchool(id)
    else {
      if (!(await confirm(`确认绑定 ${name}？绑定后不能自行换校。`))) return
      await bindSchool(id)
    }
    uni.reLaunch({ url: '/pages/index/index' })
  })
}
defineExpose({ show: load })
</script>
<template>
  <view class="page">
    <text class="heading">{{ session.user?.bound_school_id ? '选择学校' : '绑定学校' }}</text>
    <text class="muted">选择浏览范围不会增加学校权限；绑定后不能自行换校。</text>
    <input v-model="search" class="input" placeholder="搜索学校" />
    <PageStatus :busy="busy" :error="error" :empty="!choices.length" @retry="load" />
    <button
      v-for="school in choices"
      :key="school.id"
      class="card"
      :disabled="busy"
      @tap="choose(school.id, school.name)"
    >
      {{ school.name }}
    </button>
  </view>
</template>
