<script setup lang="ts">
import { ref } from 'vue'
import { session, restoreSession, refreshUser } from '../identity/session'
import { identityApi } from '../identity/api'
import { usePage } from '../../shared/use-page'
import PageStatus from '../../shared/PageStatus.vue'
const name = ref('')
const { busy, error, run } = usePage(() => {
  name.value = ''
})
function load() {
  return run(async () => {
    await restoreSession()
    name.value = session.user?.display_name ?? ''
  })
}
function save() {
  void run(async () => {
    if (!name.value.trim()) throw new Error('请填写昵称')
    await identityApi.profile(name.value)
    await refreshUser()
    uni.showToast({ title: '已更新' })
  })
}
defineExpose({ show: load })
</script>
<template>
  <view class="page">
    <text class="heading">个人信息</text>
    <PageStatus :busy="busy" :error="error" @retry="load" />
    <view class="card">
      <text>昵称</text>
      <input v-model="name" maxlength="100" class="input" placeholder="昵称" />
      <button class="primary" :disabled="busy || !session.user" @tap="save">保存</button>
    </view>
  </view>
</template>
