<script setup lang="ts">
import { schoolTheme } from '../../features/school-context/theme'
import { useScreen } from '../../app/page-lifecycle'
const screen = useScreen()
import Screen from '../../features/community/Notifications.vue'
</script>
<template>
  <view :style="schoolTheme"><Screen ref="screen" /></view>
</template>
