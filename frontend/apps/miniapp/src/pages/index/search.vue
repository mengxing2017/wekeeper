<script setup lang="ts">
import { schoolTheme } from '../../features/school-context/theme'
import { useScreen } from '../../app/page-lifecycle'
const screen = useScreen()
import Screen from '../../features/community/Feed.vue'
import { ref } from 'vue'
import { onLoad } from '@dcloudio/uni-app'
const view = ref<'feed' | 'mine' | 'collect'>('feed')
onLoad((query) => {
  view.value = query?.view === 'mine' ? 'mine' : query?.view === 'collect' ? 'collect' : 'feed'
})
</script>
<template>
  <view :style="schoolTheme"><Screen ref="screen" :initial="view" :search-page="true" /></view>
</template>
