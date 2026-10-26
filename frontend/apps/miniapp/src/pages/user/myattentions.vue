<script setup lang="ts">
import { schoolTheme } from '../../features/school-context/theme'
import { useScreen } from '../../app/page-lifecycle'
const screen = useScreen()
import { ref } from 'vue'
import { onLoad } from '@dcloudio/uni-app'
import Screen from '../../features/community/Following.vue'
const supported = ref(true)
onLoad((query) => {
  supported.value = !query?.type || query.type === '2'
})
</script>
<template>
  <view :style="schoolTheme">
    <Screen ref="screen" v-if="supported" />
    <view v-else class="page">
      <text class="heading">旧版关系明细已下线</text>
      <text class="muted">粉丝及点赞人员列表暂不提供，可在消息中查看互动通知。</text>
      <navigator url="/pages/user/message">查看消息</navigator>
    </view>
  </view>
</template>
