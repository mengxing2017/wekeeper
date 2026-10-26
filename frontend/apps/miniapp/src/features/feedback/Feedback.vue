<script setup lang="ts">
import { ref } from 'vue'
import { uploadImage } from '../media/files'
import PrivateImage from '../media/PrivateImage.vue'
import { feedbackApi } from './api'
import { requireSchool, schoolContext, scopeKey } from '../school-context/context'
import { usePage } from '../../shared/use-page'
import PageStatus from '../../shared/PageStatus.vue'
const content = ref(''),
  contact = ref(''),
  submitted = ref(false),
  images = ref<string[]>([])
const { busy, error, run } = usePage(() => {
  content.value = ''
  contact.value = ''
  submitted.value = false
  images.value = []
})
function load() {
  return run(async () => {
    await requireSchool()
  })
}
function submit() {
  void run(async () => {
    if (!content.value.trim()) throw new Error('请填写反馈内容')
    await feedbackApi.submit(schoolContext.id, content.value, contact.value, [...images.value])
    images.value = []
    content.value = ''
    contact.value = ''
    submitted.value = true
  })
}
function choose() {
  void run(async () => {
    if (!(await requireSchool())) return
    const key = scopeKey(),
      s = schoolContext.id
    const result = await uni.chooseImage({
      count: 9 - images.value.length,
      sizeType: ['compressed'],
    })
    for (const path of result.tempFilePaths) {
      if (key !== scopeKey()) return
      const media = await uploadImage(s, path)
      if (key !== scopeKey()) return
      images.value.push(media.url)
    }
  })
}
defineExpose({ show: load })
</script>
<template>
  <view class="page">
    <text class="heading">意见反馈</text>
    <PageStatus :busy="busy" :error="error" @retry="load" />
    <view class="card">
      <textarea
        v-model="content"
        class="input editor"
        maxlength="500"
        placeholder="请描述你的意见或遇到的问题"
      />
      <input v-model="contact" class="input" maxlength="200" placeholder="联系方式（选填）" />
      <view class="image-grid">
        <view v-for="(src, i) in images" :key="src">
          <PrivateImage :school="schoolContext.id" :source="src" />
          <button size="mini" :disabled="busy" @tap="images.splice(i, 1)">移除</button>
        </view>
      </view>
      <button :disabled="busy || images.length >= 9" @tap="choose">
        添加图片（{{ images.length }}/9）
      </button>
      <button class="primary" :disabled="busy" @tap="submit">提交反馈</button>
      <text v-if="submitted" class="muted">已提交，感谢你的反馈。</text>
    </view>
  </view>
</template>
