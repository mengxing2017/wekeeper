<script setup lang="ts">
import { session, restoreSession, logout } from '../identity/session'
import { schoolContext, loadSchools } from '../school-context/context'
import { usePage, navigate, confirm } from '../../shared/use-page'
import PageStatus from '../../shared/PageStatus.vue'
const { busy, error, run } = usePage()
function load() {
  return run(async () => {
    await restoreSession()
    if (session.user) await loadSchools()
  })
}
function signout() {
  void run(async () => {
    if (!(await confirm('确认退出登录？'))) return
    try {
      await logout()
    } catch {
      uni.showToast({ title: '本机已退出，服务端注销未确认', icon: 'none' })
    }
    uni.reLaunch({ url: '/pages/user/login' })
  })
}
defineExpose({ show: load })
</script>
<template>
  <view class="page">
    <view class="card">
      <image
        src="/static/images/avatar.png"
        style="width: 120rpx; height: 120rpx; border-radius: 50%"
      />
      <text class="heading">{{ session.user?.display_name || '校园成员' }}</text>
      <text class="muted">
        {{ schoolContext.schools.find((s) => s.id === schoolContext.id)?.name || '尚未绑定学校' }}
      </text>
      <button v-if="!session.user" class="primary" @tap="navigate('/pages/user/login')">
        微信登录
      </button>
    </view>
    <PageStatus :busy="busy" :error="error" @retry="load" />
    <view v-if="session.user" class="card menu">
      <button @tap="navigate('/pages/user/userInfo')">个人信息</button>
      <button @tap="navigate('/pages/plugin/index')">学校</button>
      <button @tap="navigate('/pages/index/search?view=mine')">我的发布</button>
      <button @tap="navigate('/pages/index/search?view=collect')">我的收藏</button>
      <button @tap="navigate('/pages/user/attentions')">我的关注</button>
      <button @tap="navigate('/pages/user/message')">消息</button>
      <button @tap="navigate('/pages/index/category')">校园服务</button>
      <button @tap="navigate('/pages/user/legal')">学校说明</button>
      <button @tap="navigate('/pages/user/feedback')">意见反馈</button>
      <button :disabled="busy" @tap="signout">退出登录</button>
    </view>
  </view>
</template>
