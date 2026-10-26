<script setup lang="ts">
import { ref } from 'vue'
import { communityApi, type Post } from './api'
import { schoolContext, requireSchool, scopeKey, onScopeChange } from '../school-context/context'
import { session } from '../identity/session'
import { uploadImage } from '../media/files'
import PrivateImage from '../media/PrivateImage.vue'
import { usePage, confirm } from '../../shared/use-page'
import PageStatus from '../../shared/PageStatus.vue'
import { onUnmounted } from 'vue'
const props = defineProps<{ id?: string; school?: string }>()
const title = ref(''),
  content = ref(''),
  tags = ref(''),
  images = ref<string[]>([]),
  revision = ref<number>(),
  loaded = ref(''),
  message = ref('')
const { busy, error, run } = usePage(() => {
  title.value = ''
  content.value = ''
  tags.value = ''
  images.value = []
  revision.value = undefined
  loaded.value = ''
  message.value = ''
})
const draftKey = () => `wekeeper:draft:${session.user?.id}:${schoolContext.id}:${props.id ?? 'new'}`
function saveDraft() {
  if (
    !session.user ||
    !schoolContext.id ||
    (!title.value.trim() && !content.value.trim() && !images.value.length)
  )
    return
  uni.setStorageSync(draftKey(), {
    title: title.value,
    content: content.value,
    tags: tags.value,
    images: images.value,
    revision: revision.value,
  })
}
const stop = onScopeChange(() => {
  loaded.value = ''
})
onUnmounted(stop)
function load() {
  return run(async () => {
    if (!(await requireSchool(props.school))) return
    const key = scopeKey()
    if (loaded.value === key) return
    let post: Post | undefined
    if (props.id) post = await communityApi.post(schoolContext.id, props.id)
    if (key !== scopeKey()) return
    if (post) {
      if (post.author_id !== session.user?.id) throw new Error('只能编辑自己的动态')
      title.value = post.title
      content.value = post.content
      tags.value = post.tags.join(',')
      images.value = [...post.images]
      revision.value = post.revision
    }
    const draft: unknown = uni.getStorageSync(draftKey())
    if (
      draft &&
      typeof draft === 'object' &&
      (await confirm('发现当前账号在本校的草稿，是否恢复？'))
    ) {
      if (key !== scopeKey()) return
      const v = draft as Record<string, unknown>
      title.value = typeof v.title === 'string' ? v.title : ''
      content.value = typeof v.content === 'string' ? v.content : ''
      tags.value = typeof v.tags === 'string' ? v.tags : ''
      images.value = Array.isArray(v.images)
        ? v.images.filter(
            (x): x is string =>
              typeof x === 'string' && x.startsWith(`/api/v1/schools/${schoolContext.id}/media/`),
          )
        : []
      // An edit draft must still refer to the current revision. Conflicts require a fresh read.
      if (post && v.revision !== post.revision) {
        images.value = [...post.images]
        throw new Error('帖子已更新，请核对恢复的正文后再提交')
      }
    }
    loaded.value = key
  })
}

function choose() {
  void run(async () => {
    if (!(await requireSchool(props.school))) return
    if (images.value.length >= 9) throw new Error('最多选择 9 张图片')
    const key = scopeKey(),
      s = schoolContext.id
    const result = await uni.chooseImage({
      count: 9 - images.value.length,
      sizeType: ['compressed'],
      sourceType: ['album', 'camera'],
    })
    for (const path of result.tempFilePaths) {
      if (key !== scopeKey()) return
      const media = await uploadImage(s, path)
      if (key !== scopeKey()) return
      images.value.push(media.url)
      saveDraft()
    }
  })
}
function submit() {
  void run(async () => {
    if (!title.value.trim() || !content.value.trim()) throw new Error('请填写标题和正文')
    if (!loaded.value || loaded.value !== scopeKey()) throw new Error('请先加载当前学校的编辑页面')
    const s = schoolContext.id,
      key = scopeKey(),
      draft = draftKey()
    saveDraft()
    const result = await communityApi.save(
      s,
      {
        title: title.value,
        content: content.value,
        images: [...images.value],
        tags: tags.value
          .split(/[,，]/)
          .map((v) => v.trim())
          .filter(Boolean),
        revision: revision.value,
      },
      props.id,
    )
    uni.removeStorageSync(draft)
    if (key !== scopeKey()) return
    message.value = result.audit_status === 'approved' ? '发布成功，已通过审核' : '已提交，等待审核'
    title.value = ''
    content.value = ''
    tags.value = ''
    images.value = []
    revision.value = result.revision
    uni.showModal({
      title: '提交成功',
      content: message.value,
      showCancel: false,
      success: () => uni.redirectTo({ url: `/pages/index/detail?school=${s}&id=${result.id}` }),
    })
  })
}
function discard() {
  void run(async () => {
    if (await confirm('确认丢弃当前草稿？')) {
      uni.removeStorageSync(draftKey())
      title.value = ''
      content.value = ''
      tags.value = ''
      images.value = []
    }
  })
}
// Scope changes clear in-memory fields; drafts are explicitly saved under account + school.
defineExpose({ show: load, hide: saveDraft })
</script>
<template>
  <view class="page">
    <text class="heading">{{ props.id ? '编辑动态' : '分享校园生活' }}</text>
    <text class="muted">
      {{ schoolContext.schools.find((s) => s.id === schoolContext.id)?.name }}
    </text>
    <PageStatus :busy="busy" :error="error" @retry="load" />
    <view class="card">
      <input v-model="title" class="input" maxlength="200" placeholder="标题（必填）" />
      <textarea
        v-model="content"
        class="input editor"
        maxlength="20000"
        placeholder="分享你的校园故事…"
      />
      <input v-model="tags" class="input" placeholder="标签，使用逗号分隔" />
      <view class="image-grid">
        <view v-for="(src, i) in images" :key="src">
          <PrivateImage :school="schoolContext.id" :source="src" />
          <button size="mini" :disabled="busy" @tap="images.splice(i, 1)">移除</button>
        </view>
      </view>
      <button :disabled="busy || images.length >= 9" @tap="choose">
        选择图片（{{ images.length }}/9）
      </button>
      <view class="row">
        <button size="mini" :disabled="busy" @tap="saveDraft">保存本地草稿</button>
        <button size="mini" :disabled="busy" @tap="discard">丢弃草稿</button>
      </view>
      <button class="primary" :disabled="busy" :loading="busy" @tap="submit">提交审核</button>
      <text class="muted">网络中断时请先在“我的发布”核对结果，再决定是否重新提交。</text>
    </view>
  </view>
</template>
