<script setup lang="ts">
import { ref } from 'vue'
import { login, session } from './session'
import { loadSchools } from '../school-context/context'
const accepted = ref(false)
async function submit() {
  if (!accepted.value) return
  try {
    await login()
    if (await loadSchools()) uni.reLaunch({ url: '/pages/index/index' })
    else uni.redirectTo({ url: '/pages/plugin/index' })
  } catch (error) {
    session.error = error instanceof Error ? error.message : '登录或学校加载失败'
  }
}
</script>
<template>
  <view class="page login">
    <text class="brand">WeKeeper</text>
    <text class="heading">欢迎来到校园社区</text>
    <text class="muted">使用微信身份登录，绑定学校后参与校园交流。</text>
    <view class="card">
      <checkbox-group @change="accepted = !accepted">
        <label>
          <checkbox :checked="accepted" />
          我同意使用微信身份登录并选择学校
        </label>
      </checkbox-group>
      <button
        class="primary"
        :disabled="!accepted || session.busy"
        :loading="session.busy"
        @tap="submit"
      >
        微信登录
      </button>
      <text v-if="session.error" class="error">{{ session.error }}</text>
    </view>
  </view>
</template>
